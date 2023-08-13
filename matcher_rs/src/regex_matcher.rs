use std::borrow::Cow;

use fancy_regex::{escape, Regex};
use zerovec::VarZeroVec;

use super::{MatchTableType, TextMatcherTrait};

pub struct RegexTable<'a> {
    pub table_id: u32,
    pub match_id: &'a str,
    pub match_table_type: &'a MatchTableType,
    pub wordlist: &'a VarZeroVec<'a, str>,
}

enum RegexType {
    StandardRegex {
        regex: Regex,
    },
    ListRegex {
        regex_list: Vec<Regex>,
        wordlist: Vec<String>,
    },
}

struct RegexPatternTable {
    table_id: u32,
    match_id: String,
    table_match_type: RegexType,
}

#[derive(Debug)]
pub struct RegexResult<'a> {
    pub word: Cow<'a, str>,
    pub table_id: u32,
    pub match_id: &'a str,
}

pub struct RegexMatcher {
    regex_pattern_table_list: Vec<RegexPatternTable>,
}

impl RegexMatcher {
    pub fn new(regex_table_list: &Vec<RegexTable>) -> RegexMatcher {
        let mut regex_pattern_table_list = Vec::with_capacity(regex_table_list.len());

        for regex_table in regex_table_list {
            let size = regex_table.wordlist.len();

            match regex_table.match_table_type {
                MatchTableType::SimilarChar => {
                    let pattern = regex_table
                        .wordlist
                        .iter()
                        .map(|charstr| format!("({})", escape(charstr).replace(',', "|")))
                        .collect::<Vec<String>>()
                        .join(".?");

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id.to_owned(),
                        table_match_type: RegexType::StandardRegex {
                            regex: Regex::new(&pattern).unwrap(),
                        },
                    });
                }
                MatchTableType::Acrostic => {
                    let mut wordlist = Vec::with_capacity(size);
                    let mut regex_list = Vec::with_capacity(size);

                    for word in regex_table.wordlist.iter() {
                        let pattern = format!(
                            r"(?:^|[\s\pP]+?){}",
                            escape(word).replace(',', r".*?[\s\pP]+?")
                        );

                        wordlist.push(word.to_owned());
                        regex_list.push(Regex::new(&pattern).unwrap());
                    }

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id.to_owned(),
                        table_match_type: RegexType::ListRegex {
                            regex_list,
                            wordlist,
                        },
                    });
                }
                MatchTableType::Regex => {
                    let wordlist = regex_table
                        .wordlist
                        .iter()
                        .map(|word| word.to_owned())
                        .collect::<Vec<String>>();

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id.to_owned(),
                        table_match_type: RegexType::ListRegex {
                            regex_list: wordlist
                                .iter()
                                .filter_map(|word| Regex::new(&word).ok())
                                .collect(),
                            wordlist,
                        },
                    });
                }
                _ => unreachable!(),
            };
        }

        RegexMatcher {
            regex_pattern_table_list,
        }
    }
}

impl<'a> TextMatcherTrait<'a, RegexResult<'a>> for RegexMatcher {
    fn is_match(&self, text: &str) -> bool {
        for regex_table in &self.regex_pattern_table_list {
            match &regex_table.table_match_type {
                RegexType::StandardRegex { regex } => {
                    if regex.is_match(text).unwrap() {
                        return true;
                    }
                }
                RegexType::ListRegex { regex_list, .. } => {
                    if regex_list.iter().any(|regex| regex.is_match(text).unwrap()) {
                        return true;
                    }
                }
            }
        }

        false
    }

    fn process(&'a self, text: &str) -> Vec<RegexResult<'a>> {
        let mut result_list = Vec::new();

        for regex_table in &self.regex_pattern_table_list {
            match &regex_table.table_match_type {
                RegexType::StandardRegex { regex } => {
                    for caps in regex.captures_iter(text).map(|caps| caps.unwrap()) {
                        result_list.push(RegexResult {
                            word: Cow::Owned(
                                caps.iter()
                                    .skip(1)
                                    .filter_map(|m| m.map(|match_char| match_char.as_str()))
                                    .collect::<Vec<&str>>()
                                    .join(""),
                            ),
                            table_id: regex_table.table_id,
                            match_id: &regex_table.match_id,
                        });
                    }
                }
                RegexType::ListRegex {
                    regex_list,
                    wordlist,
                } => {
                    for (index, regex) in regex_list.iter().enumerate() {
                        if regex.is_match(text).unwrap() {
                            result_list.push(RegexResult {
                                word: Cow::Borrowed(&wordlist[index]),
                                table_id: regex_table.table_id,
                                match_id: &regex_table.match_id,
                            });
                        }
                    }
                }
            }
        }

        result_list
    }
}
