use std::borrow::Cow;

use fancy_regex::Regex;
use rapidfuzz::distance::levenshtein;
use zerovec::VarZeroVec;

use super::TextMatcherTrait;

pub struct SimTable<'a> {
    pub table_id: u32,
    pub match_id: &'a str,
    pub wordlist: &'a VarZeroVec<'a, str>,
}

struct SimProcessedTable {
    table_id: u32,
    match_id: String,
    wordlist: Vec<String>,
}

#[derive(Debug)]
pub struct SimResult<'a> {
    pub word: Cow<'a, str>,
    pub table_id: u32,
    pub match_id: &'a str,
    pub similarity: f64,
}

pub struct SimMatcher {
    remove_special_pattern: Regex,
    sim_processed_table_list: Vec<SimProcessedTable>,
}

impl SimMatcher {
    pub fn new(sim_table_list: &Vec<SimTable>) -> SimMatcher {
        SimMatcher {
            remove_special_pattern: Regex::new(r"\W+").unwrap(),
            sim_processed_table_list: sim_table_list
                .iter()
                .map(|sim_table| SimProcessedTable {
                    table_id: sim_table.table_id,
                    match_id: sim_table.match_id.to_owned(),
                    wordlist: sim_table
                        .wordlist
                        .iter()
                        .map(|word| word.to_owned())
                        .collect::<Vec<String>>(),
                })
                .collect(),
        }
    }
}

impl<'a> TextMatcherTrait<'a, SimResult<'a>> for SimMatcher {
    fn is_match(&self, text: &str) -> bool {
        let processed_text = self.remove_special_pattern.replace_all(text, "");

        self.sim_processed_table_list.iter().any(|sim_table| {
            sim_table.wordlist.iter().any(|text| {
                levenshtein::normalized_similarity_with_args(
                    text.chars(),
                    processed_text.chars(),
                    &levenshtein::Args::default().score_cutoff(0.8),
                )
                .is_some()
            })
        })
    }

    fn process(&'a self, text: &str) -> Vec<SimResult<'a>> {
        let processed_text = self.remove_special_pattern.replace_all(text, "");

        let mut result_list = Vec::new();

        for sim_table in &self.sim_processed_table_list {
            result_list.extend(sim_table.wordlist.iter().filter_map(|text| {
                levenshtein::normalized_similarity_with_args(
                    text.chars(),
                    processed_text.chars(),
                    &levenshtein::Args::default().score_cutoff(0.8),
                )
                .map(|similarity| SimResult {
                    word: Cow::Borrowed(text),
                    table_id: sim_table.table_id,
                    match_id: &sim_table.match_id,
                    similarity,
                })
            }));
        }

        result_list
    }
}