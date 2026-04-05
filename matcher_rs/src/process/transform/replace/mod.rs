//! Text-replacement engines for Fanjian, Pinyin, and Normalize transformations.
//!
//! All three engines share a **two-stage page table** for O(1) codepoint lookup:
//!
//! - **L1** (`&[u16]`): indexed by `codepoint >> 8` (the "page index"). A non-zero
//!   value is the 1-based page number in L2; zero means the entire 256-codepoint
//!   page has no mappings.
//! - **L2** (`&[u32]`): indexed by `page * 256 + (codepoint & 0xFF)`. The
//!   interpretation of the stored `u32` depends on the engine:
//!   - **Fanjian**: the simplified codepoint value. `0` = unmapped.
//!   - **Pinyin / Normalize**: packed `(byte_offset << 8) | byte_length` into a
//!     shared string buffer, unpacked via [`unpack_str_ref`]. `0` = unmapped.
//!
//! Shared helpers ([`page_table_lookup`], [`decode_page_table`], [`unpack_str_ref`],
//! [`replace_scan`], [`replace_spans_tracking_ascii`]) live in this module; each
//! engine has its own sub-module.

mod fanjian;
mod normalize;
mod pinyin;

pub(crate) use fanjian::FanjianMatcher;
pub(crate) use normalize::NormalizeMatcher;
pub(crate) use pinyin::PinyinMatcher;

#[cfg(feature = "runtime_build")]
use ahash::{AHashMap, AHashSet};

use crate::process::string_pool::get_string_from_pool;
use crate::process::transform::simd::skip_ascii_simd;
use crate::process::transform::utf8::decode_utf8_raw;

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

#[cfg(not(feature = "runtime_build"))]
fn decode_u16_table(bytes: &[u8]) -> Box<[u16]> {
    debug_assert_eq!(bytes.len() % 2, 0);
    bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

#[cfg(not(feature = "runtime_build"))]
fn decode_u32_table(bytes: &[u8]) -> Box<[u32]> {
    debug_assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

#[cfg(not(feature = "runtime_build"))]
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
