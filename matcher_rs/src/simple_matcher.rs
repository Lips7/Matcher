use std::borrow::Cow;
use std::intrinsics::{likely, unlikely};
use std::iter;
use std::simd::Simd;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind::DFA, MatchKind};
use gxhash::{HashMap as GxHashMap, HashSet as GxHashSet};
use nohash_hasher::{IntMap, IntSet};
use serde::Serialize;
use tinyvec::ArrayVec;

use super::{MatchResultTrait, StrConvType, TextMatcherTrait};

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

const WORD_COMBINATION_LIMIT: usize = 32;
const ZEROS: Simd<u8, WORD_COMBINATION_LIMIT> = Simd::from_array([0; WORD_COMBINATION_LIMIT]);

pub type SimpleMatchType = StrConvType;

pub type SimpleMatchTypeWordMap<'a> = GxHashMap<SimpleMatchType, IntMap<u64, &'a str>>;

struct WordConf {
    word: String,
    split_bit: Simd<u8, WORD_COMBINATION_LIMIT>,
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

impl MatchResultTrait<'_> for SimpleResult<'_> {
    fn word_id(&self) -> usize {
        self.word_id as usize
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

pub struct SimpleMatcher {
    simple_match_type_process_map: GxHashMap<SimpleMatchType, (Vec<&'static str>, AhoCorasick)>,
    simple_match_type_ac_table_map: GxHashMap<SimpleMatchType, SimpleAcTable>,
    simple_wordconf_map: IntMap<u64, WordConf>,
    min_chars_count: usize,
}

impl SimpleMatcher {
    pub fn new(simple_match_type_word_map: &SimpleMatchTypeWordMap) -> SimpleMatcher {
        let mut simple_matcher = SimpleMatcher {
            simple_match_type_process_map: GxHashMap::default(),
            simple_match_type_ac_table_map: GxHashMap::default(),
            simple_wordconf_map: IntMap::default(),
            min_chars_count: usize::MAX,
        };

        for (simple_match_type, simple_word_map) in simple_match_type_word_map {
            for simple_match_type_bit in simple_match_type.iter() {
                simple_matcher
                    .simple_match_type_process_map
                    .entry(simple_match_type_bit)
                    .or_insert_with(|| Self::_get_process_matcher(&simple_match_type_bit));
            }

            let simple_ac_table = simple_matcher.build_simple_ac_table(
                &(*simple_match_type - SimpleMatchType::TextDelete),
                simple_word_map,
            );

            simple_matcher.simple_match_type_ac_table_map.insert(
                *simple_match_type - SimpleMatchType::WordDelete,
                simple_ac_table,
            );
        }

        simple_matcher
    }

    fn _get_process_matcher(
        simple_match_type_bit: &SimpleMatchType,
    ) -> (Vec<&'static str>, AhoCorasick) {
        let mut process_dict = GxHashMap::default();

        match *simple_match_type_bit {
            SimpleMatchType::None => {}
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

    fn build_simple_ac_table<'a>(
        &mut self,
        simple_match_type: &SimpleMatchType,
        simple_word_map: &IntMap<u64, &'a str>,
    ) -> SimpleAcTable {
        let mut ac_wordlist = Vec::with_capacity(simple_word_map.len());
        let mut ac_word_conf_list = Vec::with_capacity(simple_word_map.len());

        for (&simple_word_id, &simple_word) in simple_word_map {
            self.min_chars_count = self.min_chars_count.min(
                simple_word
                    .chars()
                    .filter(|&c| c != ',')
                    .collect::<GxHashSet<char>>()
                    .len(),
            );

            let mut ac_split_word_counter = GxHashMap::default();
            for ac_split_word in simple_word.split(',').filter(|&x| !x.is_empty()) {
                ac_split_word_counter
                    .entry(ac_split_word)
                    .and_modify(|cnt| *cnt += 1)
                    .or_insert(1);
            }

            let split_bit_vec = ac_split_word_counter
                .values()
                .take(WORD_COMBINATION_LIMIT)
                .map(|&x| 1 << (x.min(8) - 1))
                .collect::<ArrayVec<[u8; 32]>>();
            let split_bit = Simd::load_or_default(&split_bit_vec);

            self.simple_wordconf_map.insert(
                simple_word_id,
                WordConf {
                    word: simple_word.to_owned(),
                    split_bit,
                },
            );

            for (offset, split_word) in ac_split_word_counter
                .keys()
                .take(WORD_COMBINATION_LIMIT)
                .enumerate()
            {
                for ac_word in self.reduce_text_process(simple_match_type, split_word.as_bytes()) {
                    ac_wordlist.push(ac_word);
                    ac_word_conf_list.push((simple_word_id, offset));
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
        simple_match_type: &SimpleMatchType,
        text_bytes: &'a [u8],
    ) -> ArrayVec<[Cow<'a, [u8]>; 4]> {
        let mut processed_text_bytes_list: ArrayVec<[Cow<'a, [u8]>; 4]> = ArrayVec::new();
        processed_text_bytes_list.push(Cow::Borrowed(text_bytes));

        for simple_match_type_bit in simple_match_type.iter() {
            let (process_replace_list, process_matcher) = unsafe {
                self.simple_match_type_process_map
                    .get(&simple_match_type_bit)
                    .unwrap_unchecked()
            };
            let tmp_processed_text_bytes =
                unsafe { processed_text_bytes_list.last_mut().unwrap_unchecked() };

            match simple_match_type_bit {
                SimpleMatchType::None => {}
                SimpleMatchType::Fanjian => {
                    if unlikely(process_matcher.is_match(tmp_processed_text_bytes.as_ref())) {
                        *tmp_processed_text_bytes = Cow::Owned(
                            process_matcher.replace_all_bytes(text_bytes, process_replace_list),
                        );
                    }
                }
                SimpleMatchType::TextDelete | SimpleMatchType::WordDelete => {
                    if likely(process_matcher.is_match(tmp_processed_text_bytes.as_ref())) {
                        let mut processed_text_bytes =
                            Vec::with_capacity(tmp_processed_text_bytes.len());
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
                }
                _ => {
                    if process_matcher.is_match(tmp_processed_text_bytes.as_ref()) {
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
        let text_bytes = text.as_bytes();

        if unlikely(bytecount::num_chars(text_bytes) < self.min_chars_count) {
            return false;
        }

        let mut word_id_split_bit_map = IntMap::default();

        for (simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            let processed_text_bytes_list = self.reduce_text_process(simple_match_type, text_bytes);
            let processed_times = processed_text_bytes_list.len();
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
                        unsafe { self.simple_wordconf_map.get(&word_id).unwrap_unchecked() };

                    let split_bit_vec = word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                        iter::repeat_n(word_conf.split_bit, processed_times)
                            .collect::<ArrayVec<[_; 4]>>()
                    });

                    *unsafe {
                        split_bit_vec
                            .get_unchecked_mut(index)
                            .as_mut_array()
                            .get_unchecked_mut(ac_word_conf.1)
                    } >>= 1;

                    if unlikely(
                        split_bit_vec
                            .iter()
                            .fold(Simd::splat(1), |acc, &bit| acc & bit)
                            == ZEROS,
                    ) {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn process(&'a self, text: &str) -> Vec<SimpleResult<'a>> {
        let text_bytes = text.as_bytes();
        let mut result_list = Vec::new();

        if unlikely(bytecount::num_chars(text_bytes) < self.min_chars_count) {
            return result_list;
        }

        let mut word_id_set = IntSet::default();
        let mut word_id_split_bit_map = IntMap::default();

        for (simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            let processed_text_bytes_list = self.reduce_text_process(simple_match_type, text_bytes);
            let processed_times = processed_text_bytes_list.len();
            for (index, processed_text) in processed_text_bytes_list.iter().enumerate() {
                for ac_result in simple_ac_table
                    .ac_matcher
                    .find_overlapping_iter(processed_text)
                {
                    let ac_word_conf = unsafe {
                        simple_ac_table
                            .ac_word_conf_list
                            .get_unchecked(ac_result.pattern().as_usize())
                    };
                    let word_id = ac_word_conf.0;

                    if word_id_set.contains(&word_id) {
                        continue;
                    }

                    let word_conf =
                        unsafe { self.simple_wordconf_map.get(&word_id).unwrap_unchecked() };

                    let split_bit_vec = word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                        iter::repeat_n(word_conf.split_bit, processed_times)
                            .collect::<ArrayVec<[_; 4]>>()
                    });

                    *unsafe {
                        split_bit_vec
                            .get_unchecked_mut(index)
                            .as_mut_array()
                            .get_unchecked_mut(ac_word_conf.1)
                    } >>= 1;

                    if unlikely(
                        split_bit_vec
                            .iter()
                            .fold(Simd::splat(1), |acc, &bit| acc & bit)
                            == ZEROS,
                    ) {
                        word_id_set.insert(word_id);
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
