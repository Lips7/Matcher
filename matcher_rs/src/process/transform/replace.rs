//! Text-replacement engines for Fanjian, Pinyin, and Normalize transformations.
//!
//! All three engines share a common pattern: scan the input for replacement
//! spans, then rebuild the output by interleaving unchanged text with
//! replacement strings.
//!
//! ## Fanjian and Pinyin — page-table engines
//!
//! Both use a **two-stage page table** for O(1) codepoint lookup:
//!
//! - **L1** (`&[u16]`): indexed by `codepoint >> 8` (the "page index"). A non-zero
//!   value is the 1-based page number in L2; zero means the entire 256-codepoint
//!   page has no mappings.
//! - **L2** (`&[u32]`): indexed by `page * 256 + (codepoint & 0xFF)`. The
//!   interpretation of the stored `u32` depends on the engine:
//!   - **Fanjian**: the simplified codepoint value. `0` = unmapped.
//!   - **Pinyin**: packed `(byte_offset << 8) | byte_length` into a shared
//!     string buffer. `0` = unmapped.
//!
//! Scan loops use [`skip_ascii_simd`] to
//! fast-forward over ASCII bytes that cannot produce hits, falling through to
//! the page-table probe only for multi-byte (non-ASCII) codepoints.
//!
//! ## Normalize — Aho-Corasick engine
//!
//! [`NormalizeMatcher`] performs multi-character replacement (full-width to
//! half-width, variant forms, number normalization, etc.) using leftmost-longest
//! Aho-Corasick matching. The automaton scans the text once, finds all
//! non-overlapping matches, and rebuilds the output by interleaving unchanged
//! spans with replacements from a parallel lookup table.

#[cfg(feature = "runtime_build")]
use ahash::{AHashMap, AHashSet};
use std::borrow::Cow;

use aho_corasick::{
    AhoCorasick, AhoCorasickBuilder, AhoCorasickKind, MatchKind as AhoCorasickMatchKind,
};

use crate::process::string_pool::get_string_from_pool;
use crate::process::transform::simd::skip_ascii_simd;
use crate::process::transform::utf8::decode_utf8_raw;

// ---------------------------------------------------------------------------
// Shared replacement helpers
// ---------------------------------------------------------------------------

/// Shared scan-and-rebuild helper for Fanjian replacement.
///
/// Pulls the first item from `iter`; if `None`, returns `None` (no replacements needed).
/// Otherwise allocates a [`String`] from the thread-local pool, copies unchanged text
/// between replacement spans, and pushes each replacement `char`.
///
/// The caller is responsible for ensuring the iterator yields spans in strictly
/// ascending, non-overlapping byte-offset order; otherwise the interleaved
/// `push_str` calls will produce garbled output.
#[inline(always)]
fn replace_scan<I>(text: &str, mut iter: I) -> Option<String>
where
    I: Iterator<Item = (usize, usize, char)>,
{
    if let Some((start, end, ch)) = iter.next() {
        let mut result = get_string_from_pool(text.len());
        result.push_str(&text[..start]);
        result.push(ch);
        let mut last_end = end;
        for (start, end, ch) in iter {
            result.push_str(&text[last_end..start]);
            result.push(ch);
            last_end = end;
        }
        result.push_str(&text[last_end..]);
        Some(result)
    } else {
        None
    }
}

/// Like [`replace_scan`] but also returns whether the output is pure ASCII.
///
/// Accepts `(start, end, replacement_str)` triples where each replacement is a `&str`
/// pushed directly into the result. `is_ascii` is computed via `result.is_ascii()` after
/// the string is fully assembled (single pass, equivalent cost to a SIMD density scan).
///
/// Used by [`PinyinMatcher::replace`] and [`NormalizeMatcher::replace`].
#[inline(always)]
fn replace_spans_tracking_ascii<'a, I>(text: &str, mut iter: I) -> Option<(String, bool)>
where
    I: Iterator<Item = (usize, usize, &'a str)>,
{
    if let Some((start, end, replacement)) = iter.next() {
        let mut result = get_string_from_pool(text.len());
        result.push_str(&text[..start]);
        result.push_str(replacement);
        let mut last_end = end;
        for (start, end, replacement) in iter {
            result.push_str(&text[last_end..start]);
            result.push_str(replacement);
            last_end = end;
        }
        result.push_str(&text[last_end..]);
        let is_ascii = result.is_ascii();
        Some((result, is_ascii))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Shared page-table helpers (Fanjian + Pinyin)
// ---------------------------------------------------------------------------

/// Looks up one codepoint in a two-stage page table.
///
/// Returns `Some(value)` when the codepoint has a non-zero entry in L2, or
/// `None` when the page is unmapped (L1 entry is zero), the page index is out
/// of range, or the L2 value is zero.
///
/// # Safety
///
/// Uses `get_unchecked` in two places:
///
/// - **L1 access** (`l1[page_idx]`): guarded by the `page_idx >= l1.len()`
///   bounds check immediately above.
/// - **L2 access** (`l2[page * 256 + char_idx]`): `page` is non-zero (checked
///   above) and was assigned during table construction, so `page * 256 +
///   char_idx` is always within the allocated L2 extent. A `debug_assert!`
///   verifies this in debug builds.
///
/// # Panics
///
/// Debug builds panic if `page * 256 + char_idx >= l2.len()`, which would
/// indicate a corrupt or mismatched table pair.
#[inline(always)]
fn page_table_lookup(cp: u32, l1: &[u16], l2: &[u32]) -> Option<u32> {
    let page_idx = (cp >> 8) as usize;
    let char_idx = (cp & 0xFF) as usize;
    if page_idx >= l1.len() {
        return None;
    }
    // SAFETY: `page_idx < l1.len()` is checked by the guard above.
    let page = unsafe { *l1.get_unchecked(page_idx) as usize };
    if page == 0 {
        return None;
    }
    debug_assert!(page * 256 + char_idx < l2.len());
    // SAFETY: `page` is a non-zero index assigned during table construction;
    // `page * 256 + char_idx` is always within the allocated L2 extent.
    let value = unsafe { *l2.get_unchecked(page * 256 + char_idx) };
    (value != 0).then_some(value)
}

#[cfg(not(feature = "runtime_build"))]
/// Decodes a little-endian `u16` table emitted by `build.rs`.
///
/// Each consecutive pair of bytes is interpreted as one `u16` in little-endian
/// order. The input length must be a multiple of 2.
///
/// # Panics
///
/// Debug builds panic if `bytes.len() % 2 != 0`.
#[inline]
fn decode_u16_table(bytes: &[u8]) -> Box<[u16]> {
    debug_assert_eq!(bytes.len() % 2, 0);
    bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

#[cfg(not(feature = "runtime_build"))]
/// Decodes a little-endian `u32` table emitted by `build.rs`.
///
/// Each consecutive group of 4 bytes is interpreted as one `u32` in
/// little-endian order. The input length must be a multiple of 4.
///
/// # Panics
///
/// Debug builds panic if `bytes.len() % 4 != 0`.
#[inline]
fn decode_u32_table(bytes: &[u8]) -> Box<[u32]> {
    debug_assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

/// Trims leading and trailing spaces from one packed Pinyin `(offset, len)` entry.
///
/// The raw Pinyin string buffer may include surrounding spaces around each
/// syllable (e.g., `" zhong "`) for the full-word `PinYin` variant. The
/// `PinYinChar` variant calls this function to strip those spaces so that
/// individual characters produce bare syllables (e.g., `"zhong"`).
///
/// The return value is a new packed `(offset << 8) | length` with the
/// trimmed boundaries. Returns `0` unchanged if the input is `0` (unmapped).
#[inline]
fn trim_pinyin_packed(value: u32, strings: &str) -> u32 {
    if value == 0 {
        return 0;
    }
    let mut start = (value >> 8) as usize;
    let mut end = start + (value & 0xFF) as usize;
    let bytes = strings.as_bytes();
    while start < end && bytes[start] == b' ' {
        start += 1;
    }
    while end > start && bytes[end - 1] == b' ' {
        end -= 1;
    }
    ((start as u32) << 8) | ((end - start) as u32)
}

// ---------------------------------------------------------------------------
// Fanjian (Traditional → Simplified Chinese)
// ---------------------------------------------------------------------------

/// Iterator over Traditional Chinese codepoints that have Simplified replacements.
///
/// Scans `text` byte-by-byte (with [`skip_ascii_simd`] acceleration), decodes
/// each non-ASCII codepoint via [`decode_utf8_raw`], and probes the two-stage
/// page table. Yields `(start, end, simplified_char)` for every codepoint that
/// maps to a *different* Simplified form.
struct FanjianFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for FanjianFindIter<'a> {
    type Item = (usize, usize, char);

    /// Advances to the next Traditional codepoint that has a Simplified replacement.
    ///
    /// # Safety (internal)
    ///
    /// - [`decode_utf8_raw`] is called only after [`skip_ascii_simd`] has
    ///   positioned `byte_offset` at a non-ASCII byte, which is always a valid
    ///   UTF-8 lead byte inside a `&str`.
    /// - [`char::from_u32_unchecked`] is called on the L2 value. The page
    ///   table is generated from valid Unicode codepoints at build time, so
    ///   every non-zero L2 entry is a valid scalar value. A `debug_assert!`
    ///   verifies this in debug builds.
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.text.as_bytes();
        let len = bytes.len();

        loop {
            self.byte_offset = skip_ascii_simd(bytes, self.byte_offset);
            if self.byte_offset >= len {
                return None;
            }

            let start = self.byte_offset;
            // SAFETY: SIMD skip positioned `start` at a non-ASCII byte in a valid UTF-8 `&str`,
            // so it is a valid multi-byte lead byte.
            let (cp, char_len) = unsafe { decode_utf8_raw(bytes, start) };
            self.byte_offset += char_len;

            if let Some(mapped_cp) = page_table_lookup(cp, self.l1, self.l2)
                && mapped_cp != cp
            {
                debug_assert!(char::from_u32(mapped_cp).is_some());
                // SAFETY: Page table values are valid Unicode codepoints assigned at build time.
                let mapped = unsafe { char::from_u32_unchecked(mapped_cp) };
                return Some((start, self.byte_offset, mapped));
            }
        }
    }
}

/// Two-stage page-table matcher for Traditional-to-Simplified Chinese replacement.
///
/// Stores a pair of decoded page tables (`l1` and `l2`) whose layout is
/// described in the [module documentation](self). Each non-zero L2 entry is
/// the Unicode codepoint of the Simplified equivalent.
///
/// Construction is feature-gated:
/// - **Default**: `FanjianMatcher` decodes the binary tables emitted by
///   `build.rs`.
/// - **`runtime_build`**: `FanjianMatcher::from_map` builds the tables from a
///   `HashMap<u32, u32>` parsed from source text at startup.
#[derive(Clone)]
pub(crate) struct FanjianMatcher {
    /// L1 page index: `codepoint >> 8` -> 1-based page number (0 = unmapped).
    l1: Box<[u16]>,
    /// L2 data pages: `page * 256 + (codepoint & 0xFF)` -> simplified codepoint (0 = unmapped).
    l2: Box<[u32]>,
}

impl FanjianMatcher {
    /// Returns an iterator over all codepoints in `text` whose Simplified form
    /// differs from the original.
    #[inline(always)]
    fn iter<'a>(&'a self, text: &'a str) -> FanjianFindIter<'a> {
        FanjianFindIter {
            l1: &self.l1,
            l2: &self.l2,
            text,
            byte_offset: 0,
        }
    }

    /// Replaces every Traditional Chinese codepoint in `text` that has a Simplified mapping.
    ///
    /// Returns `None` when no replacements were needed.
    pub(crate) fn replace(&self, text: &str) -> Option<String> {
        replace_scan(text, self.iter(text))
    }

    /// Builds a matcher from the precompiled build-time page tables.
    ///
    /// `l1` and `l2` are raw little-endian byte slices embedded at compile time
    /// by `build.rs` (see `constants::FANJIAN_L1_BYTES` and
    /// `constants::FANJIAN_L2_BYTES`). They are decoded into boxed `u16` /
    /// `u32` slices via [`decode_u16_table`] and [`decode_u32_table`].
    #[cfg(not(feature = "runtime_build"))]
    pub(crate) fn new(l1: &'static [u8], l2: &'static [u8]) -> Self {
        Self {
            l1: decode_u16_table(l1),
            l2: decode_u32_table(l2),
        }
    }

    /// Builds a matcher from a runtime-parsed codepoint map.
    ///
    /// `map` keys are Traditional codepoints; values are Simplified codepoints.
    /// Delegates to [`build_2_stage_table`] to produce the L1/L2 arrays.
    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_map(map: AHashMap<u32, u32>) -> Self {
        let (l1, l2) = build_2_stage_table(&map);
        Self {
            l1: l1.into_boxed_slice(),
            l2: l2.into_boxed_slice(),
        }
    }
}

// ---------------------------------------------------------------------------
// Pinyin (CJK → Pinyin romanization)
// ---------------------------------------------------------------------------

/// Iterator over codepoints that have Pinyin replacements.
///
/// Similar to [`FanjianFindIter`] but returns borrowed Pinyin syllable slices
/// instead of replacement `char`s. The current generated Pinyin table has no
/// ASCII keys, so the iterator skips all ASCII runs up front and only probes
/// the page table for non-ASCII codepoints. Yields `(start, end, pinyin_slice)`
/// for each matched codepoint.
struct PinyinFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    /// Shared string buffer containing all Pinyin syllables concatenated.
    strings: &'a str,
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for PinyinFindIter<'a> {
    type Item = (usize, usize, &'a str);

    /// Advances to the next codepoint that has a Pinyin replacement.
    ///
    /// # Safety (internal)
    ///
    /// - [`decode_utf8_raw`] is called only when the current byte is non-ASCII
    ///   (`>= 0x80`), guaranteeing a valid UTF-8 lead byte.
    /// - The `offset + str_len <= self.strings.len()` bounds check ensures the
    ///   slice into the Pinyin string buffer is in range.
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.text.as_bytes();
        let len = bytes.len();

        loop {
            self.byte_offset = skip_ascii_simd(bytes, self.byte_offset);
            if self.byte_offset >= len {
                return None;
            }

            let start = self.byte_offset;
            // SAFETY: `skip_ascii_simd` positioned `start` at a non-ASCII byte in a valid UTF-8
            // `&str`, so it is a valid multi-byte lead byte.
            let (cp, char_len) = unsafe { decode_utf8_raw(bytes, start) };
            self.byte_offset += char_len;

            if let Some(value) = page_table_lookup(cp, self.l1, self.l2) {
                let offset = (value >> 8) as usize;
                let str_len = (value & 0xFF) as usize;
                if offset + str_len <= self.strings.len() {
                    return Some((
                        start,
                        self.byte_offset,
                        &self.strings[offset..offset + str_len],
                    ));
                }
            }
        }
    }
}

/// Two-stage page-table matcher for CJK-to-Pinyin replacement.
///
/// Uses the same two-stage page table as [`FanjianMatcher`], but each non-zero
/// L2 entry encodes `(byte_offset << 8) | byte_length` into a shared string
/// buffer (`strings`) that contains all Pinyin syllables concatenated.
///
/// Two construction modes mirror those of `FanjianMatcher`:
/// - **Default**: `PinyinMatcher` decodes build-time binary tables.
/// - **`runtime_build`**: `PinyinMatcher::from_map` builds tables from a
///   `HashMap<u32, &str>`.
///
/// When `trim_space` is `true` (used by the `PinYinChar` variant), each L2
/// entry is adjusted by [`trim_pinyin_packed`] to exclude surrounding spaces.
#[derive(Clone)]
pub(crate) struct PinyinMatcher {
    /// L1 page index: `codepoint >> 8` -> 1-based page number (0 = unmapped).
    l1: Box<[u16]>,
    /// L2 data pages: `page * 256 + (codepoint & 0xFF)` -> packed `(offset << 8) | length`.
    l2: Box<[u32]>,
    /// Concatenated Pinyin syllable strings. Borrowed from a `&'static str`
    /// constant in the default build; owned when using `runtime_build`.
    strings: Cow<'static, str>,
}

impl PinyinMatcher {
    /// Returns an iterator over all codepoints in `text` that have Pinyin output.
    #[inline(always)]
    fn iter<'a>(&'a self, text: &'a str) -> PinyinFindIter<'a> {
        PinyinFindIter {
            l1: &self.l1,
            l2: &self.l2,
            strings: self.strings.as_ref(),
            text,
            byte_offset: 0,
        }
    }

    /// Replaces every matched codepoint in `text` with its Pinyin syllable string.
    ///
    /// Returns `None` when no codepoint in `text` has a Pinyin mapping,
    /// allowing callers to continue borrowing the original input. The `bool`
    /// indicates whether the output is pure ASCII.
    pub(crate) fn replace(&self, text: &str) -> Option<(String, bool)> {
        replace_spans_tracking_ascii(text, self.iter(text))
    }

    /// Builds a matcher from the precompiled build-time tables and string storage.
    ///
    /// `l1` and `l2` are raw little-endian byte slices (see
    /// `constants::PINYIN_L1_BYTES`, `constants::PINYIN_L2_BYTES`).
    /// `strings` is the concatenated Pinyin buffer
    /// (`constants::PINYIN_STR_BYTES`).
    ///
    /// When `trim_space` is `true`, every L2 entry is post-processed through
    /// [`trim_pinyin_packed`] to strip surrounding spaces from the syllable
    /// boundaries. This is used by the `PinYinChar` process type.
    #[cfg(not(feature = "runtime_build"))]
    pub(crate) fn new(
        l1: &'static [u8],
        l2: &'static [u8],
        strings: &'static str,
        trim_space: bool,
    ) -> Self {
        let l1 = decode_u16_table(l1);
        let mut l2 = decode_u32_table(l2);
        if trim_space {
            for value in l2.iter_mut() {
                *value = trim_pinyin_packed(*value, strings);
            }
        }
        Self {
            l1,
            l2,
            strings: Cow::Borrowed(strings),
        }
    }

    /// Builds a matcher from a runtime-parsed codepoint-to-string map.
    ///
    /// Concatenates all replacement strings into a single owned buffer, packs
    /// each entry as `(offset << 8) | length`, and delegates to
    /// [`build_2_stage_table`]. If `trim_space` is `true`, entries are
    /// post-processed through [`trim_pinyin_packed`].
    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_map(map: AHashMap<u32, &str>, trim_space: bool) -> Self {
        let mut strings = String::new();
        let packed: AHashMap<u32, u32> = map
            .into_iter()
            .map(|(key, value)| {
                let offset = strings.len() as u32;
                let length = value.len() as u32;
                strings.push_str(value);
                (key, (offset << 8) | length)
            })
            .collect();
        let (l1, l2) = build_2_stage_table(&packed);
        let strings: Cow<'static, str> = Cow::Owned(strings);
        let mut l2 = l2.into_boxed_slice();
        if trim_space {
            for value in l2.iter_mut() {
                *value = trim_pinyin_packed(*value, strings.as_ref());
            }
        }
        Self {
            l1: l1.into_boxed_slice(),
            l2,
            strings,
        }
    }
}

// ---------------------------------------------------------------------------
// Normalize (Aho-Corasick multi-character replacement)
// ---------------------------------------------------------------------------

/// Multi-character normalization matcher plus its parallel replacement table.
///
/// The matcher holds a compiled [`AhoCorasick`] DFA and a `replace_list` where
/// index `i` is the replacement string for the `i`-th pattern in the automaton.
/// Pattern order is established at construction time and must be consistent
/// between the automaton and the replacement list.
#[derive(Clone)]
pub(crate) struct NormalizeMatcher {
    engine: AhoCorasick,
    /// Replacement strings parallel to the automaton's pattern indices.
    /// `replace_list[match.pattern_index]` is the output for a given match.
    replace_list: Vec<&'static str>,
}

impl NormalizeMatcher {
    /// Creates a find iterator over all leftmost-longest matches in `text`.
    #[inline(always)]
    fn find_iter<'a>(&'a self, text: &'a str) -> aho_corasick::FindIter<'a, 'a> {
        self.engine.find_iter(text)
    }

    /// Replaces every normalization match in `text`.
    ///
    /// Scans `text` with the Aho-Corasick automaton in leftmost-longest mode.
    /// For each match, copies the unchanged text since the last match, then
    /// appends the replacement string from `replace_list[pattern_index]`.
    ///
    /// Returns `None` when no pattern matched, so callers can preserve
    /// borrowed input without allocation. The `bool` indicates whether the
    /// output is pure ASCII.
    pub(crate) fn replace(&self, text: &str) -> Option<(String, bool)> {
        let replace_list = &self.replace_list;
        replace_spans_tracking_ascii(
            text,
            self.find_iter(text)
                .map(|m| (m.start(), m.end(), replace_list[m.pattern().as_usize()])),
        )
    }

    /// Builds a matcher from an ordered pattern list.
    ///
    /// Compiles the patterns into an aho_corasick DFA using leftmost-longest
    /// match semantics.
    ///
    /// # Panics
    ///
    /// Panics (via `.unwrap()`) if the Aho-Corasick builder fails.
    pub(crate) fn new<I, P>(patterns: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<str> + AsRef<[u8]>,
    {
        Self {
            engine: AhoCorasickBuilder::new()
                .kind(Some(AhoCorasickKind::DFA))
                .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                .build(patterns)
                .unwrap(),
            replace_list: Vec::new(),
        }
    }

    /// Attaches the replacement list parallel to the compiled pattern order.
    ///
    /// `replace_list[i]` must be the replacement for pattern `i` in the
    /// automaton. Consumes and returns `self` for builder-style chaining.
    pub(crate) fn with_replacements(mut self, replace_list: Vec<&'static str>) -> Self {
        self.replace_list = replace_list;
        self
    }

    /// Builds a matcher from a runtime-parsed normalization dictionary.
    ///
    /// Sorts the dictionary entries by key for deterministic pattern ordering,
    /// builds the Aho-Corasick automaton from the sorted keys, and attaches
    /// the corresponding replacement values via [`NormalizeMatcher::with_replacements`].
    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_dict(dict: AHashMap<&'static str, &'static str>) -> Self {
        let mut pairs: Vec<(&'static str, &'static str)> = dict.into_iter().collect();
        pairs.sort_unstable_by_key(|&(k, _)| k);
        let replace_list: Vec<&'static str> = pairs.iter().map(|&(_, v)| v).collect();
        Self::new(pairs.into_iter().map(|(k, _)| k)).with_replacements(replace_list)
    }
}

// ---------------------------------------------------------------------------
// Runtime page-table builder (shared by Fanjian + Pinyin)
// ---------------------------------------------------------------------------

/// Converts a sparse codepoint map into the shared two-stage page-table layout.
///
/// Groups all keys by their high bits (`key >> 8`) to determine which 256-entry
/// pages are needed. L1 has `(0x10FFFF >> 8) + 1` entries covering the entire
/// Unicode range; each non-empty page gets a 1-based index into L2.
///
/// L2 is laid out as `(num_pages + 1) * 256` entries: page 0 is a dummy
/// (all zeros) so that an L1 value of `0` naturally maps to the zero page.
///
/// Returns `(l1, l2)` ready to be boxed into slices.
#[cfg(feature = "runtime_build")]
fn build_2_stage_table(map: &AHashMap<u32, u32>) -> (Vec<u16>, Vec<u32>) {
    let mut pages: AHashSet<u32> = map.keys().map(|&key| key >> 8).collect();
    let mut page_list: Vec<u32> = pages.drain().collect();
    page_list.sort_unstable();
    const L1_SIZE: usize = (0x10FFFF >> 8) + 1;
    let mut l1 = vec![0u16; L1_SIZE];
    let mut l2 = vec![0u32; (page_list.len() + 1) * 256];
    for (index, &page) in page_list.iter().enumerate() {
        let l2_page_idx = (index + 1) as u16;
        l1[page as usize] = l2_page_idx;
        for char_idx in 0..256u32 {
            let cp = (page << 8) | char_idx;
            if let Some(&value) = map.get(&cp) {
                l2[(l2_page_idx as usize * 256) + char_idx as usize] = value;
            }
        }
    }
    (l1, l2)
}
