use std::borrow::Cow;
use std::collections::HashMap;

use bitflags::bitflags;
use gxhash::HashMap as GxHashMap;
use nohash_hasher::IntMap;
use serde::{Deserializer, Serializer};
use sonic_rs::{to_string, Deserialize, Serialize};

use crate::regex_matcher::{RegexMatcher, RegexTable};
use crate::sim_matcher::{SimMatcher, SimTable};
use crate::simple_matcher::{SimpleMatchType, SimpleMatcher};

bitflags! {
    #[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
    pub struct StrConvType: u8 {
        const None = 0b00000001;
        const Fanjian = 0b00000010;
        const WordDelete = 0b00000100;
        const TextDelete = 0b00001000;
        const Delete = 0b00001100;
        const Normalize = 0b00010000;
        const DeleteNormalize = 0b00011100;
        const FanjianDeleteNormalize = 0b00011110;
        const PinYin = 0b00100000;
        const PinYinChar = 0b01000000;
    }
}

impl Serialize for StrConvType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for StrConvType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bits: u8 = u8::deserialize(deserializer)?;
        Ok(StrConvType::from_bits_retain(bits))
    }
}

pub trait TextMatcherTrait<'a, T> {
    fn is_match(&self, text: &str) -> bool;
    fn process(&'a self, text: &str) -> Vec<T>;
    fn batch_process(&'a self, text_array: &[&str]) -> Vec<Vec<T>> {
        text_array.iter().map(|&text| self.process(text)).collect()
    }
}

pub trait MatchResultTrait<'a> {
    fn word_id(&self) -> usize {
        0
    }
    fn table_id(&self) -> usize {
        0
    }
    fn word(&self) -> &str;
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "snake_case")]
pub enum MatchTableType {
    Simple,
    SimilarChar,
    Acrostic,
    SimilarTextLevenshtein,
    Regex,
}

#[derive(Serialize, Deserialize)]
pub struct MatchTable<'a> {
    pub table_id: u32,
    pub match_table_type: MatchTableType,
    pub simple_match_type: SimpleMatchType,
    #[serde(borrow)]
    pub word_list: Vec<&'a str>,
    pub exemption_simple_match_type: SimpleMatchType,
    #[serde(borrow)]
    pub exemption_word_list: Vec<&'a str>,
}

#[derive(Debug)]
struct WordTableConf {
    match_id: String,
    table_id: u32,
    is_exemption: bool,
}

#[derive(Serialize)]
pub struct MatchResult<'a> {
    table_id: u32,
    word: Cow<'a, str>,
}

impl MatchResultTrait<'_> for MatchResult<'_> {
    fn word_id(&self) -> usize {
        0
    }
    fn table_id(&self) -> usize {
        self.table_id as usize
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

struct ResultDict<'a> {
    result_list: Vec<MatchResult<'a>>,
    exemption_flag: bool,
}

pub type MatchTableMap<'a> = GxHashMap<&'a str, Vec<MatchTable<'a>>>;

pub struct Matcher {
    simple_word_table_conf_map: IntMap<u64, WordTableConf>,
    simple_word_table_conf_id_map: IntMap<u64, u64>,
    simple_matcher: Option<SimpleMatcher>,
    regex_matcher: Option<RegexMatcher>,
    sim_matcher: Option<SimMatcher>,
}

impl Matcher {
    pub fn new<'a>(match_table_map: &'a MatchTableMap) -> Matcher {
        let mut word_id: u64 = 0;
        let mut word_table_conf_id: u64 = 0;
        let mut simple_word_table_conf_map = IntMap::default();
        let mut simple_word_table_conf_id_map = IntMap::default();

        let mut simple_match_type_word_map: GxHashMap<SimpleMatchType, IntMap<u64, &'a str>> =
            GxHashMap::default();

        let mut regex_table_list: Vec<RegexTable> = Vec::new();
        let mut sim_table_list: Vec<SimTable> = Vec::new();

        for (&match_id, table_list) in match_table_map {
            for table in table_list {
                let table_id = table.table_id;
                let match_table_type = &table.match_table_type;
                let word_list = &table.word_list;
                let exemption_word_list = &table.exemption_word_list;

                if !word_list.is_empty() {
                    match match_table_type {
                        MatchTableType::Simple => {
                            simple_word_table_conf_map.insert(
                                word_table_conf_id,
                                WordTableConf {
                                    match_id: match_id.to_owned(),
                                    table_id,
                                    is_exemption: false,
                                },
                            );

                            let simple_word_map = simple_match_type_word_map
                                .entry(table.simple_match_type)
                                .or_default();

                            for word in word_list.iter() {
                                simple_word_table_conf_id_map.insert(word_id, word_table_conf_id);
                                simple_word_map.insert(word_id, word);
                                word_id += 1;
                            }

                            word_table_conf_id += 1
                        }
                        MatchTableType::SimilarTextLevenshtein => sim_table_list.push(SimTable {
                            table_id,
                            match_id,
                            word_list,
                        }),
                        _ => regex_table_list.push(RegexTable {
                            table_id,
                            match_id,
                            match_table_type,
                            word_list,
                        }),
                    }
                }

                if !exemption_word_list.is_empty() {
                    simple_word_table_conf_map.insert(
                        word_table_conf_id,
                        WordTableConf {
                            match_id: match_id.to_owned(),
                            table_id,
                            is_exemption: true,
                        },
                    );

                    let simple_word_map = simple_match_type_word_map
                        .entry(table.exemption_simple_match_type)
                        .or_default();

                    for exemption_word in exemption_word_list.iter() {
                        simple_word_table_conf_id_map.insert(word_id, word_table_conf_id);
                        simple_word_map.insert(word_id, exemption_word);
                        word_id += 1;
                    }

                    word_table_conf_id += 1
                }
            }
        }

        Matcher {
            simple_word_table_conf_map,
            simple_word_table_conf_id_map,
            simple_matcher: (!simple_match_type_word_map.is_empty())
                .then(|| SimpleMatcher::new(&simple_match_type_word_map)),
            regex_matcher: (!regex_table_list.is_empty())
                .then(|| RegexMatcher::new(&regex_table_list)),
            sim_matcher: (!sim_table_list.is_empty()).then(|| SimMatcher::new(&sim_table_list)),
        }
    }

    fn word_match_raw(&self, text: &str) -> GxHashMap<&str, Vec<MatchResult>> {
        if !text.is_empty() {
            let mut match_result_dict: GxHashMap<&str, ResultDict> = GxHashMap::default();

            if let Some(simple_matcher) = &self.simple_matcher {
                for simple_result in simple_matcher.process(text) {
                    let word_table_conf = unsafe {
                        self.simple_word_table_conf_map
                            .get(
                                self.simple_word_table_conf_id_map
                                    .get(&simple_result.word_id)
                                    .unwrap_unchecked(),
                            )
                            .unwrap_unchecked()
                    };

                    let result_dict = match_result_dict
                        .entry(&word_table_conf.match_id)
                        .or_insert(ResultDict {
                            result_list: Vec::new(),
                            exemption_flag: false,
                        });

                    if word_table_conf.is_exemption {
                        result_dict.exemption_flag = true;
                    }

                    result_dict.result_list.push(MatchResult {
                        table_id: word_table_conf.table_id,
                        word: simple_result.word,
                    });
                }
            }

            if let Some(regex_matcher) = &self.regex_matcher {
                for regex_result in regex_matcher.process(text) {
                    let result_dict =
                        match_result_dict
                            .entry(regex_result.match_id)
                            .or_insert(ResultDict {
                                result_list: Vec::new(),
                                exemption_flag: false,
                            });

                    result_dict.result_list.push(MatchResult {
                        table_id: regex_result.table_id,
                        word: regex_result.word,
                    });
                }
            }

            if let Some(sim_matcher) = &self.sim_matcher {
                for sim_result in sim_matcher.process(text) {
                    let result_dict =
                        match_result_dict
                            .entry(sim_result.match_id)
                            .or_insert(ResultDict {
                                result_list: Vec::new(),
                                exemption_flag: false,
                            });

                    result_dict.result_list.push(MatchResult {
                        table_id: sim_result.table_id,
                        word: sim_result.word,
                    });
                }
            }

            match_result_dict
                .into_iter()
                .filter_map(|(match_id, result_dict)| {
                    (!result_dict.exemption_flag).then_some((match_id, result_dict.result_list))
                })
                .collect()
        } else {
            GxHashMap::default()
        }
    }

    pub fn word_match(&self, text: &str) -> HashMap<&str, String> {
        self.word_match_raw(text)
            .into_iter()
            .map(|(match_id, result_list)| {
                (match_id, unsafe {
                    to_string(&result_list).unwrap_unchecked()
                })
            })
            .collect()
    }

    pub fn word_match_as_string(&self, text: &str) -> String {
        unsafe { to_string(&self.word_match(text)).unwrap_unchecked() }
    }
}

impl<'a> TextMatcherTrait<'a, MatchResult<'a>> for Matcher {
    fn is_match(&self, text: &str) -> bool {
        if let Some(simple_matcher) = &self.simple_matcher {
            if simple_matcher.is_match(text) {
                return true;
            }
        }

        if let Some(regex_matcher) = &self.regex_matcher {
            if regex_matcher.is_match(text) {
                return true;
            }
        }

        if let Some(sim_matcher) = &self.sim_matcher {
            if sim_matcher.is_match(text) {
                return true;
            }
        }

        false
    }

    fn process(&'a self, text: &str) -> Vec<MatchResult<'a>> {
        self.word_match_raw(text)
            .into_iter()
            .flat_map(|(_, result_list)| result_list)
            .collect()
    }
}
