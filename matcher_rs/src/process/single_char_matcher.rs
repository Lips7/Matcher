use std::borrow::Cow;

#[derive(Clone)]
pub enum SingleCharMatcher {
    Fanjian {
        l1: Cow<'static, [u8]>,
        l2: Cow<'static, [u8]>,
    },
    Pinyin {
        l1: Cow<'static, [u8]>,
        l2: Cow<'static, [u8]>,
        strings: Cow<'static, str>,
        trim_space: bool,
    },
    Delete {
        bitset: Cow<'static, [u8]>,
    },
}

pub enum SingleCharMatch<'a> {
    Char(char),
    Str(&'a str),
    Delete,
}

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
    #[inline(always)]
    pub fn find_iter<'a>(&'a self, text: &'a str) -> SingleCharFindIter<'a> {
        SingleCharFindIter::new(self, text)
    }
}
