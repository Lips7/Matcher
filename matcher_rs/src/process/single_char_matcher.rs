use std::borrow::Cow;
#[cfg(feature = "runtime_build")]
use std::collections::HashMap;
#[cfg(feature = "runtime_build")]
use std::collections::HashSet;
use std::simd::{Simd, cmp::SimdPartialOrd};

use crate::process::simd_utils::{
    simd_ascii_delete_mask, skip_ascii_simd, skip_non_digit_ascii_simd,
};

#[cfg(feature = "runtime_build")]
const UNICODE_BITSET_SIZE: usize = 0x110000 / 8;

/// Single-character lookup engine backed by compact, pre-compiled data structures.
///
/// Each variant provides O(1) per-codepoint dispatch with no state-machine overhead.
/// Instances are constructed by `get_process_matcher` and cached for the lifetime of the program.
///
/// ## Page-table layout (Fanjian and Pinyin)
///
/// For a Unicode codepoint `cp`:
/// ```text
/// page_idx = cp >> 8          (selects one of 4352 L1 entries)
/// char_idx = cp & 0xFF        (selects one of 256 entries within the page)
/// page     = u16::from_le(l1[page_idx * 2 ..])
/// value    = u32::from_le(l2[(page * 256 + char_idx) * 4 ..])
/// ```
/// `page == 0` means the entire 256-codepoint block has no mapping (fast skip).
///
/// For Pinyin the `value` packs `(offset << 8) | length` into the string buffer;
/// for Fanjian the value is the mapped codepoint directly.
#[derive(Clone)]
pub(crate) enum SingleCharMatcher {
    /// Traditional Chinese → Simplified Chinese via a 2-stage page table.
    ///
    /// * `l1` — L1 index: `u16[4352]`, one entry per 256-codepoint block. Non-zero entries
    ///   point to a page in `l2`.
    /// * `l2` — L2 data: `u32[num_pages * 256]`. Each entry is the mapped codepoint, or
    ///   `0` if the source codepoint has no mapping (i.e. already Simplified).
    Fanjian { l1: Box<[u16]>, l2: Box<[u32]> },
    /// Chinese character → Pinyin syllable(s) via a 2-stage page table.
    ///
    /// * `l1` / `l2` — same page-table structure as `Fanjian`, but each L2 value packs
    ///   `(offset << 8) | length` pointing into `strings`.
    /// * `strings` — concatenated Pinyin syllables (e.g. `"zhong guo ..."`) with space
    ///   separators between syllables.
    Pinyin {
        l1: Box<[u16]>,
        l2: Box<[u32]>,
        strings: Cow<'static, str>,
    },
    /// Character deletion via a 139 KB flat BitSet covering all Unicode planes.
    ///
    /// * `bitset` — `u8[139264]`; bit `cp % 8` of byte `cp / 8` is set if codepoint
    ///   `cp` should be removed. Covers codepoints 0x0 – 0x10FFFF.
    /// * `ascii_lut` — cache-hot copy of the first 16 bytes of `bitset` (codepoints 0–127),
    ///   kept alongside the struct fields to avoid touching the 139 KB bitset for ASCII input.
    Delete {
        bitset: Cow<'static, [u8]>,
        ascii_lut: [u8; 16],
        ascii_lut_simd: Simd<u8, 16>,
    },
}

/// The transformation to apply to a matched codepoint.
pub(crate) enum SingleCharMatch<'a> {
    Char(char),
    Str(&'a str),
    Delete,
}

/// Looks up a Unicode codepoint in a 2-stage page table, returning the packed value or `None`.
#[inline(always)]
fn page_table_lookup(cp: u32, l1: &[u16], l2: &[u32]) -> Option<u32> {
    let page_idx = (cp >> 8) as usize;
    let char_idx = (cp & 0xFF) as usize;
    if page_idx >= l1.len() {
        return None;
    }
    let page = unsafe { *l1.get_unchecked(page_idx) as usize };
    if page == 0 {
        return None;
    }
    // SAFETY: page is a non-zero L1 entry produced by build_2_stage_table, so
    // page * 256 + char_idx (where char_idx < 256) is within the L2 allocation.
    debug_assert!(page * 256 + char_idx < l2.len());
    let val = unsafe { *l2.get_unchecked(page * 256 + char_idx) };
    if val != 0 { Some(val) } else { None }
}

#[cfg(not(feature = "runtime_build"))]
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
#[inline]
fn decode_u32_table(bytes: &[u8]) -> Box<[u32]> {
    debug_assert_eq!(bytes.len() % 4, 0);
    bytes
        .chunks_exact(4)
        .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect::<Vec<_>>()
        .into_boxed_slice()
}

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

/// Decodes one UTF-8 character from `bytes` starting at `offset`.
///
/// Returns `(codepoint, byte_length)`. Only handles non-ASCII leading bytes (>= 0x80).
///
/// # Safety
/// `bytes` must be valid UTF-8, `offset < bytes.len()`, and `bytes[offset] >= 0x80`.
/// All three hold when `bytes` is `str::as_bytes()` and `offset` is at a char boundary.
#[inline(always)]
unsafe fn decode_utf8_raw(bytes: &[u8], offset: usize) -> (u32, usize) {
    // SAFETY: caller guarantees offset < bytes.len() and bytes is valid UTF-8 at offset.
    let b0 = unsafe { *bytes.get_unchecked(offset) };
    if b0 < 0xE0 {
        // 2-byte: 110xxxxx 10xxxxxx
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        (((b0 as u32 & 0x1F) << 6) | (b1 as u32 & 0x3F), 2)
    } else if b0 < 0xF0 {
        // 3-byte: 1110xxxx 10xxxxxx 10xxxxxx
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        let b2 = unsafe { *bytes.get_unchecked(offset + 2) };
        (
            ((b0 as u32 & 0x0F) << 12) | ((b1 as u32 & 0x3F) << 6) | (b2 as u32 & 0x3F),
            3,
        )
    } else {
        // 4-byte: 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        let b2 = unsafe { *bytes.get_unchecked(offset + 2) };
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

/// Monomorphized iterator for Fanjian (Traditional→Simplified) lookups.
pub(crate) struct FanjianFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for FanjianFindIter<'a> {
    type Item = (usize, usize, SingleCharMatch<'a>);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.text.as_bytes();
        let len = bytes.len();

        loop {
            // SIMD skip: advance past all ASCII bytes (no Fanjian mapping for ASCII).
            self.byte_offset = skip_ascii_simd(bytes, self.byte_offset);

            if self.byte_offset >= len {
                return None;
            }

            let start = self.byte_offset;
            // SAFETY: byte_offset < len, bytes is valid UTF-8, bytes[byte_offset] >= 0x80.
            let (cp, char_len) = unsafe { decode_utf8_raw(bytes, start) };
            self.byte_offset += char_len;

            if let Some(mapped_cp) = page_table_lookup(cp, self.l1, self.l2)
                && mapped_cp != cp
            {
                // SAFETY: build.rs guarantees mapped_cp is a valid Unicode scalar value.
                debug_assert!(char::from_u32(mapped_cp).is_some());
                let mapped = unsafe { char::from_u32_unchecked(mapped_cp) };
                return Some((start, self.byte_offset, SingleCharMatch::Char(mapped)));
            }
        }
    }
}

/// Monomorphized iterator for Delete (bitset-based character removal).
pub(crate) struct DeleteFindIter<'a> {
    bitset: &'a [u8],
    /// Cache-hot copy of `bitset[0..16]` covering ASCII codepoints 0–127.
    ascii_lut: [u8; 16],
    ascii_lut_simd: Simd<u8, 16>,
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for DeleteFindIter<'a> {
    type Item = (usize, usize, SingleCharMatch<'a>);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.text.as_bytes();
        let len = bytes.len();

        loop {
            if self.byte_offset >= len {
                return None;
            }
            let b = bytes[self.byte_offset];
            let start = self.byte_offset;
            if b < 0x80 {
                // ASCII: check ascii_lut (16 bytes, cache-hot) without touching 139 KB bitset.
                let cp = b as usize;
                self.byte_offset += 1;
                if (self.ascii_lut[cp >> 3] & (1 << (cp & 7))) != 0 {
                    return Some((start, self.byte_offset, SingleCharMatch::Delete));
                }
                // SIMD fast-skip: process 16 non-deletable ASCII bytes at a time.
                while self.byte_offset + 16 <= len {
                    let chunk = Simd::<u8, 16>::from_slice(&bytes[self.byte_offset..]);
                    let non_ascii_mask = chunk.simd_ge(Simd::<u8, 16>::splat(0x80u8)).to_bitmask();
                    let del_mask = simd_ascii_delete_mask(chunk, self.ascii_lut_simd);
                    let stop_mask = non_ascii_mask | del_mask;
                    if stop_mask != 0 {
                        self.byte_offset += stop_mask.trailing_zeros() as usize;
                        break;
                    }
                    self.byte_offset += 16;
                }
                // Scalar tail for < 16 remaining bytes.
                while self.byte_offset < len {
                    let b2 = bytes[self.byte_offset];
                    if b2 >= 0x80 {
                        break;
                    }
                    let cp2 = b2 as usize;
                    if (self.ascii_lut[cp2 >> 3] & (1 << (cp2 & 7))) != 0 {
                        break;
                    }
                    self.byte_offset += 1;
                }
            } else {
                // Non-ASCII: decode and check the full 139 KB bitset.
                // SAFETY: byte_offset < len, bytes is valid UTF-8, bytes[byte_offset] >= 0x80.
                let (cp, char_len) = unsafe { decode_utf8_raw(bytes, start) };
                self.byte_offset += char_len;
                let cp_usize = cp as usize;
                if cp_usize / 8 < self.bitset.len()
                    && (self.bitset[cp_usize / 8] & (1 << (cp_usize % 8))) != 0
                {
                    return Some((start, self.byte_offset, SingleCharMatch::Delete));
                }
            }
        }
    }
}

/// Monomorphized iterator for Pinyin (codepoint→syllable) lookups.
pub(crate) struct PinYinFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    strings: &'a str,
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for PinYinFindIter<'a> {
    type Item = (usize, usize, SingleCharMatch<'a>);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let bytes = self.text.as_bytes();
        let len = bytes.len();

        loop {
            // SIMD skip: advance past non-digit ASCII bytes. Digits (0x30–0x39) may have Pinyin mappings.
            self.byte_offset = skip_non_digit_ascii_simd(bytes, self.byte_offset);
            if self.byte_offset >= len {
                return None;
            }

            let start = self.byte_offset;
            let b = bytes[start];
            let (cp, char_len) = if b < 0x80 {
                // ASCII digit (0x30–0x39).
                (b as u32, 1)
            } else {
                // SAFETY: byte_offset < len, bytes is valid UTF-8, bytes[byte_offset] >= 0x80.
                unsafe { decode_utf8_raw(bytes, start) }
            };
            self.byte_offset += char_len;

            if let Some(val) = page_table_lookup(cp, self.l1, self.l2) {
                let offset = (val >> 8) as usize;
                let str_len = (val & 0xFF) as usize;
                if offset + str_len <= self.strings.len() {
                    return Some((
                        start,
                        self.byte_offset,
                        SingleCharMatch::Str(&self.strings[offset..offset + str_len]),
                    ));
                }
            }
        }
    }
}

impl SingleCharMatcher {
    #[inline(always)]
    pub(crate) fn fanjian_iter<'a>(&'a self, text: &'a str) -> FanjianFindIter<'a> {
        let SingleCharMatcher::Fanjian { l1, l2 } = self else {
            unreachable!("fanjian_iter called on non-Fanjian matcher");
        };
        FanjianFindIter {
            l1,
            l2,
            text,
            byte_offset: 0,
        }
    }

    #[inline(always)]
    pub(crate) fn delete_iter<'a>(&'a self, text: &'a str) -> DeleteFindIter<'a> {
        let SingleCharMatcher::Delete {
            bitset,
            ascii_lut,
            ascii_lut_simd,
        } = self
        else {
            unreachable!("delete_iter called on non-Delete matcher");
        };
        DeleteFindIter {
            bitset,
            ascii_lut: *ascii_lut,
            ascii_lut_simd: *ascii_lut_simd,
            text,
            byte_offset: 0,
        }
    }

    #[inline(always)]
    pub(crate) fn pinyin_iter<'a>(&'a self, text: &'a str) -> PinYinFindIter<'a> {
        let SingleCharMatcher::Pinyin { l1, l2, strings } = self else {
            unreachable!("pinyin_iter called on non-Pinyin matcher");
        };
        PinYinFindIter {
            l1,
            l2,
            strings,
            text,
            byte_offset: 0,
        }
    }

    #[cfg(not(feature = "runtime_build"))]
    pub(crate) fn fanjian(l1: Cow<'static, [u8]>, l2: Cow<'static, [u8]>) -> Self {
        SingleCharMatcher::Fanjian {
            l1: decode_u16_table(l1.as_ref()),
            l2: decode_u32_table(l2.as_ref()),
        }
    }

    pub(crate) fn delete(bitset: Cow<'static, [u8]>) -> Self {
        let mut ascii_lut = [0u8; 16];
        let copy_len = bitset.len().min(16);
        ascii_lut[..copy_len].copy_from_slice(&bitset[..copy_len]);
        let ascii_lut_simd = Simd::<u8, 16>::from_array(ascii_lut);
        SingleCharMatcher::Delete {
            bitset,
            ascii_lut,
            ascii_lut_simd,
        }
    }

    #[cfg(not(feature = "runtime_build"))]
    pub(crate) fn pinyin(
        l1: Cow<'static, [u8]>,
        l2: Cow<'static, [u8]>,
        strings: Cow<'static, str>,
        trim_space: bool,
    ) -> Self {
        let l1 = decode_u16_table(l1.as_ref());
        let mut l2 = decode_u32_table(l2.as_ref());
        if trim_space {
            for value in l2.iter_mut() {
                *value = trim_pinyin_packed(*value, strings.as_ref());
            }
        }
        SingleCharMatcher::Pinyin { l1, l2, strings }
    }

    /// Converts a codepoint→value map into a 2-stage page-table byte representation.
    ///
    /// Returns `(l1, l2)`. L1 is a `u16[4352]` array (one entry per
    /// 256-codepoint block); non-zero entries index into L2. L2 stores the `u32`
    /// values for each mapped codepoint.
    #[cfg(feature = "runtime_build")]
    fn build_2_stage_table(map: &HashMap<u32, u32>) -> (Vec<u16>, Vec<u32>) {
        let mut pages: HashSet<u32> = map.keys().map(|&k| k >> 8).collect();
        let mut page_list: Vec<u32> = pages.drain().collect();
        page_list.sort_unstable();
        const L1_SIZE: usize = (0x10FFFF >> 8) + 1; // 4352: one entry per 256-codepoint block
        let mut l1 = vec![0u16; L1_SIZE];
        let mut l2 = vec![0u32; (page_list.len() + 1) * 256];
        for (i, &page) in page_list.iter().enumerate() {
            let l2_page_idx = (i + 1) as u16;
            l1[page as usize] = l2_page_idx;
            for char_idx in 0..256u32 {
                let cp = (page << 8) | char_idx;
                if let Some(&val) = map.get(&cp) {
                    l2[(l2_page_idx as usize * 256) + char_idx as usize] = val;
                }
            }
        }
        (l1, l2)
    }

    /// Builds a Fanjian matcher from a codepoint→codepoint map.
    #[cfg(feature = "runtime_build")]
    pub(crate) fn fanjian_from_map(map: HashMap<u32, u32>) -> Self {
        let (l1, l2) = Self::build_2_stage_table(&map);
        Self::Fanjian {
            l1: l1.into_boxed_slice(),
            l2: l2.into_boxed_slice(),
        }
    }

    /// Builds a Delete matcher from text source and whitespace list.
    #[cfg(feature = "runtime_build")]
    pub(crate) fn delete_from_sources(text_delete: &str, white_space: &[&str]) -> Self {
        let mut bitset = vec![0u8; UNICODE_BITSET_SIZE];
        for line in text_delete.trim().lines() {
            for c in line.chars() {
                let cp = c as usize;
                bitset[cp / 8] |= 1 << (cp % 8);
            }
        }
        for &ws in white_space {
            for c in ws.chars() {
                let cp = c as usize;
                bitset[cp / 8] |= 1 << (cp % 8);
            }
        }
        Self::delete(Cow::Owned(bitset))
    }

    /// Builds a Pinyin matcher from a codepoint→syllable map.
    ///
    /// The constructor packs each syllable into a shared strings buffer and
    /// records `(offset, length)` as the L2 value.
    #[cfg(feature = "runtime_build")]
    pub(crate) fn pinyin_from_map(map: HashMap<u32, &str>, trim_space: bool) -> Self {
        let mut strings = String::new();
        let packed: HashMap<u32, u32> = map
            .into_iter()
            .map(|(k, v)| {
                let offset = strings.len() as u32;
                let length = v.len() as u32;
                strings.push_str(v);
                (k, (offset << 8) | length)
            })
            .collect();
        let (l1, l2) = Self::build_2_stage_table(&packed);
        let strings: Cow<'static, str> = Cow::Owned(strings);
        let mut l2 = l2.into_boxed_slice();
        if trim_space {
            for value in l2.iter_mut() {
                *value = trim_pinyin_packed(*value, strings.as_ref());
            }
        }
        Self::Pinyin {
            l1: l1.into_boxed_slice(),
            l2,
            strings,
        }
    }
}
