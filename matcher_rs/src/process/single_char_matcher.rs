use std::borrow::Cow;
#[cfg(feature = "runtime_build")]
use std::collections::HashMap;
#[cfg(feature = "runtime_build")]
use std::collections::HashSet;

/// Single-character lookup engine backed by compact, pre-compiled data structures.
///
/// Each variant provides O(1) per-codepoint dispatch with no state-machine overhead.
/// Instances are constructed by [`get_process_matcher`](crate::get_process_matcher) and
/// cached for the lifetime of the program.
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
pub enum SingleCharMatcher {
    /// Traditional Chinese → Simplified Chinese via a 2-stage page table.
    ///
    /// * `l1` — L1 index: `u16[4352]`, one entry per 256-codepoint block. Non-zero entries
    ///   point to a page in `l2`.
    /// * `l2` — L2 data: `u32[num_pages * 256]`. Each entry is the mapped codepoint, or
    ///   `0` if the source codepoint has no mapping (i.e. already Simplified).
    Fanjian {
        l1: Cow<'static, [u8]>,
        l2: Cow<'static, [u8]>,
    },
    /// Chinese character → Pinyin syllable(s) via a 2-stage page table.
    ///
    /// * `l1` / `l2` — same page-table structure as `Fanjian`, but each L2 value packs
    ///   `(offset << 8) | length` pointing into `strings`.
    /// * `strings` — concatenated Pinyin syllables (e.g. `"zhong guo ..."`) with space
    ///   separators between syllables.
    /// * `trim_space` — when `true` (used by `PinYinChar`), leading/trailing spaces are
    ///   stripped from each syllable slice before yielding.
    Pinyin {
        l1: Cow<'static, [u8]>,
        l2: Cow<'static, [u8]>,
        strings: Cow<'static, str>,
        trim_space: bool,
    },
    /// Character deletion via a 139 KB flat BitSet covering all Unicode planes.
    ///
    /// * `bitset` — `u8[139264]`; bit `cp % 8` of byte `cp / 8` is set if codepoint
    ///   `cp` should be removed. Covers codepoints 0x0 – 0x10FFFF.
    Delete { bitset: Cow<'static, [u8]> },
}

/// The transformation to apply to a matched codepoint, yielded by [`SingleCharFindIter`].
pub enum SingleCharMatch<'a> {
    Char(char),
    Str(&'a str),
    Delete,
}

/// An iterator over single-character matches in a text string.
///
/// Scans `text` character-by-character, yielding `(start, end, `[`SingleCharMatch`]`)` tuples
/// for each codepoint that the underlying [`SingleCharMatcher`] maps to a transformation.
/// `start` and `end` are byte offsets into the original `text` slice.
pub struct SingleCharFindIter<'a> {
    matcher: &'a SingleCharMatcher,
    text: &'a str,
    byte_offset: usize,
}

impl<'a> SingleCharFindIter<'a> {
    #[inline(always)]
    pub fn new(matcher: &'a SingleCharMatcher, text: &'a str) -> Self {
        Self {
            matcher,
            text,
            byte_offset: 0,
        }
    }
}

/// Looks up a Unicode codepoint in a 2-stage page table, returning the packed value or `None`.
#[inline(always)]
fn page_table_lookup(cp: u32, l1: &[u8], l2: &[u8]) -> Option<u32> {
    let page_idx = (cp >> 8) as usize;
    let char_idx = (cp & 0xFF) as usize;
    if page_idx * 2 + 1 >= l1.len() {
        return None;
    }
    let page = u16::from_le_bytes(l1[page_idx * 2..page_idx * 2 + 2].try_into().unwrap()) as usize;
    if page == 0 {
        return None;
    }
    let l2_idx = page * 256 + char_idx;
    let val = u32::from_le_bytes(l2[l2_idx * 4..l2_idx * 4 + 4].try_into().unwrap());
    if val != 0 { Some(val) } else { None }
}

impl<'a> Iterator for SingleCharFindIter<'a> {
    type Item = (usize, usize, SingleCharMatch<'a>);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let text = &self.text[self.byte_offset..];
        for (i, c) in text.char_indices() {
            let cp = c as u32;
            let start = self.byte_offset + i;
            let end = start + c.len_utf8();

            match self.matcher {
                SingleCharMatcher::Fanjian { l1, l2 } => {
                    // Fanjian only maps CJK codepoints (U+4E00+); all ASCII is unchanged.
                    if cp < 0x80 {
                        continue;
                    }
                    if let Some(mapped_cp) = page_table_lookup(cp, l1, l2) {
                        let mapped = char::from_u32(mapped_cp).unwrap_or(c);
                        if mapped != c {
                            self.byte_offset = end;
                            return Some((start, end, SingleCharMatch::Char(mapped)));
                        }
                    }
                }
                SingleCharMatcher::Pinyin {
                    l1,
                    l2,
                    strings,
                    trim_space,
                } => {
                    // Pinyin maps CJK codepoints and ASCII digits (0-9).
                    if cp < 0x80 && !c.is_ascii_digit() {
                        continue;
                    }
                    if let Some(val) = page_table_lookup(cp, l1, l2) {
                        let offset = (val >> 8) as usize;
                        let len = (val & 0xFF) as usize;
                        if offset + len <= strings.len() {
                            let mut s = &strings[offset..offset + len];
                            if *trim_space {
                                s = s.trim();
                            }
                            self.byte_offset = end;
                            return Some((start, end, SingleCharMatch::Str(s)));
                        }
                    }
                }
                SingleCharMatcher::Delete { bitset } => {
                    let cp_usize = cp as usize;
                    if cp_usize / 8 < bitset.len()
                        && (bitset[cp_usize / 8] & (1 << (cp_usize % 8))) != 0
                    {
                        self.byte_offset = end;
                        return Some((start, end, SingleCharMatch::Delete));
                    }
                }
            }
        }
        self.byte_offset = self.text.len();
        None
    }
}

impl SingleCharMatcher {
    /// Returns an iterator over all codepoints in `text` that this matcher transforms.
    ///
    /// Each item is `(start_byte, end_byte, `[`SingleCharMatch`]`)`. Characters with no
    /// mapping are skipped. The iterator runs in O(n) time over the input length.
    #[inline(always)]
    pub fn find_iter<'a>(&'a self, text: &'a str) -> SingleCharFindIter<'a> {
        SingleCharFindIter::new(self, text)
    }

    pub fn fanjian(l1: Cow<'static, [u8]>, l2: Cow<'static, [u8]>) -> Self {
        SingleCharMatcher::Fanjian { l1, l2 }
    }

    pub fn delete(bitset: Cow<'static, [u8]>) -> Self {
        SingleCharMatcher::Delete { bitset }
    }

    pub fn pinyin(
        l1: Cow<'static, [u8]>,
        l2: Cow<'static, [u8]>,
        strings: Cow<'static, str>,
        trim_space: bool,
    ) -> Self {
        SingleCharMatcher::Pinyin {
            l1,
            l2,
            strings,
            trim_space,
        }
    }

    /// Converts a codepoint→value map into a 2-stage page-table byte representation.
    ///
    /// Returns `(l1_bytes, l2_bytes)`. L1 is a `u16[4352]` array (one entry per
    /// 256-codepoint block); non-zero entries index into L2. L2 stores the `u32`
    /// values for each mapped codepoint.
    #[cfg(feature = "runtime_build")]
    fn build_2_stage_table(map: &HashMap<u32, u32>) -> (Vec<u8>, Vec<u8>) {
        let mut pages: HashSet<u32> = map.keys().map(|&k| k >> 8).collect();
        let mut page_list: Vec<u32> = pages.drain().collect();
        page_list.sort_unstable();
        let mut l1 = vec![0u16; 4352];
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
        let mut l1_bytes = Vec::with_capacity(l1.len() * 2);
        for val in l1 {
            l1_bytes.extend_from_slice(&val.to_le_bytes());
        }
        let mut l2_bytes = Vec::with_capacity(l2.len() * 4);
        for val in l2 {
            l2_bytes.extend_from_slice(&val.to_le_bytes());
        }
        (l1_bytes, l2_bytes)
    }

    /// Builds a Fanjian matcher from a codepoint→codepoint map.
    #[cfg(feature = "runtime_build")]
    pub fn fanjian_from_map(map: HashMap<u32, u32>) -> Self {
        let (l1, l2) = Self::build_2_stage_table(&map);
        Self::fanjian(Cow::Owned(l1), Cow::Owned(l2))
    }

    /// Builds a Delete matcher from text source and whitespace list.
    #[cfg(feature = "runtime_build")]
    pub fn delete_from_sources(text_delete: &str, white_space: &[&str]) -> Self {
        let mut bitset = vec![0u8; 139264];
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
    pub fn pinyin_from_map(map: HashMap<u32, &str>, trim_space: bool) -> Self {
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
        Self::pinyin(
            Cow::Owned(l1),
            Cow::Owned(l2),
            Cow::Owned(strings),
            trim_space,
        )
    }
}
