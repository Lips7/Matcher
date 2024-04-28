use std::borrow::Cow;
use std::collections::HashMap;
use std::intrinsics::{likely, unlikely};
use std::rc::Rc;

use ahash::AHashMap;
use serde::{Deserialize, Serialize};
use serde_json::to_string;
use zerovec::VarZeroVec;

use crate::regex_matcher::{RegexMatcher, RegexTable};
use crate::sim_matcher::{SimMatcher, SimTable};
use crate::simple_matcher::{SimpleMatchType, SimpleMatcher, SimpleWord};

pub trait TextMatcherTrait<'a, T> {
    fn is_match(&self, text: &str) -> bool;
    fn process(&'a self, text: &str) -> Vec<T>;
    fn batch_process(&'a self, text_array: &[&str]) -> Vec<Vec<T>> {
        text_array.iter().map(|&text| self.process(text)).collect()
    }
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
    #[serde(borrow)]
    pub wordlist: VarZeroVec<'a, str>,
    #[serde(borrow)]
    pub exemption_wordlist: VarZeroVec<'a, str>,
    pub simple_match_type: SimpleMatchType,
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

struct ResultDict<'a> {
    result_list: Vec<MatchResult<'a>>,
    exemption_flag: bool,
}

pub type MatchTableDict<'a> = AHashMap<&'a str, Vec<MatchTable<'a>>>;

pub struct Matcher {
    word_table_list: Vec<Rc<WordTableConf>>,
    simple_matcher: Option<SimpleMatcher>,
    regex_matcher: Option<RegexMatcher>,
    sim_matcher: Option<SimMatcher>,
}

impl Matcher {
    pub fn new(match_table_dict: &MatchTableDict) -> Matcher {
        let mut word_id: u64 = 0;
        let mut word_table_list: Vec<Rc<WordTableConf>> = Vec::new();

        let mut simple_wordlist_dict: AHashMap<SimpleMatchType, Vec<SimpleWord>> = AHashMap::new();

        let mut regex_table_list: Vec<RegexTable> = Vec::new();
        let mut sim_table_list: Vec<SimTable> = Vec::new();

        for (&match_id, table_list) in match_table_dict {
            for table in table_list {
                let table_id = table.table_id;
                let match_table_type = &table.match_table_type;
                let wordlist = &table.wordlist;
                let exemption_wordlist = &table.exemption_wordlist;

                if !wordlist.is_empty() {
                    match match_table_type {
                        MatchTableType::Simple => {
                            let word_table_conf = Rc::new(WordTableConf {
                                match_id: match_id.to_owned(),
                                table_id,
                                is_exemption: false,
                            });
                            let simple_word_list = simple_wordlist_dict
                                .entry(table.simple_match_type)
                                .or_default();

                            for word in wordlist.iter() {
                                word_table_list.push(Rc::clone(&word_table_conf));
                                simple_word_list.push(SimpleWord { word_id, word });
                                word_id += 1;
                            }
                        }
                        MatchTableType::SimilarTextLevenshtein => sim_table_list.push(SimTable {
                            table_id,
                            match_id,
                            wordlist,
                        }),
                        _ => regex_table_list.push(RegexTable {
                            table_id,
                            match_id,
                            match_table_type,
                            wordlist,
                        }),
                    }
                }

                if !exemption_wordlist.is_empty() {
                    let word_table_conf = Rc::new(WordTableConf {
                        match_id: match_id.to_owned(),
                        table_id,
                        is_exemption: true,
                    });

                    let simple_word_list = simple_wordlist_dict
                        .entry(SimpleMatchType::FanjianDeleteNormalize)
                        .or_default();

                    for exemption_word in exemption_wordlist.iter() {
                        word_table_list.push(Rc::clone(&word_table_conf));
                        simple_word_list.push(SimpleWord {
                            word_id,
                            word: exemption_word,
                        });
                        word_id += 1;
                    }
                }
            }
        }

        Matcher {
            word_table_list,
            simple_matcher: (!simple_wordlist_dict.is_empty())
                .then(|| SimpleMatcher::new(&simple_wordlist_dict)),
            regex_matcher: (!regex_table_list.is_empty())
                .then(|| RegexMatcher::new(&regex_table_list)),
            sim_matcher: (!sim_table_list.is_empty()).then(|| SimMatcher::new(&sim_table_list)),
        }
    }

    fn word_match_raw(&self, text: &str) -> AHashMap<&str, Vec<MatchResult>> {
        if likely(!text.is_empty()) {
            let mut match_result_dict: AHashMap<&str, ResultDict> = AHashMap::new();

            if let Some(simple_matcher) = &self.simple_matcher {
                for simple_result in simple_matcher.process(text) {
                    let word_table_conf = unsafe {
                        self.word_table_list
                            .get_unchecked(simple_result.word_id as usize)
                    };

                    let result_dict = match_result_dict
                        .entry(&word_table_conf.match_id)
                        .or_insert(ResultDict {
                            result_list: Vec::new(),
                            exemption_flag: false,
                        });

                    if unlikely(word_table_conf.is_exemption) {
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
                    likely(!result_dict.exemption_flag)
                        .then_some((match_id, result_dict.result_list))
                })
                .collect()
        } else {
            AHashMap::new()
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
