use std::borrow::Cow;

use ahash::AHashMap;
use nohash_hasher::{IntMap, IntSet};
use ouroboros::self_referencing;
use serde::{Deserialize, Serialize};
use tinyvec::{ArrayVec, TinyVec};
use vectorscan_rs::{Database, Flag, Pattern, Scan, ScanMode, Scanner};

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
pub struct VectorWord<'a> {
    pub word_id: u64,
    pub word: &'a str,
}

pub type VectorMatchType = StrConvType;

pub type VectorWordlistDict<'a> = AHashMap<VectorMatchType, Vec<VectorWord<'a>>>;

struct WordConf {
    word: String,
    split_bit: TinyVec<[u64; 64]>,
}

#[self_referencing]
struct ReplaceTable {
    process_replace_list: Vec<&'static str>,
    database: Database,
    #[borrows(database)]
    #[not_covariant]
    scanner: Scanner<'this>,
}

#[self_referencing]
struct VectorTable {
    word_conf_list: Vec<(u64, usize)>,
    database: Database,
    #[borrows(database)]
    #[not_covariant]
    scanner: Scanner<'this>,
}

#[derive(Debug, Serialize)]
pub struct VectorResult<'a> {
    pub word_id: u64,
    pub word: Cow<'a, str>,
}

pub struct VectorMatcher {
    str_conv_process_dict: AHashMap<VectorMatchType, ReplaceTable>,
    vector_table_dict: AHashMap<VectorMatchType, VectorTable>,
    vector_word_map: IntMap<u64, WordConf>,
}

impl VectorMatcher {
    pub fn new(vector_wordlist_dict: &VectorWordlistDict) -> VectorMatcher {
        let mut vector_matcher = VectorMatcher {
            str_conv_process_dict: AHashMap::new(),
            vector_table_dict: AHashMap::new(),
            vector_word_map: IntMap::default(),
        };

        for (vector_match_type, vector_wordlist) in vector_wordlist_dict {
            for str_conv_type in vector_match_type.iter() {
                vector_matcher
                    .str_conv_process_dict
                    .entry(str_conv_type)
                    .or_insert_with(|| Self::_get_process_matcher(str_conv_type));
            }

            let word_str_conv_list = *vector_match_type - VectorMatchType::TextDelete;

            let vector_table =
                vector_matcher.build_vector_table(&word_str_conv_list, vector_wordlist);

            vector_matcher.vector_table_dict.insert(
                *vector_match_type - VectorMatchType::WordDelete,
                vector_table,
            );
        }

        vector_matcher
    }

    fn _get_process_matcher(str_conv_type: VectorMatchType) -> ReplaceTable {
        let mut process_dict = AHashMap::new();

        match str_conv_type {
            VectorMatchType::Fanjian => {
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
            VectorMatchType::WordDelete => {
                process_dict.extend(
                    PUNCTUATION_SPECIAL
                        .trim()
                        .split('\n')
                        .map(|pair_str| (pair_str, "")),
                );

                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }
            VectorMatchType::TextDelete => {
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
            VectorMatchType::Normalize => {
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
            VectorMatchType::PinYin => {
                process_dict.extend(PINYIN.trim().split('\n').map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }
            VectorMatchType::PinYinChar => {
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
            .retain(|&key, &mut value| (key == "#" || !key.starts_with('#')) && key != value && !key.is_empty());

        let process_database = Database::new(
            process_dict
                .keys()
                .into_iter()
                .enumerate()
                .map(|(id, &key)| {
                    Pattern::new(
                        key.as_bytes(),
                        Flag::CASELESS | Flag::SOM_LEFTMOST,
                        id as u32,
                    )
                })
                .collect(),
            ScanMode::BLOCK,
            true,
        )
        .unwrap();
        let process_replace_list = process_dict.iter().map(|(_, &val)| val).collect();

        ReplaceTableBuilder {
            process_replace_list,
            database: process_database,
            scanner_builder: |database: &Database| Scanner::new(database).unwrap(),
        }
        .build()
    }

    fn build_vector_table(
        &mut self,
        str_conv_type_list: &VectorMatchType,
        vector_wordlist: &Vec<VectorWord>,
    ) -> VectorTable {
        let mut wordlist = Vec::with_capacity(vector_wordlist.len());
        let mut word_conf_list = Vec::with_capacity(vector_wordlist.len());

        for vector_word in vector_wordlist {
            let mut split_word_counter: AHashMap<&str, u8> = AHashMap::new();
            for split_word in vector_word.word.split(',').filter(|x| !x.is_empty()) {
                split_word_counter
                    .entry(split_word)
                    .and_modify(|cnt| *cnt += 1)
                    .or_insert(1);
            }

            let split_bit = split_word_counter
                .values()
                .map(|&x| if x < 64 { 1 << (x - 1) } else { 1 << 63 })
                .collect();

            self.vector_word_map.insert(
                vector_word.word_id,
                WordConf {
                    word: vector_word.word.to_owned(),
                    split_bit,
                },
            );

            for (offset, split_word) in split_word_counter.keys().enumerate() {
                for word in self.reduce_text_process(str_conv_type_list, split_word.as_bytes()) {
                    wordlist.push(word.into_owned());
                    word_conf_list.push((vector_word.word_id, offset));
                }
            }
        }

        VectorTableBuilder {
            word_conf_list,
            database: Database::new(
                wordlist
                    .iter()
                    .enumerate()
                    .map(|(id, word)| {
                        Pattern::new(word, Flag::CASELESS, id as u32)
                    })
                    .collect(),
                ScanMode::BLOCK,
                true,
            )
            .unwrap(),
            scanner_builder: |database: &Database| Scanner::new(database).unwrap(),
        }
        .build()
    }

    #[inline]
    fn replace_all_bytes(
        &self,
        text_bytes: &[u8],
        scanner: &Scanner,
        process_replace_list: &Vec<&str>,
    ) -> Vec<u8> {
        let mut processed_text_bytes = Vec::with_capacity(text_bytes.len());
        let mut last_match = 0;
        let _ = scanner.scan(text_bytes, |rule_id, from, to, _| {
            processed_text_bytes.extend(&text_bytes[last_match..from as usize]);
            last_match = to as usize;
            processed_text_bytes
                .extend(unsafe { process_replace_list.get_unchecked(rule_id as usize) }.as_bytes());
            Scan::Continue
        });
        processed_text_bytes.extend(&text_bytes[last_match..]);

        processed_text_bytes
    }

    #[inline]
    fn reduce_text_process<'a>(
        &self,
        str_conv_type_list: &VectorMatchType,
        text_bytes: &'a [u8],
    ) -> ArrayVec<[Cow<'a, [u8]>; 4]> {
        let mut processed_text_bytes_list: ArrayVec<[Cow<'a, [u8]>; 4]> = ArrayVec::new();
        processed_text_bytes_list.push(Cow::Borrowed(text_bytes));

        for str_conv_type in str_conv_type_list.iter() {
            let replace_table = unsafe {
                self.str_conv_process_dict
                    .get(&str_conv_type)
                    .unwrap_unchecked()
            };
            let process_replace_list = replace_table.borrow_process_replace_list();

            let tmp_processed_text_bytes =
                unsafe { processed_text_bytes_list.last_mut().unwrap_unchecked() };

            let mut match_flag = false;
            let _ = replace_table.with_scanner(|scanner| {
                scanner.scan(tmp_processed_text_bytes, |_, _, _, _| {
                    match_flag = true;
                    Scan::Terminate
                })
            });

            if match_flag {
                match str_conv_type {
                    VectorMatchType::Fanjian => {
                        *tmp_processed_text_bytes =
                            Cow::Owned(replace_table.with_scanner(|scanner| {
                                self.replace_all_bytes(text_bytes, scanner, process_replace_list)
                            }));
                    }
                    VectorMatchType::TextDelete | VectorMatchType::WordDelete => {
                        let mut processed_text_bytes =
                            Vec::with_capacity(tmp_processed_text_bytes.len());
                        let mut last_match = 0;
                        replace_table.with_scanner(|scanner| {
                            let _ = scanner.scan(tmp_processed_text_bytes, |_, from, to, _| {
                                processed_text_bytes
                                    .extend(&tmp_processed_text_bytes[last_match..from as usize]);
                                last_match = to as usize;
                                Scan::Continue
                            });
                        });
                        processed_text_bytes.extend(&tmp_processed_text_bytes[last_match..]);

                        processed_text_bytes_list.push(Cow::Owned(processed_text_bytes))
                    }
                    _ => {
                        processed_text_bytes_list.push(Cow::Owned(replace_table.with_scanner(
                            |scanner| {
                                self.replace_all_bytes(text_bytes, scanner, process_replace_list)
                            },
                        )));
                    }
                }
            }
        }

        processed_text_bytes_list
    }
}

impl<'a> TextMatcherTrait<'a, VectorResult<'a>> for VectorMatcher {
    fn is_match(&self, text: &str) -> bool {
        !self.process(text).is_empty()
    }

    fn process(&'a self, text: &str) -> Vec<VectorResult<'a>> {
        let text_bytes = text.as_bytes();
        let mut result_list = Vec::new();

        let mut word_id_set = IntSet::default();
        let mut word_id_split_bit_map = IntMap::default();

        for (vector_match_type, vector_table) in &self.vector_table_dict {
            let processed_text_bytes_list = self.reduce_text_process(vector_match_type, text_bytes);
            for (index, processed_text) in processed_text_bytes_list.iter().enumerate() {
                vector_table.with_scanner(|scanner| {
                    let _ = scanner.scan(&processed_text, |word_id, _, _, _| {
                        let match_word_conf = unsafe {
                            vector_table
                                .borrow_word_conf_list()
                                .get_unchecked(word_id as usize)
                        };
                        let word_id = match_word_conf.0;
                        let word_conf =
                            unsafe { self.vector_word_map.get(&word_id).unwrap_unchecked() };

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
                                .get_unchecked_mut(match_word_conf.1)
                                .get_unchecked_mut(index)
                        } >>= 1;

                        if split_bit.iter().all(|bit| bit.iter().any(|&b| b == 0))
                            && !word_id_set.contains(&word_id)
                        {
                            word_id_set.insert(word_id);
                            result_list.push(VectorResult {
                                word_id,
                                word: Cow::Borrowed(&word_conf.word),
                            });
                        }
                        Scan::Continue
                    });
                });
            }
        }

        result_list
    }
}
