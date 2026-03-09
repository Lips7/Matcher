use std::borrow::Cow;

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
///
/// * `Char(char)` — replace the source codepoint with a single character (Fanjian).
/// * `Str(&str)` — replace the source codepoint with a string slice (Pinyin).
/// * `Delete` — remove the source codepoint entirely (Delete).
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
    /// Creates a new [`SingleCharFindIter`] anchored at the start of `text`.
    ///
    /// # Arguments
    /// * `matcher` - The [`SingleCharMatcher`] that defines the transformation to apply.
    /// * `text` - The input string to scan.
    #[inline(always)]
    pub fn new(matcher: &'a SingleCharMatcher, text: &'a str) -> Self {
        Self {
            matcher,
            text,
            byte_offset: 0,
        }
    }
}

impl<'a> Iterator for SingleCharFindIter<'a> {
    type Item = (usize, usize, SingleCharMatch<'a>);

    /// Advances the iterator to the next matching codepoint.
    ///
    /// Resumes scanning from where the previous call left off. For each character
    /// that the underlying [`SingleCharMatcher`] maps to a transformation, the
    /// iterator yields `(start_byte, end_byte, match)` and suspends; characters
    /// with no mapping are skipped silently.
    ///
    /// Returns [`None`] when the end of the text is reached.
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        let text = &self.text[self.byte_offset..];
        for (i, c) in text.char_indices() {
            let cp = c as u32;
            let start = self.byte_offset + i;
            let end = start + c.len_utf8();

            match self.matcher {
                SingleCharMatcher::Fanjian { l1, l2 } => {
                    let page_idx = (cp >> 8) as usize;
                    let char_idx = (cp & 0xFF) as usize;
                    if page_idx * 2 + 1 < l1.len() {
                        let page = u16::from_le_bytes(
                            l1[page_idx * 2..page_idx * 2 + 2].try_into().unwrap(),
                        ) as usize;
                        if page != 0 {
                            let l2_idx = page * 256 + char_idx;
                            let mapped_cp = u32::from_le_bytes(
                                l2[l2_idx * 4..l2_idx * 4 + 4].try_into().unwrap(),
                            );
                            if mapped_cp != 0 {
                                let mapped = char::from_u32(mapped_cp).unwrap_or(c);
                                if mapped != c {
                                    self.byte_offset = end;
                                    return Some((start, end, SingleCharMatch::Char(mapped)));
                                }
                            }
                        }
                    }
                }
                SingleCharMatcher::Pinyin {
                    l1,
                    l2,
                    strings,
                    trim_space,
                } => {
                    let page_idx = (cp >> 8) as usize;
                    let char_idx = (cp & 0xFF) as usize;
                    if page_idx * 2 + 1 < l1.len() {
                        let page = u16::from_le_bytes(
                            l1[page_idx * 2..page_idx * 2 + 2].try_into().unwrap(),
                        ) as usize;
                        if page != 0 {
                            let l2_idx = page * 256 + char_idx;
                            let val = u32::from_le_bytes(
                                l2[l2_idx * 4..l2_idx * 4 + 4].try_into().unwrap(),
                            );
                            if val != 0 {
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
}
