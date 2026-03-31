//! Charwise lookup engines for Fanjian and Pinyin transformations.
//!
//! Both engines share a **two-stage page table** for O(1) codepoint lookup:
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
//! Scan loops use [`skip_ascii_simd`] and [`skip_non_digit_ascii_simd`] to
//! fast-forward over ASCII bytes that cannot produce hits, falling through to
//! the page-table probe only for multi-byte (non-ASCII) codepoints.
//!
//! [`FanjianMatcher::replace`] has a **same-length fast path**: when the
//! simplified codepoint has the same UTF-8 byte width as the traditional one,
//! it overwrites the bytes in-place via `as_bytes_mut()` without rebuilding the
//! string. Only when a byte-length mismatch is encountered does it fall back to
//! the full scan-and-rebuild path through [`replace_scan`].

#[cfg(feature = "runtime_build")]
use ahash::{AHashMap, AHashSet};
use std::borrow::Cow;

use crate::process::transform::simd::{skip_ascii_simd, skip_non_digit_ascii_simd};
use crate::process::variant::get_string_from_pool;

/// Replacement payload yielded by the charwise iterators.
///
/// Each variant corresponds to one type of replacement output:
/// [`FanjianFindIter`] always yields [`Replacement::Char`], while
/// [`PinyinFindIter`] always yields [`Replacement::Str`].
enum Replacement<'a> {
    /// Replace the matched span with a single Unicode scalar value (Fanjian).
    Char(char),
    /// Replace the matched span with a borrowed string slice (Pinyin).
    Str(&'a str),
}

/// Shared scan-and-rebuild helper for iterators that yield non-overlapping replacement spans.
///
/// Pulls the first item from `iter`; if `None`, returns `None` (no replacements needed).
/// Otherwise allocates a [`String`] from the thread-local pool, copies unchanged text
/// between replacement spans, and calls `push` for each replacement payload.
///
/// The caller is responsible for ensuring the iterator yields spans in strictly
/// ascending, non-overlapping byte-offset order; otherwise the interleaved
/// `push_str` calls will produce garbled output.
#[inline(always)]
fn replace_scan<'a, I, F>(text: &str, mut iter: I, mut push: F) -> Option<String>
where
    I: Iterator<Item = (usize, usize, Replacement<'a>)>,
    F: FnMut(&mut String, Replacement<'a>),
{
    if let Some((start, end, replacement)) = iter.next() {
        let mut result = get_string_from_pool(text.len());
        result.push_str(&text[..start]);
        push(&mut result, replacement);
        let mut last_end = end;
        for (start, end, replacement) in iter {
            result.push_str(&text[last_end..start]);
            push(&mut result, replacement);
            last_end = end;
        }
        result.push_str(&text[last_end..]);
        Some(result)
    } else {
        None
    }
}

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

/// Decodes one non-ASCII UTF-8 codepoint from `bytes[offset..]`.
///
/// Returns `(codepoint, byte_length)` where `byte_length` is 2, 3, or 4.
/// This function handles only multi-byte sequences (lead byte >= 0xC0); it
/// must not be called on ASCII bytes.
///
/// # Safety
///
/// - `offset` must point at a valid UTF-8 continuation-sequence start (lead
///   byte >= 0xC0). Callers guarantee this by only invoking after confirming
///   `bytes[offset] >= 0x80`, inside a `&str` (which is always valid UTF-8).
/// - `bytes[offset .. offset + char_len]` must be in bounds. This is guaranteed
///   because the input originates from a `&str` whose total length covers the
///   full multi-byte sequence.
/// - Each `get_unchecked` reads a continuation byte at a known offset (1, 2,
///   or 3 past the lead byte). The lead byte's high bits determine how many
///   continuation bytes exist, and valid UTF-8 guarantees they are present.
#[inline(always)]
unsafe fn decode_utf8_raw(bytes: &[u8], offset: usize) -> (u32, usize) {
    // SAFETY: `offset` points at a valid UTF-8 lead byte within `bytes`.
    let b0 = unsafe { *bytes.get_unchecked(offset) };
    if b0 < 0xE0 {
        // SAFETY: 2-byte sequence; valid UTF-8 guarantees the continuation byte is present.
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        (((b0 as u32 & 0x1F) << 6) | (b1 as u32 & 0x3F), 2)
    } else if b0 < 0xF0 {
        // SAFETY: 3-byte sequence; valid UTF-8 guarantees both continuation bytes are present.
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        // SAFETY: Second continuation byte of a 3-byte UTF-8 sequence; guaranteed present.
        let b2 = unsafe { *bytes.get_unchecked(offset + 2) };
        (
            ((b0 as u32 & 0x0F) << 12) | ((b1 as u32 & 0x3F) << 6) | (b2 as u32 & 0x3F),
            3,
        )
    } else {
        // SAFETY: 4-byte sequence; valid UTF-8 guarantees all three continuation bytes are present.
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        // SAFETY: Second continuation byte of a 4-byte UTF-8 sequence; guaranteed present.
        let b2 = unsafe { *bytes.get_unchecked(offset + 2) };
        // SAFETY: Third continuation byte of a 4-byte UTF-8 sequence; guaranteed present.
        let b3 = unsafe { *bytes.get_unchecked(offset + 3) };
        (
            ((b0 as u32 & 0x07) << 18)
                | ((b1 as u32 & 0x3F) << 12)
                | ((b2 as u32 & 0x3F) << 6)
                | (b3 as u32 & 0x3F),
            4,
        )
    }
}

/// Iterator over Traditional Chinese codepoints that have Simplified replacements.
///
/// Scans `text` byte-by-byte (with [`skip_ascii_simd`] acceleration), decodes
/// each non-ASCII codepoint via [`decode_utf8_raw`], and probes the two-stage
/// page table. Yields `(start, end, Replacement::Char(simplified))` for every
/// codepoint that maps to a *different* Simplified form.
struct FanjianFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for FanjianFindIter<'a> {
    type Item = (usize, usize, Replacement<'a>);

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
                return Some((start, self.byte_offset, Replacement::Char(mapped)));
            }
        }
    }
}

/// Byte-by-byte iterator over Fanjian-transformed text.
///
/// Yields the UTF-8 bytes of `text` with all Traditional Chinese codepoints
/// replaced by their Simplified equivalents. Wraps [`FanjianFindIter`]
/// internally: original bytes are yielded between replacement spans, and
/// replacement character bytes are yielded from a small stack buffer.
pub(crate) struct FanjianByteIter<'a> {
    find_iter: FanjianFindIter<'a>,
    source: &'a [u8],
    pos: usize,
    /// Pre-fetched next match start (usize::MAX when exhausted).
    next_start: usize,
    next_end: usize,
    /// Replacement char for the pre-fetched match.
    next_char: char,
    /// Encoded replacement bytes being yielded.
    buf: [u8; 4],
    buf_pos: u8,
    buf_len: u8,
}

impl<'a> Iterator for FanjianByteIter<'a> {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        // Drain replacement buffer
        if self.buf_pos < self.buf_len {
            let b = self.buf[self.buf_pos as usize];
            self.buf_pos += 1;
            return Some(b);
        }

        // At match start? Encode replacement and advance.
        if self.pos == self.next_start {
            self.pos = self.next_end;
            let len = self.next_char.len_utf8();
            self.next_char.encode_utf8(&mut self.buf);
            self.buf_len = len as u8;
            self.buf_pos = 1;
            // Fetch next match
            self.advance_find_iter();
            return Some(self.buf[0]);
        }

        // Yield original byte
        if self.pos < self.source.len() {
            let b = self.source[self.pos];
            self.pos += 1;
            Some(b)
        } else {
            None
        }
    }
}

impl<'a> FanjianByteIter<'a> {
    #[inline(always)]
    fn advance_find_iter(&mut self) {
        match self.find_iter.next() {
            Some((s, e, Replacement::Char(c))) => {
                self.next_start = s;
                self.next_end = e;
                self.next_char = c;
            }
            _ => {
                self.next_start = usize::MAX;
            }
        }
    }
}

/// Iterator over codepoints that have Pinyin replacements.
///
/// Similar to [`FanjianFindIter`] but uses [`skip_non_digit_ascii_simd`]
/// instead of [`skip_ascii_simd`], because ASCII digits may appear in the
/// Pinyin tables and must not be skipped. Yields
/// `(start, end, Replacement::Str(pinyin_slice))` for each matched codepoint.
struct PinyinFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    /// Shared string buffer containing all Pinyin syllables concatenated.
    strings: &'a str,
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for PinyinFindIter<'a> {
    type Item = (usize, usize, Replacement<'a>);

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
            self.byte_offset = skip_non_digit_ascii_simd(bytes, self.byte_offset);
            if self.byte_offset >= len {
                return None;
            }

            let start = self.byte_offset;
            let byte = bytes[start];
            let (cp, char_len) = if byte < 0x80 {
                (byte as u32, 1)
            } else {
                // SAFETY: `byte >= 0x80` means non-ASCII in a valid UTF-8 `&str`, so `start` is a
                // valid multi-byte lead byte.
                unsafe { decode_utf8_raw(bytes, start) }
            };
            self.byte_offset += char_len;

            if let Some(value) = page_table_lookup(cp, self.l1, self.l2) {
                let offset = (value >> 8) as usize;
                let str_len = (value & 0xFF) as usize;
                if offset + str_len <= self.strings.len() {
                    return Some((
                        start,
                        self.byte_offset,
                        Replacement::Str(&self.strings[offset..offset + str_len]),
                    ));
                }
            }
        }
    }
}

/// Byte-by-byte iterator over Pinyin-transformed text.
///
/// Yields the UTF-8 bytes of `text` with all matched CJK codepoints replaced
/// by their Pinyin syllable bytes. Wraps [`PinyinFindIter`] internally.
pub(crate) struct PinyinByteIter<'a> {
    find_iter: PinyinFindIter<'a>,
    source: &'a [u8],
    pos: usize,
    /// Pre-fetched next match start (usize::MAX when exhausted).
    next_start: usize,
    next_end: usize,
    /// Pre-fetched replacement bytes for the next match.
    next_repl: &'a [u8],
    /// Current replacement bytes being yielded.
    repl: &'a [u8],
    repl_pos: usize,
}

impl<'a> Iterator for PinyinByteIter<'a> {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        // Drain current replacement
        if self.repl_pos < self.repl.len() {
            let b = self.repl[self.repl_pos];
            self.repl_pos += 1;
            return Some(b);
        }

        // At match start?
        if self.pos == self.next_start {
            self.pos = self.next_end;
            self.repl = self.next_repl;
            self.repl_pos = 1;
            let first = self.repl[0];
            self.advance_find_iter();
            return Some(first);
        }

        // Yield original byte
        if self.pos < self.source.len() {
            let b = self.source[self.pos];
            self.pos += 1;
            Some(b)
        } else {
            None
        }
    }
}

impl<'a> PinyinByteIter<'a> {
    #[inline(always)]
    fn advance_find_iter(&mut self) {
        match self.find_iter.next() {
            Some((s, e, Replacement::Str(r))) => {
                self.next_start = s;
                self.next_end = e;
                self.next_repl = r.as_bytes();
            }
            _ => {
                self.next_start = usize::MAX;
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
/// - **Default**: [`FanjianMatcher::new`] decodes the binary tables emitted by
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

    /// Returns a byte-by-byte iterator over Fanjian-transformed text.
    ///
    /// Equivalent output to `replace()` followed by iterating the result's
    /// bytes, but without allocating an intermediate `String`.
    #[inline(always)]
    pub(crate) fn byte_iter<'a>(&'a self, text: &'a str) -> FanjianByteIter<'a> {
        let mut iter = FanjianByteIter {
            find_iter: self.iter(text),
            source: text.as_bytes(),
            pos: 0,
            next_start: usize::MAX,
            next_end: 0,
            next_char: '\0',
            buf: [0; 4],
            buf_pos: 0,
            buf_len: 0,
        };
        iter.advance_find_iter();
        iter
    }

    /// Replaces every Traditional Chinese codepoint in `text` that has a Simplified mapping.
    ///
    /// Returns `None` when no replacements were needed.
    pub(crate) fn replace(&self, text: &str) -> Option<String> {
        replace_scan(text, self.iter(text), |result, replacement| {
            let Replacement::Char(mapped) = replacement else {
                unreachable!("fanjian iter yields char replacements");
            };
            result.push(mapped);
        })
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

/// Two-stage page-table matcher for CJK-to-Pinyin replacement.
///
/// Uses the same two-stage page table as [`FanjianMatcher`], but each non-zero
/// L2 entry encodes `(byte_offset << 8) | byte_length` into a shared string
/// buffer (`strings`) that contains all Pinyin syllables concatenated.
///
/// Two construction modes mirror those of `FanjianMatcher`:
/// - **Default**: [`PinyinMatcher::new`] decodes build-time binary tables.
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

    /// Returns a byte-by-byte iterator over Pinyin-transformed text.
    ///
    /// Equivalent output to `replace()` followed by iterating the result's
    /// bytes, but without allocating an intermediate `String`.
    #[inline(always)]
    pub(crate) fn byte_iter<'a>(&'a self, text: &'a str) -> PinyinByteIter<'a> {
        let mut iter = PinyinByteIter {
            find_iter: self.iter(text),
            source: text.as_bytes(),
            pos: 0,
            next_start: usize::MAX,
            next_end: 0,
            next_repl: &[],
            repl: &[],
            repl_pos: 0,
        };
        iter.advance_find_iter();
        iter
    }

    /// Replaces every matched codepoint in `text` with its Pinyin syllable string.
    ///
    /// Returns `None` when no codepoint in `text` has a Pinyin mapping,
    /// allowing callers to continue borrowing the original input. The `bool`
    /// indicates whether the output is pure ASCII, tracked incrementally to
    /// avoid a redundant scan. Pinyin replacements are always ASCII, so only
    /// the unchanged gaps between replacements need checking.
    pub(crate) fn replace(&self, text: &str) -> Option<(String, bool)> {
        let mut iter = self.iter(text);
        if let Some((start, end, replacement)) = iter.next() {
            let mut result = get_string_from_pool(text.len());
            let prefix = &text[..start];
            let mut is_ascii = prefix.is_ascii();
            result.push_str(prefix);
            let Replacement::Str(mapped) = replacement else {
                unreachable!("pinyin iter yields string replacements");
            };
            result.push_str(mapped);
            let mut last_end = end;
            for (start, end, replacement) in iter {
                let gap = &text[last_end..start];
                is_ascii = is_ascii && gap.is_ascii();
                result.push_str(gap);
                let Replacement::Str(mapped) = replacement else {
                    unreachable!("pinyin iter yields string replacements");
                };
                result.push_str(mapped);
                last_end = end;
            }
            let suffix = &text[last_end..];
            is_ascii = is_ascii && suffix.is_ascii();
            result.push_str(suffix);
            Some((result, is_ascii))
        } else {
            None
        }
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

#[cfg(test)]
mod tests {
    #[cfg(not(feature = "runtime_build"))]
    use super::*;

    #[cfg(not(feature = "runtime_build"))]
    use super::super::constants;

    #[cfg(not(feature = "runtime_build"))]
    fn fanjian() -> FanjianMatcher {
        FanjianMatcher::new(constants::FANJIAN_L1_BYTES, constants::FANJIAN_L2_BYTES)
    }

    #[cfg(not(feature = "runtime_build"))]
    fn pinyin() -> PinyinMatcher {
        PinyinMatcher::new(
            constants::PINYIN_L1_BYTES,
            constants::PINYIN_L2_BYTES,
            constants::PINYIN_STR_BYTES,
            false,
        )
    }

    #[cfg(not(feature = "runtime_build"))]
    fn pinyin_char() -> PinyinMatcher {
        PinyinMatcher::new(
            constants::PINYIN_L1_BYTES,
            constants::PINYIN_L2_BYTES,
            constants::PINYIN_STR_BYTES,
            true,
        )
    }

    #[cfg(not(feature = "runtime_build"))]
    fn assert_byte_iter_eq_replace_fanjian(matcher: &FanjianMatcher, text: &str) {
        let materialized: Vec<u8> = match matcher.replace(text) {
            Some(s) => s.into_bytes(),
            None => text.as_bytes().to_vec(),
        };
        let streamed: Vec<u8> = matcher.byte_iter(text).collect();
        assert_eq!(materialized, streamed, "fanjian mismatch for: {:?}", text);
    }

    #[cfg(not(feature = "runtime_build"))]
    fn assert_byte_iter_eq_replace_pinyin(matcher: &PinyinMatcher, text: &str) {
        let materialized: Vec<u8> = match matcher.replace(text) {
            Some((s, _)) => s.into_bytes(),
            None => text.as_bytes().to_vec(),
        };
        let streamed: Vec<u8> = matcher.byte_iter(text).collect();
        assert_eq!(materialized, streamed, "pinyin mismatch for: {:?}", text);
    }

    #[test]
    #[cfg(not(feature = "runtime_build"))]
    fn fanjian_byte_iter_matches_replace() {
        let m = fanjian();
        for text in ["", "hello", "國際經濟", "abc東def國", "a", "東"] {
            assert_byte_iter_eq_replace_fanjian(&m, text);
        }
    }

    #[test]
    #[cfg(not(feature = "runtime_build"))]
    fn pinyin_byte_iter_matches_replace() {
        let m = pinyin();
        for text in ["", "hello", "中文", "abc中def文", "a", "中"] {
            assert_byte_iter_eq_replace_pinyin(&m, text);
        }
    }

    #[test]
    #[cfg(not(feature = "runtime_build"))]
    fn pinyin_char_byte_iter_matches_replace() {
        let m = pinyin_char();
        for text in ["", "hello", "中文", "abc中def文"] {
            assert_byte_iter_eq_replace_pinyin(&m, text);
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(500))]

        #[test]
        #[cfg(not(feature = "runtime_build"))]
        fn prop_fanjian_byte_iter(text in "\\PC{0,200}") {
            let m = fanjian();
            assert_byte_iter_eq_replace_fanjian(&m, &text);
        }

        #[test]
        #[cfg(not(feature = "runtime_build"))]
        fn prop_pinyin_byte_iter(text in "\\PC{0,200}") {
            let m = pinyin();
            assert_byte_iter_eq_replace_pinyin(&m, &text);
        }
    }
}
