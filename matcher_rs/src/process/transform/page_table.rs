//! Text-replacement engines for VariantNorm, Romanize, and Normalize
//! transformations.
//!
//! All three engines share a **two-stage page table** for O(1) codepoint
//! lookup:
//!
//! - **L1** (`&[u16]`): indexed by `codepoint >> 8` (the "page index"). A
//!   non-zero value is the 1-based page number in L2; zero means the entire
//!   256-codepoint page has no mappings.
//! - **L2** (`&[u32]`): indexed by `page * 256 + (codepoint & 0xFF)`. The
//!   interpretation of the stored `u32` depends on the engine:
//!   - **VariantNorm**: the normalized codepoint value. `0` = unmapped.
//!   - **Romanize / Normalize**: packed `(byte_offset << 8) | byte_length` into
//!     a shared string buffer, unpacked via [`unpack_str_ref`]. `0` = unmapped.
//!
//! Shared helpers ([`page_table_lookup`], [`decode_page_table`],
//! [`unpack_str_ref`], [`replace_scan`], [`replace_spans`]) live in this
//! module; each engine has its own sibling module.
//!
//! # Performance
//!
//! - **O(1) per codepoint**: two `get_unchecked` lookups (L1 page, L2 value) on
//!   the hot path — branchless modulo the page-zero check.
//! - **Minimal allocation**: output buffers are pre-allocated to the input
//!   length, avoiding mid-write reallocations.
//! - **Span-copy output**: unchanged byte ranges are bulk-copied; only mapped
//!   codepoints incur per-replacement overhead.

use super::{simd::skip_ascii_simd, utf8::decode_utf8_raw};

// ---------------------------------------------------------------------------
// Unified find iterator for str-replacement page tables
// ---------------------------------------------------------------------------

/// Find iterator for page-table-backed string replacement.
///
/// Yields `(byte_start, byte_end, &str)` tuples for each codepoint that has a
/// mapping in the page table. The const generic `CHECK_ASCII` controls ASCII
/// handling:
///
/// - `false` — uses [`skip_ascii_simd`] to bulk-skip ASCII runs. Suitable when
///   all page-table keys are non-ASCII (e.g., CJK romanization).
/// - `true` — checks each ASCII byte individually, since some (A–Z) may have
///   mappings (e.g., Unicode normalization casefolding).
pub(super) struct StrReplaceFindIter<'a, const CHECK_ASCII: bool> {
    pub(super) l1: &'a [u16],
    pub(super) l2: &'a [u32],
    pub(super) strings: &'a str,
    pub(super) text: &'a str,
    pub(super) byte_offset: usize,
}

impl<'a, const CHECK_ASCII: bool> Iterator for StrReplaceFindIter<'a, CHECK_ASCII> {
    type Item = (usize, usize, &'a str);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.text.as_bytes();
        let len = bytes.len();

        loop {
            if !CHECK_ASCII {
                self.byte_offset = skip_ascii_simd(bytes, self.byte_offset);
            }
            if self.byte_offset >= len {
                return None;
            }

            let start = self.byte_offset;
            let b = bytes[start];

            if b < 0x80 {
                self.byte_offset += 1;
                if CHECK_ASCII
                    && b.is_ascii_uppercase()
                    && let Some(value) = page_table_lookup(b as u32, self.l1, self.l2)
                    && let Some(s) = unpack_str_ref(value, self.strings)
                {
                    return Some((start, start + 1, s));
                }
                continue;
            }

            // SAFETY: positioned at a non-ASCII lead byte in a valid UTF-8 `&str`.
            let (cp, char_len) = unsafe { decode_utf8_raw(bytes, start) };
            self.byte_offset += char_len;

            if let Some(value) = page_table_lookup(cp, self.l1, self.l2)
                && let Some(s) = unpack_str_ref(value, self.strings)
            {
                return Some((start, self.byte_offset, s));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Shared replacement helpers
// ---------------------------------------------------------------------------

/// Builds a `String` by applying `(start, end, char)` replacements from an
/// iterator over a source `text`.
///
/// Returns `None` if the iterator is empty (nothing to replace). Used by
/// `VariantNormMatcher::replace` for single-codepoint replacements.
#[inline(always)]
pub(crate) fn replace_scan<I>(text: &str, mut iter: I) -> Option<String>
where
    I: Iterator<Item = (usize, usize, char)>,
{
    if let Some((start, end, ch)) = iter.next() {
        let mut result = String::with_capacity(text.len());
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

/// Builds a `String` by applying `(start, end, &str)` replacements from an
/// iterator over a source `text`.
///
/// Returns `None` if the iterator is empty. Used by
/// `RomanizeMatcher::replace` and `NormalizeMatcher::replace` for
/// multi-byte string replacements.
#[inline(always)]
pub(crate) fn replace_spans<'a, I>(text: &str, mut iter: I) -> Option<String>
where
    I: Iterator<Item = (usize, usize, &'a str)>,
{
    if let Some((start, end, replacement)) = iter.next() {
        let mut result = String::with_capacity(text.len());
        result.push_str(&text[..start]);
        result.push_str(replacement);
        let mut last_end = end;
        for (start, end, replacement) in iter {
            result.push_str(&text[last_end..start]);
            result.push_str(replacement);
            last_end = end;
        }
        result.push_str(&text[last_end..]);
        Some(result)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Shared page-table infrastructure
// ---------------------------------------------------------------------------

/// Looks up one codepoint in a two-stage page table.
///
/// # Safety
///
/// Uses `get_unchecked` for L1 (guarded by bounds check) and L2 (page index
/// assigned during construction guarantees in-range access).
#[inline(always)]
pub(crate) fn page_table_lookup(cp: u32, l1: &[u16], l2: &[u32]) -> Option<u32> {
    let page_idx = (cp >> 8) as usize;
    let char_idx = (cp & 0xFF) as usize;
    if page_idx >= l1.len() {
        return None;
    }
    // SAFETY: The `page_idx >= l1.len()` guard above ensures in-bounds.
    unsafe { core::hint::assert_unchecked(page_idx < l1.len()) };
    let page = l1[page_idx] as usize;
    if page == 0 {
        return None;
    }
    // SAFETY: Page indices assigned during construction guarantee in-range L2
    // access.
    unsafe { core::hint::assert_unchecked(page * 256 + char_idx < l2.len()) };
    let value = l2[page * 256 + char_idx];
    (value != 0).then_some(value)
}

/// Decodes a little-endian `&[u8]` into `Box<[u16]>` (L1 page table).
fn decode_u16_table(bytes: &[u8]) -> Box<[u16]> {
    debug_assert_eq!(bytes.len() % 2, 0);
    bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

/// Decodes a little-endian `&[u8]` into `Box<[u32]>` (L2 page table).
fn decode_u32_table(bytes: &[u8]) -> Box<[u32]> {
    debug_assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

/// Convenience wrapper: decodes both L1 and L2 page tables from raw bytes.
pub(crate) fn decode_page_table(l1: &[u8], l2: &[u8]) -> (Box<[u16]>, Box<[u32]>) {
    (decode_u16_table(l1), decode_u32_table(l2))
}

/// Unpacks a `(offset << 8) | length` L2 value into a string-buffer slice.
#[inline(always)]
pub(crate) fn unpack_str_ref(value: u32, strings: &str) -> Option<&str> {
    let offset = (value >> 8) as usize;
    let len = (value & 0xFF) as usize;
    if offset + len <= strings.len() {
        Some(&strings[offset..offset + len])
    } else {
        None
    }
}

/// Trims leading and trailing ASCII spaces from a packed L2 entry.
///
/// Used by `RomanizeMatcher::new` when `trim_space` is `true`
/// (`RomanizeChar` variant) to produce space-free per-character romanization.
pub(crate) fn trim_romanize_packed(value: u32, strings: &str) -> u32 {
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
