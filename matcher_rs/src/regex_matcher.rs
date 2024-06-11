use std::borrow::Cow;

use fancy_regex::{escape, Regex};

use super::{MatchResultTrait, MatchTableType, TextMatcherTrait};

pub struct RegexTable<'a> {
    pub table_id: u32,
    pub match_id: &'a str,
    pub match_table_type: &'a MatchTableType,
    pub word_list: &'a Vec<&'a str>,
}

enum RegexType {
    StandardRegex {
        regex: Regex,
    },
    ListRegex {
        regex_list: Vec<Regex>,
        word_list: Vec<String>,
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

impl MatchResultTrait<'_> for RegexResult<'_> {
    fn table_id(&self) -> usize {
        self.table_id as usize
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

pub struct RegexMatcher {
    regex_pattern_table_list: Vec<RegexPatternTable>,
}

impl RegexMatcher {
    pub fn new(regex_table_list: &Vec<RegexTable>) -> RegexMatcher {
        let mut regex_pattern_table_list = Vec::with_capacity(regex_table_list.len());

        for regex_table in regex_table_list {
            let size = regex_table.word_list.len();

            match regex_table.match_table_type {
                MatchTableType::SimilarChar => {
                    let pattern = regex_table
                        .word_list
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
                    let mut word_list = Vec::with_capacity(size);
                    let mut regex_list = Vec::with_capacity(size);

                    for &word in regex_table.word_list.iter() {
                        let pattern = format!(
                            r"(?:^|[\s\pP]+?){}",
                            escape(word).replace(',', r".*?[\s\pP]+?")
                        );

                        word_list.push(word.to_owned());
                        regex_list.push(Regex::new(&pattern).unwrap());
                    }

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id.to_owned(),
                        table_match_type: RegexType::ListRegex {
                            regex_list,
                            word_list,
                        },
                    });
                }
                MatchTableType::Regex => {
                    let word_list = regex_table
                        .word_list
                        .iter()
                        .map(|&word| word.to_owned())
                        .collect::<Vec<String>>();

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id.to_owned(),
                        table_match_type: RegexType::ListRegex {
                            regex_list: word_list
                                .iter()
                                .filter_map(|word| Regex::new(&word).ok())
                                .collect(),
                            word_list,
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
                    word_list,
                } => {
                    for (index, regex) in regex_list.iter().enumerate() {
                        if regex.is_match(text).unwrap() {
                            result_list.push(RegexResult {
                                word: Cow::Borrowed(&word_list[index]),
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
