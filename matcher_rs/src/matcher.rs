use std::borrow::Cow;
use std::collections::HashMap;

use id_set::IdSet;
use nohash_hasher::{IntMap, IntSet};
use sonic_rs::{to_string, Deserialize, Serialize};

use crate::process::process_matcher::{
    build_process_type_tree, reduce_text_process_with_tree, ProcessType, ProcessTypeBitNode,
};
use crate::regex_matcher::{RegexMatchType, RegexMatcher, RegexResult, RegexTable};
use crate::sim_matcher::{SimMatchType, SimMatcher, SimResult, SimTable};
use crate::simple_matcher::{SimpleMatcher, SimpleTable};

pub trait TextMatcherTrait<'a, T: MatchResultTrait<'a> + 'a> {
    fn is_match(&'a self, text: &'a str) -> bool;
    fn _is_match_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> bool;
    fn process(&'a self, text: &'a str) -> Vec<T>;
    fn _process_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> Vec<T>;
    fn process_iter(&'a self, text: &'a str) -> Box<dyn Iterator<Item = T> + 'a> {
        Box::new(self.process(text).into_iter())
    }
    fn batch_process(&'a self, text_array: &[&'a str]) -> Vec<Vec<T>> {
        text_array.iter().map(|&text| self.process(text)).collect()
    }
}

pub trait MatchResultTrait<'a> {
    fn match_id(&self) -> u32;
    fn table_id(&self) -> u32;
    fn word_id(&self) -> u32;
    fn word(&self) -> &str;
    fn similarity(&self) -> f64;
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MatchTableType {
    Simple {
        process_type: ProcessType,
    },
    Regex {
        regex_match_type: RegexMatchType,
        process_type: ProcessType,
    },
    Similar {
        sim_match_type: SimMatchType,
        threshold: f64,
        process_type: ProcessType,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MatchTable<'a> {
    pub table_id: u32,
    pub match_table_type: MatchTableType,
    #[serde(borrow)]
    pub word_list: Vec<&'a str>,
    pub exemption_process_type: ProcessType,
    #[serde(borrow)]
    pub exemption_word_list: Vec<&'a str>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct WordTableConf {
    match_id: u32,
    table_id: u32,
    offset: u32,
    is_exemption: bool,
}

#[derive(Serialize)]
pub struct MatchResult<'a> {
    pub match_id: u32,
    pub table_id: u32,
    pub word_id: u32,
    pub word: Cow<'a, str>,
    pub similarity: f64,
}

impl MatchResultTrait<'_> for MatchResult<'_> {
    fn match_id(&self) -> u32 {
        self.match_id
    }
    fn word_id(&self) -> u32 {
        self.word_id
    }
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
    fn similarity(&self) -> f64 {
        self.similarity
    }
}

impl<'a, 'b: 'a> From<SimResult<'b>> for MatchResult<'a> {
    fn from(sim_result: SimResult<'b>) -> Self {
        MatchResult {
            match_id: sim_result.match_id,
            table_id: sim_result.table_id,
            word_id: sim_result.word_id,
            word: sim_result.word,
            similarity: sim_result.similarity,
        }
    }
}

impl<'a, 'b: 'a> From<RegexResult<'b>> for MatchResult<'a> {
    fn from(regex_result: RegexResult<'b>) -> Self {
        MatchResult {
            match_id: regex_result.match_id,
            table_id: regex_result.table_id,
            word_id: regex_result.word_id,
            word: regex_result.word,
            similarity: 1.0,
        }
    }
}

pub type MatchTableMap<'a> = IntMap<u32, Vec<MatchTable<'a>>>;

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Matcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    simple_word_table_conf_list: Vec<WordTableConf>,
    simple_word_table_conf_index_list: Vec<usize>,
    simple_matcher: Option<SimpleMatcher>,
    regex_matcher: Option<RegexMatcher>,
    sim_matcher: Option<SimMatcher>,
}

impl Matcher {
    pub fn new<S>(match_table_map: &HashMap<u32, Vec<MatchTable<'_>>, S>) -> Matcher {
        let mut process_type_list = Vec::new();

        let mut simple_word_id = 0;
        let mut simple_word_table_conf_id = 0;
        let mut simple_word_table_conf_list = Vec::new();
        let mut simple_word_table_conf_index_list = Vec::new();
        let mut simple_table: SimpleTable = IntMap::default();

        let mut regex_table_list = Vec::new();
        let mut sim_table_list = Vec::new();

        for (&match_id, table_list) in match_table_map {
            for table in table_list {
                let table_id = table.table_id;
                let match_table_type = table.match_table_type;
                let word_list = &table.word_list;
                let exemption_word_list = &table.exemption_word_list;

                if !word_list.is_empty() {
                    match match_table_type {
                        MatchTableType::Simple { process_type } => {
                            process_type_list.push(process_type);
                            simple_word_table_conf_list.push(WordTableConf {
                                match_id,
                                table_id,
                                offset: simple_word_id,
                                is_exemption: false,
                            });

                            let simple_word_map = simple_table.entry(process_type).or_default();

                            for word in word_list.iter() {
                                simple_word_table_conf_index_list.push(simple_word_table_conf_id);
                                simple_word_map.insert(simple_word_id, word);
                                simple_word_id += 1;
                            }

                            simple_word_table_conf_id += 1
                        }
                        MatchTableType::Similar {
                            process_type,
                            sim_match_type,
                            threshold,
                        } => {
                            process_type_list.push(process_type);
                            sim_table_list.push(SimTable {
                                table_id,
                                match_id,
                                process_type,
                                sim_match_type,
                                word_list,
                                threshold,
                            })
                        }
                        MatchTableType::Regex {
                            process_type,
                            regex_match_type,
                        } => {
                            process_type_list.push(process_type);
                            regex_table_list.push(RegexTable {
                                table_id,
                                match_id,
                                process_type,
                                regex_match_type,
                                word_list,
                            })
                        }
                    }
                }

                if !exemption_word_list.is_empty() {
                    process_type_list.push(table.exemption_process_type);
                    simple_word_table_conf_list.push(WordTableConf {
                        match_id,
                        table_id,
                        offset: simple_word_id,
                        is_exemption: true,
                    });

                    let simple_word_map = simple_table
                        .entry(table.exemption_process_type)
                        .or_default();

                    for exemption_word in exemption_word_list.iter() {
                        simple_word_table_conf_index_list.push(simple_word_table_conf_id);
                        simple_word_map.insert(simple_word_id, exemption_word);
                        simple_word_id += 1;
                    }

                    simple_word_table_conf_id += 1
                }
            }
        }

        let process_type_tree = build_process_type_tree(&process_type_list);

        Matcher {
            process_type_tree,
            simple_word_table_conf_list,
            simple_word_table_conf_index_list,
            simple_matcher: (!simple_table.is_empty()).then(|| SimpleMatcher::new(&simple_table)),
            regex_matcher: (!regex_table_list.is_empty())
                .then(|| RegexMatcher::new(&regex_table_list)),
            sim_matcher: (!sim_table_list.is_empty()).then(|| SimMatcher::new(&sim_table_list)),
        }
    }

    pub fn word_match<'a>(&'a self, text: &'a str) -> HashMap<u32, Vec<MatchResult>> {
        if text.is_empty() {
            return HashMap::default();
        }

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._word_match_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    fn _word_match_with_processed_text_process_type_set<'a>(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> HashMap<u32, Vec<MatchResult>> {
        let mut match_result_dict = HashMap::new();
        let mut failed_match_table_id_set = IntSet::default();

        if let Some(regex_matcher) = &self.regex_matcher {
            for regex_result in regex_matcher
                ._process_with_processed_text_process_type_set(processed_text_process_type_set)
            {
                let result_list: &mut Vec<MatchResult> = match_result_dict
                    .entry(regex_result.match_id)
                    .or_insert(Vec::new());

                result_list.push(regex_result.into());
            }
        }

        if let Some(sim_matcher) = &self.sim_matcher {
            for sim_result in sim_matcher
                ._process_with_processed_text_process_type_set(processed_text_process_type_set)
            {
                let result_list = match_result_dict
                    .entry(sim_result.match_id)
                    .or_insert(Vec::new());

                result_list.push(sim_result.into());
            }
        }

        if let Some(simple_matcher) = &self.simple_matcher {
            for simple_result in simple_matcher
                ._process_with_processed_text_process_type_set(processed_text_process_type_set)
            {
                // Guaranteed not failed
                let word_table_conf = unsafe {
                    self.simple_word_table_conf_list.get_unchecked(
                        *self
                            .simple_word_table_conf_index_list
                            .get_unchecked(simple_result.word_id as usize),
                    )
                };
                let match_table_id =
                    ((word_table_conf.match_id as u64) << 32) | (word_table_conf.table_id as u64);

                if failed_match_table_id_set.contains(&match_table_id) {
                    continue;
                }

                let result_list = match_result_dict
                    .entry(word_table_conf.match_id)
                    .or_insert(Vec::new());
                if word_table_conf.is_exemption {
                    failed_match_table_id_set.insert(match_table_id);
                    result_list
                        .retain(|match_result| match_result.table_id != word_table_conf.table_id);
                } else {
                    result_list.push(MatchResult {
                        match_id: word_table_conf.match_id,
                        table_id: word_table_conf.table_id,
                        word_id: unsafe {
                            simple_result.word_id.unchecked_sub(word_table_conf.offset)
                        },
                        word: simple_result.word,
                        similarity: 1.0,
                    });
                }
            }
        }

        match_result_dict.retain(|_, match_result_list| !match_result_list.is_empty());
        match_result_dict
    }

    pub fn word_match_as_string(&self, text: &str) -> String {
        if text.is_empty() {
            return String::from("{}");
        }
        unsafe { to_string(&self.word_match(text)).unwrap_unchecked() }
    }
}

impl<'a> TextMatcherTrait<'a, MatchResult<'a>> for Matcher {
    fn is_match(&self, text: &str) -> bool {
        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._is_match_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    fn _is_match_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> bool {
        match &self.simple_matcher {
            Some(_) => !self
                ._word_match_with_processed_text_process_type_set(processed_text_process_type_set)
                .is_empty(),
            None => {
                if let Some(regex_matcher) = &self.regex_matcher {
                    if regex_matcher._is_match_with_processed_text_process_type_set(
                        processed_text_process_type_set,
                    ) {
                        return true;
                    }
                }
                if let Some(sim_matcher) = &self.sim_matcher {
                    if sim_matcher._is_match_with_processed_text_process_type_set(
                        processed_text_process_type_set,
                    ) {
                        return true;
                    }
                }
                false
            }
        }
    }

    fn process(&'a self, text: &'a str) -> Vec<MatchResult<'a>> {
        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._process_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    fn _process_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> Vec<MatchResult<'a>> {
        self._word_match_with_processed_text_process_type_set(processed_text_process_type_set)
            .into_iter()
            .flat_map(|(_, result_list)| result_list) // Flatten the result lists from all match IDs into a single iterator.
            .collect()
    }
}
