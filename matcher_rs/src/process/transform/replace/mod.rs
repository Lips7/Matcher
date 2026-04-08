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
//! module; each engine has its own sub-module.
//!
//! # Performance
//!
//! - **O(1) per codepoint**: two `get_unchecked` lookups (L1 page, L2 value) on
//!   the hot path — branchless modulo the page-zero check.
//! - **String pool reuse**: output buffers are recycled via the thread-local
//!   [`string_pool`](crate::process::string_pool) to reduce allocator pressure.
//! - **Span-copy output**: unchanged byte ranges are bulk-copied; only mapped
//!   codepoints incur per-replacement overhead.

mod normalize;
mod romanize;
mod variant_norm;

pub(crate) use normalize::NormalizeMatcher;
pub(crate) use romanize::RomanizeMatcher;
pub(crate) use variant_norm::VariantNormMatcher;

use crate::process::{
    string_pool::get_string_from_pool,
    transform::{simd::skip_ascii_simd, utf8::decode_utf8_raw},
};

// ---------------------------------------------------------------------------
// Shared replacement helpers
// ---------------------------------------------------------------------------

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

#[inline(always)]
fn replace_spans<'a, I>(text: &str, mut iter: I) -> Option<String>
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
fn page_table_lookup(cp: u32, l1: &[u16], l2: &[u32]) -> Option<u32> {
    let page_idx = (cp >> 8) as usize;
    let char_idx = (cp & 0xFF) as usize;
    if page_idx >= l1.len() {
        return None;
    }
    // SAFETY: `page_idx < l1.len()` checked above.
    let page = unsafe { *l1.get_unchecked(page_idx) as usize };
    if page == 0 {
        return None;
    }
    debug_assert!(page * 256 + char_idx < l2.len());
    // SAFETY: `page` is a non-zero index assigned during table construction.
    let value = unsafe { *l2.get_unchecked(page * 256 + char_idx) };
    (value != 0).then_some(value)
}

fn decode_u16_table(bytes: &[u8]) -> Box<[u16]> {
    debug_assert_eq!(bytes.len() % 2, 0);
    bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn decode_u32_table(bytes: &[u8]) -> Box<[u32]> {
    debug_assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

fn decode_page_table(l1: &[u8], l2: &[u8]) -> (Box<[u16]>, Box<[u32]>) {
    (decode_u16_table(l1), decode_u32_table(l2))
}

/// Unpacks a `(offset << 8) | length` L2 value into a string-buffer slice.
#[inline(always)]
fn unpack_str_ref(value: u32, strings: &str) -> Option<&str> {
    let offset = (value >> 8) as usize;
    let len = (value & 0xFF) as usize;
    if offset + len <= strings.len() {
        Some(&strings[offset..offset + len])
    } else {
        None
    }
}

fn trim_romanize_packed(value: u32, strings: &str) -> u32 {
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
