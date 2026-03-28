//! Charwise lookup engines for Fanjian and Pinyin transformations.

use std::borrow::Cow;
#[cfg(feature = "runtime_build")]
use std::collections::{HashMap, HashSet};

use crate::process::transform::simd::{skip_ascii_simd, skip_non_digit_ascii_simd};
use crate::process::variant::{get_string_from_pool, return_string_to_pool};

enum Replacement<'a> {
    Char(char),
    Str(&'a str),
}

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
    debug_assert!(page * 256 + char_idx < l2.len());
    let value = unsafe { *l2.get_unchecked(page * 256 + char_idx) };
    (value != 0).then_some(value)
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

#[inline(always)]
unsafe fn decode_utf8_raw(bytes: &[u8], offset: usize) -> (u32, usize) {
    let b0 = unsafe { *bytes.get_unchecked(offset) };
    if b0 < 0xE0 {
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        (((b0 as u32 & 0x1F) << 6) | (b1 as u32 & 0x3F), 2)
    } else if b0 < 0xF0 {
        let b1 = unsafe { *bytes.get_unchecked(offset + 1) };
        let b2 = unsafe { *bytes.get_unchecked(offset + 2) };
        (
            ((b0 as u32 & 0x0F) << 12) | ((b1 as u32 & 0x3F) << 6) | (b2 as u32 & 0x3F),
            3,
        )
    } else {
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

struct FanjianFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for FanjianFindIter<'a> {
    type Item = (usize, usize, Replacement<'a>);

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
            let (cp, char_len) = unsafe { decode_utf8_raw(bytes, start) };
            self.byte_offset += char_len;

            if let Some(mapped_cp) = page_table_lookup(cp, self.l1, self.l2)
                && mapped_cp != cp
            {
                debug_assert!(char::from_u32(mapped_cp).is_some());
                let mapped = unsafe { char::from_u32_unchecked(mapped_cp) };
                return Some((start, self.byte_offset, Replacement::Char(mapped)));
            }
        }
    }
}

struct PinyinFindIter<'a> {
    l1: &'a [u16],
    l2: &'a [u32],
    strings: &'a str,
    text: &'a str,
    byte_offset: usize,
}

impl<'a> Iterator for PinyinFindIter<'a> {
    type Item = (usize, usize, Replacement<'a>);

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

#[derive(Clone)]
pub(crate) struct FanjianMatcher {
    l1: Box<[u16]>,
    l2: Box<[u32]>,
}

impl FanjianMatcher {
    #[inline(always)]
    fn iter<'a>(&'a self, text: &'a str) -> FanjianFindIter<'a> {
        FanjianFindIter {
            l1: &self.l1,
            l2: &self.l2,
            text,
            byte_offset: 0,
        }
    }

    pub(crate) fn replace(&self, text: &str) -> Option<String> {
        let mut result: Option<String> = None;

        for (start, end, replacement) in self.iter(text) {
            let Replacement::Char(mapped) = replacement else {
                unreachable!("fanjian iter yields char replacements");
            };
            let span_len = end - start;
            if mapped.len_utf8() == span_len {
                let buf = result.get_or_insert_with(|| {
                    let mut s = get_string_from_pool(text.len());
                    s.push_str(text);
                    s
                });
                unsafe { mapped.encode_utf8(&mut buf.as_bytes_mut()[start..end]) };
            } else {
                if let Some(existing) = result.take() {
                    return_string_to_pool(existing);
                }
                return replace_scan(text, self.iter(text), |result, replacement| {
                    let Replacement::Char(mapped) = replacement else {
                        unreachable!("fanjian iter yields char replacements");
                    };
                    result.push(mapped);
                });
            }
        }

        result
    }

    #[cfg(not(feature = "runtime_build"))]
    pub(crate) fn new(l1: &'static [u8], l2: &'static [u8]) -> Self {
        Self {
            l1: decode_u16_table(l1),
            l2: decode_u32_table(l2),
        }
    }

    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_map(map: HashMap<u32, u32>) -> Self {
        let (l1, l2) = build_2_stage_table(&map);
        Self {
            l1: l1.into_boxed_slice(),
            l2: l2.into_boxed_slice(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct PinyinMatcher {
    l1: Box<[u16]>,
    l2: Box<[u32]>,
    strings: Cow<'static, str>,
}

impl PinyinMatcher {
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

    pub(crate) fn replace(&self, text: &str) -> Option<String> {
        replace_scan(text, self.iter(text), |result, replacement| {
            let Replacement::Str(mapped) = replacement else {
                unreachable!("pinyin iter yields string replacements");
            };
            result.push_str(mapped);
        })
    }

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

    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_map(map: HashMap<u32, &str>, trim_space: bool) -> Self {
        let mut strings = String::new();
        let packed: HashMap<u32, u32> = map
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

#[cfg(feature = "runtime_build")]
fn build_2_stage_table(map: &HashMap<u32, u32>) -> (Vec<u16>, Vec<u32>) {
    let mut pages: HashSet<u32> = map.keys().map(|&key| key >> 8).collect();
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
