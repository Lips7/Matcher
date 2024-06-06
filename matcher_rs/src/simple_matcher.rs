use std::borrow::Cow;
use std::intrinsics::{likely, unlikely};

use gxhash::{HashMap as GxHashMap, HashSet as GxHashSet};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind::DFA, MatchKind};
use nohash_hasher::{IntMap, IntSet};
use serde::{Deserialize, Serialize};
use tinyvec::{ArrayVec, TinyVec};

use super::{StrConvType, TextMatcherTrait};

const FANJIAN: &str = include_str!("../str_conv_dat/RASEMAT-FANJIAN.txt");
const CN_SPECIAL: &str = include_str!("../str_conv_dat/RASEMAT-CN-SPECIAL.txt");
const EN_SPECIAL: &str = include_str!("../str_conv_dat/RASEMAT-EN-SPECIAL.txt");
const PUNCTUATION_SPECIAL: &str = include_str!("../str_conv_dat/RASEMAT-PUNCTUATION-SPECIAL.txt");
const EN_VARIATION: &str = include_str!("../str_conv_dat/RASEMAT-EN-VARIATION.txt");
const UNICODE: &str = include_str!("../str_conv_dat/RASEMAT-UNICODE.txt");
const NUM_NORM: &str = include_str!("../str_conv_dat/RASEMAT-NUM-NORM.txt");
const UPPER_LOWER: &str = include_str!("../str_conv_dat/RASEMAT-UPPER-LOWER.txt");
const PINYIN: &str = include_str!("../str_conv_dat/RASEMAT-PINYIN.txt");
const PINYIN_CHAR: &str = include_str!("../str_conv_dat/RASEMAT-PINYIN-CHAR.txt");

const WHITE_SPACE: &[&str] = &[
    "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}", "\u{00A0}",
    "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}", "\u{2005}", "\u{2006}",
    "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}",
    "\u{3000}",
];

#[derive(Serialize, Deserialize)]
pub struct SimpleWord<'a> {
    pub word_id: u64,
    pub word: &'a str,
}

pub type SimpleMatchType = StrConvType;

pub type SimpleWordlistDict<'a> = GxHashMap<SimpleMatchType, Vec<SimpleWord<'a>>>;

struct WordConf {
    word: String,
    split_bit: TinyVec<[u64; 64]>,
}

struct SimpleAcTable {
    ac_matcher: AhoCorasick,
    ac_word_conf_list: Vec<(u64, usize)>,
}

#[derive(Debug, Serialize)]
pub struct SimpleResult<'a> {
    pub word_id: u64,
    pub word: Cow<'a, str>,
}

pub struct SimpleMatcher {
    str_conv_process_dict: GxHashMap<SimpleMatchType, (Vec<&'static str>, AhoCorasick)>,
    simple_ac_table_dict: GxHashMap<SimpleMatchType, SimpleAcTable>,
    simple_word_map: IntMap<u64, WordConf>,
    min_text_len: usize,
}

impl SimpleMatcher {
    pub fn new(simple_wordlist_dict: &SimpleWordlistDict) -> SimpleMatcher {
        let mut simple_matcher = SimpleMatcher {
            str_conv_process_dict: GxHashMap::default(),
            simple_ac_table_dict: GxHashMap::default(),
            simple_word_map: IntMap::default(),
            min_text_len: 255,
        };

        for (simple_match_type, simple_wordlist) in simple_wordlist_dict {
            for str_conv_type in simple_match_type.iter() {
                simple_matcher
                    .str_conv_process_dict
                    .entry(str_conv_type)
                    .or_insert_with(|| Self::_get_process_matcher(str_conv_type));
            }

            let word_str_conv_list = *simple_match_type - SimpleMatchType::TextDelete;

            let simple_ac_table =
                simple_matcher.build_simple_ac_table(&word_str_conv_list, simple_wordlist);

            simple_matcher.simple_ac_table_dict.insert(
                *simple_match_type - SimpleMatchType::WordDelete,
                simple_ac_table,
            );
        }

        simple_matcher
    }

    fn _get_process_matcher(str_conv_type: SimpleMatchType) -> (Vec<&'static str>, AhoCorasick) {
        let mut process_dict = GxHashMap::default();

        match str_conv_type {
            SimpleMatchType::Fanjian => {
                for str_conv_dat in [FANJIAN, UNICODE] {
                    process_dict.extend(str_conv_dat.trim().split('\n').map(|pair_str| {
                        let mut pair_str_split = pair_str.split('\t');
                        (
                            pair_str_split.next().unwrap(),
                            pair_str_split.next().unwrap(),
                        )
                    }));
                }
            }
            SimpleMatchType::WordDelete => {
                process_dict.extend(
                    PUNCTUATION_SPECIAL
                        .trim()
                        .split('\n')
                        .map(|pair_str| (pair_str, "")),
                );

                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }
            SimpleMatchType::TextDelete => {
                for str_conv_dat in [PUNCTUATION_SPECIAL, CN_SPECIAL, EN_SPECIAL] {
                    process_dict.extend(
                        str_conv_dat
                            .trim()
                            .split('\n')
                            .map(|pair_str| (pair_str, "")),
                    );
                }

                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }
            SimpleMatchType::Normalize => {
                for str_conv_dat in [UPPER_LOWER, EN_VARIATION, NUM_NORM] {
                    process_dict.extend(str_conv_dat.trim().split('\n').map(|pair_str| {
                        let mut pair_str_split = pair_str.split('\t');
                        (
                            pair_str_split.next().unwrap(),
                            pair_str_split.next().unwrap(),
                        )
                    }));
                }
            }
            SimpleMatchType::PinYin => {
                process_dict.extend(PINYIN.trim().split('\n').map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }
            SimpleMatchType::PinYinChar => {
                process_dict.extend(PINYIN_CHAR.trim().split('\n').map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }
            _ => {}
        }

        process_dict
            .retain(|&key, &mut value| (key == "#" || !key.starts_with('#')) && key != value);

        let process_matcher = AhoCorasickBuilder::new()
            .kind(Some(DFA))
            .match_kind(MatchKind::LeftmostLongest)
            .build(
                process_dict
                    .iter()
                    .map(|(&key, _)| key)
                    .collect::<Vec<&str>>(),
            )
            .unwrap();
        let process_replace_list = process_dict.iter().map(|(_, &val)| val).collect();

        (process_replace_list, process_matcher)
    }

    fn build_simple_ac_table(
        &mut self,
        str_conv_type_list: &SimpleMatchType,
        simple_wordlist: &Vec<SimpleWord>,
    ) -> SimpleAcTable {
        let mut ac_wordlist = Vec::with_capacity(simple_wordlist.len());
        let mut ac_word_conf_list = Vec::with_capacity(simple_wordlist.len());

        for simple_word in simple_wordlist {
            let char_unique_cnt = simple_word
                .word
                .chars()
                .filter(|&c| c != ',')
                .collect::<GxHashSet<char>>()
                .len();

            if self.min_text_len > char_unique_cnt {
                self.min_text_len = char_unique_cnt;
            }

            let mut ac_split_word_counter: GxHashMap<&str, u8> = GxHashMap::default();
            for ac_split_word in simple_word.word.split(',').filter(|&x| !x.is_empty()) {
                ac_split_word_counter
                    .entry(ac_split_word)
                    .and_modify(|cnt| *cnt += 1)
                    .or_insert(1);
            }

            let split_bit = ac_split_word_counter
                .values()
                .map(|&x| if x < 64 { 1 << (x - 1) } else { 1 << 63 })
                .collect();

            self.simple_word_map.insert(
                simple_word.word_id,
                WordConf {
                    word: simple_word.word.to_owned(),
                    split_bit,
                },
            );

            for (offset, split_word) in ac_split_word_counter.keys().enumerate() {
                for ac_word in self.reduce_text_process(str_conv_type_list, split_word.as_bytes()) {
                    ac_wordlist.push(ac_word.into_owned());
                    ac_word_conf_list.push((simple_word.word_id, offset));
                }
            }
        }

        SimpleAcTable {
            ac_matcher: AhoCorasickBuilder::new()
                .kind(Some(DFA))
                .ascii_case_insensitive(true)
                .build(&ac_wordlist)
                .unwrap(),
            ac_word_conf_list,
        }
    }

    #[inline]
    fn reduce_text_process<'a>(
        &self,
        str_conv_type_list: &SimpleMatchType,
        text_bytes: &'a [u8],
    ) -> ArrayVec<[Cow<'a, [u8]>; 4]> {
        let mut processed_text_bytes_list: ArrayVec<[Cow<'a, [u8]>; 4]> = ArrayVec::new();
        processed_text_bytes_list.push(Cow::Borrowed(text_bytes));

        for str_conv_type in str_conv_type_list.iter() {
            let (process_replace_list, process_matcher) = unsafe {
                self.str_conv_process_dict
                    .get(&str_conv_type)
                    .unwrap_unchecked()
            };
            let tmp_processed_text_bytes =
                unsafe { processed_text_bytes_list.last_mut().unwrap_unchecked() };

            if likely(process_matcher.is_match(tmp_processed_text_bytes.as_ref())) {
                match str_conv_type {
                    SimpleMatchType::Fanjian => {
                        *tmp_processed_text_bytes = Cow::Owned(
                            process_matcher.replace_all_bytes(text_bytes, process_replace_list),
                        );
                    }
                    SimpleMatchType::TextDelete | SimpleMatchType::WordDelete => {
                        let mut processed_text_bytes = Vec::with_capacity(tmp_processed_text_bytes.len());
                        let mut last_match = 0;

                        for mat in process_matcher.find_iter(tmp_processed_text_bytes.as_ref()) {
                            processed_text_bytes.extend(unsafe {
                                tmp_processed_text_bytes.get_unchecked(last_match..mat.start())
                            });
                            last_match = mat.end();
                        }
                        processed_text_bytes.extend(unsafe {
                            tmp_processed_text_bytes.get_unchecked(last_match..)
                        });

                        processed_text_bytes_list.push(Cow::Owned(processed_text_bytes));
                    }
                    _ => {
                        let processed_text_bytes = process_matcher
                            .replace_all_bytes(tmp_processed_text_bytes, process_replace_list);
                        processed_text_bytes_list.push(Cow::Owned(processed_text_bytes));
                    }
                }
            }
        }

        processed_text_bytes_list
    }
}

impl<'a> TextMatcherTrait<'a, SimpleResult<'a>> for SimpleMatcher {
    fn is_match(&self, text: &str) -> bool {
        !self.process(text).is_empty()
    }

    fn process(&'a self, text: &str) -> Vec<SimpleResult<'a>> {
        let text_bytes = text.as_bytes();
        let mut result_list = Vec::new();

        if unlikely(bytecount::num_chars(text_bytes) < self.min_text_len) {
            return result_list;
        }

        let mut word_id_set = IntSet::default();
        let mut word_id_split_bit_map = IntMap::default();

        for (simple_match_type, simple_ac_table) in &self.simple_ac_table_dict {
            let processed_text_bytes_list = self.reduce_text_process(simple_match_type, text_bytes);
            for (index, processed_text) in processed_text_bytes_list.iter().enumerate() {
                for ac_result in simple_ac_table
                    .ac_matcher
                    .find_overlapping_iter(processed_text)
                {
                    let ac_word_id = ac_result.pattern().as_usize();
                    let ac_word_conf =
                        unsafe { simple_ac_table.ac_word_conf_list.get_unchecked(ac_word_id) };
                    let word_id = ac_word_conf.0;
                    let word_conf =
                        unsafe { self.simple_word_map.get(&word_id).unwrap_unchecked() };

                    let split_bit = word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                        word_conf
                            .split_bit
                            .iter()
                            .map(|&x| {
                                processed_text_bytes_list
                                    .iter()
                                    .map(|_| x)
                                    .collect::<ArrayVec<[u64; 4]>>()
                            })
                            .collect::<TinyVec<[_; 64]>>()
                    });

                    *unsafe {
                        split_bit
                            .get_unchecked_mut(ac_word_conf.1)
                            .get_unchecked_mut(index)
                    } >>= 1;

                    if unlikely(split_bit.iter().all(|bit| bit.iter().any(|&b| b == 0))
                        && word_id_set.insert(word_id))
                    {
                        result_list.push(SimpleResult {
                            word_id,
                            word: Cow::Borrowed(&word_conf.word),
                        });
                    }
                }
            }
        }

        result_list
    }
}
