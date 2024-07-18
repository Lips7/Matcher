use std::borrow::Cow;

use fancy_regex::{escape, Regex};
use id_set::IdSet;
use nohash_hasher::IntSet;
use regex::RegexSet;
use sonic_rs::{Deserialize, Serialize};

#[cfg(feature = "serde")]
use crate::util::serde::{serde_regex, serde_regex_list, serde_regex_set};
use crate::{
    matcher::{MatchResultTrait, TextMatcherTrait},
    process::process_matcher::{
        build_process_type_tree, reduce_text_process_with_tree, ProcessType, ProcessTypeBitNode,
    },
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegexMatchType {
    SimilarChar,
    Acrostic,
    Regex,
}

#[derive(Debug, Clone)]
pub struct RegexTable<'a> {
    pub table_id: u32,
    pub match_id: u32,
    pub process_type: ProcessType,
    pub regex_match_type: RegexMatchType,
    pub word_list: &'a Vec<&'a str>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
enum RegexType {
    Standard {
        #[cfg_attr(feature = "serde", serde(with = "serde_regex"))]
        regex: Regex,
    },
    List {
        #[cfg_attr(feature = "serde", serde(with = "serde_regex_list"))]
        regex_list: Vec<Regex>,
        word_list: Vec<String>,
    },
    Set {
        #[cfg_attr(feature = "serde", serde(with = "serde_regex_set"))]
        regex_set: RegexSet,
        word_list: Vec<String>,
    },
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct RegexPatternTable {
    table_id: u32,
    match_id: u32,
    process_type: ProcessType,
    regex_type: RegexType,
}

#[derive(Debug, Clone)]
pub struct RegexResult<'a> {
    pub match_id: u32,
    pub table_id: u32,
    pub word_id: u32,
    pub word: Cow<'a, str>,
}

impl MatchResultTrait<'_> for RegexResult<'_> {
    fn match_id(&self) -> u32 {
        self.match_id
    }
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn word_id(&self) -> u32 {
        self.word_id
    }
    fn word(&self) -> &str {
        &self.word
    }
    fn similarity(&self) -> f64 {
        1.0
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RegexMatcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    regex_pattern_table_list: Vec<RegexPatternTable>,
}

impl RegexMatcher {
    pub fn new(regex_table_list: &[RegexTable]) -> RegexMatcher {
        let mut process_type_list = Vec::with_capacity(regex_table_list.len());
        let mut regex_pattern_table_list = Vec::with_capacity(regex_table_list.len());

        for regex_table in regex_table_list {
            process_type_list.push(regex_table.process_type);

            let size = regex_table.word_list.len();

            match regex_table.regex_match_type {
                RegexMatchType::SimilarChar => {
                    let pattern = regex_table
                        .word_list
                        .iter()
                        .map(|charstr| format!("({})", escape(charstr).replace(',', "|")))
                        .collect::<Vec<String>>()
                        .join(".?");

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        process_type: regex_table.process_type,
                        regex_type: RegexType::Standard {
                            regex: Regex::new(&pattern).unwrap(),
                        },
                    });
                }
                RegexMatchType::Acrostic => {
                    let mut word_list = Vec::with_capacity(size);
                    let mut regex_list = Vec::with_capacity(size);
                    let mut pattern_list = Vec::with_capacity(size);

                    for &word in regex_table.word_list.iter() {
                        let pattern = format!(
                            r"(?i)(?:^|[\s\pP]+?){}",
                            escape(word).replace(',', r".*?[\s\pP]+?")
                        );
                        match Regex::new(&pattern) {
                            Ok(regex) => {
                                regex_list.push(regex);
                                word_list.push(word.to_owned());
                                pattern_list.push(pattern);
                            }
                            Err(e) => {
                                println!("Acrostic word {word} is illegal, ignored. Error: {e}");
                            }
                        }
                    }

                    let regex_type = RegexSet::new(pattern_list).map_or(
                        RegexType::List {
                            regex_list,
                            word_list: word_list.clone(),
                        },
                        |regex_set| RegexType::Set {
                            regex_set,
                            word_list,
                        },
                    );

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        process_type: regex_table.process_type,
                        regex_type,
                    });
                }
                RegexMatchType::Regex => {
                    let mut word_list = Vec::with_capacity(size);
                    let mut regex_list = Vec::with_capacity(size);

                    for &word in regex_table.word_list.iter() {
                        match Regex::new(word) {
                            Ok(regex) => {
                                regex_list.push(regex);
                                word_list.push(word.to_owned());
                            }
                            Err(e) => {
                                println!("Regex word {word} is illegal, ignored. Error: {e}");
                            }
                        }
                    }

                    let regex_type = RegexSet::new(&word_list).map_or(
                        RegexType::List {
                            regex_list,
                            word_list: word_list.clone(),
                        },
                        |regex_set| RegexType::Set {
                            regex_set,
                            word_list,
                        },
                    );

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        process_type: regex_table.process_type,
                        regex_type,
                    });
                }
            };
        }

        let process_type_tree = build_process_type_tree(&process_type_list);

        RegexMatcher {
            process_type_tree,
            regex_pattern_table_list,
        }
    }
}

impl<'a> TextMatcherTrait<'a, RegexResult<'a>> for RegexMatcher {
    fn is_match(&'a self, text: &'a str) -> bool {
        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._is_match_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    fn _is_match_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> bool {
        for (processed_text, process_type_set) in processed_text_process_type_set {
            for regex_pattern_table in &self.regex_pattern_table_list {
                if !process_type_set.contains(regex_pattern_table.process_type.bits() as usize) {
                    continue;
                }

                let is_match = match &regex_pattern_table.regex_type {
                    RegexType::Standard { regex } => regex.is_match(processed_text).unwrap(),
                    RegexType::List { regex_list, .. } => regex_list
                        .iter()
                        .any(|regex| regex.is_match(processed_text).unwrap()),
                    RegexType::Set { regex_set, .. } => regex_set.is_match(processed_text),
                };

                if is_match {
                    return true;
                }
            }
        }
        false
    }

    fn process(&'a self, text: &'a str) -> Vec<RegexResult<'a>> {
        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._process_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    fn _process_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> Vec<RegexResult<'a>> {
        let mut result_list = Vec::new();
        let mut table_id_index_set = IntSet::default();

        for (processed_text, process_type_set) in processed_text_process_type_set {
            for regex_pattern_table in &self.regex_pattern_table_list {
                if !process_type_set.contains(regex_pattern_table.process_type.bits() as usize) {
                    continue;
                }
                match &regex_pattern_table.regex_type {
                    RegexType::Standard { regex } => {
                        if table_id_index_set.insert(regex_pattern_table.table_id as u64) {
                            for caps in regex.captures_iter(processed_text).flatten() {
                                result_list.push(RegexResult {
                                    match_id: regex_pattern_table.match_id,
                                    table_id: regex_pattern_table.table_id,
                                    word_id: 0,
                                    word: Cow::Owned(
                                        caps.iter()
                                            .skip(1)
                                            .filter_map(|m| m.map(|match_char| match_char.as_str()))
                                            .collect::<String>(),
                                    ),
                                });
                            }
                        }
                    }
                    RegexType::List {
                        regex_list,
                        word_list,
                    } => {
                        for (index, regex) in regex_list.iter().enumerate() {
                            let table_id_index =
                                ((regex_pattern_table.table_id as u64) << 32) | (index as u64);

                            if table_id_index_set.insert(table_id_index) {
                                if let Ok(is_match) = regex.is_match(processed_text) {
                                    if is_match {
                                        result_list.push(RegexResult {
                                            match_id: regex_pattern_table.match_id,
                                            table_id: regex_pattern_table.table_id,
                                            word_id: index as u32,
                                            word: Cow::Borrowed(&word_list[index]),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    RegexType::Set {
                        regex_set,
                        word_list,
                    } => {
                        for index in regex_set.matches(processed_text) {
                            let table_id_index =
                                ((regex_pattern_table.table_id as u64) << 32) | (index as u64);

                            if table_id_index_set.insert(table_id_index) {
                                result_list.push(RegexResult {
                                    match_id: regex_pattern_table.match_id,
                                    table_id: regex_pattern_table.table_id,
                                    word_id: index as u32,
                                    word: Cow::Borrowed(&word_list[index]),
                                });
                            }
                        }
                    }
                }
            }
        }

        result_list
    }
}
