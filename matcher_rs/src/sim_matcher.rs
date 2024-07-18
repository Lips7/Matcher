use std::borrow::Cow;

use id_set::IdSet;
use nohash_hasher::IntSet;
use rapidfuzz::distance;
use sonic_rs::{Deserialize, Serialize};

use crate::{
    matcher::{MatchResultTrait, TextMatcherTrait},
    process::process_matcher::{
        build_process_type_tree, reduce_text_process_with_tree, ProcessType, ProcessTypeBitNode,
    },
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SimMatchType {
    Levenshtein,
}

#[derive(Debug, Clone)]
pub struct SimTable<'a> {
    pub table_id: u32,
    pub match_id: u32,
    pub process_type: ProcessType,
    pub sim_match_type: SimMatchType,
    pub word_list: &'a Vec<&'a str>,
    pub threshold: f64,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct SimProcessedTable {
    table_id: u32,
    match_id: u32,
    process_type: ProcessType,
    sim_match_type: SimMatchType,
    word_list: Vec<String>,
    threshold: f64,
}

#[derive(Debug, Clone)]
pub struct SimResult<'a> {
    pub match_id: u32,
    pub table_id: u32,
    pub word_id: u32,
    pub word: Cow<'a, str>,
    pub similarity: f64,
}

impl MatchResultTrait<'_> for SimResult<'_> {
    fn match_id(&self) -> u32 {
        self.match_id
    }
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn word_id(&self) -> u32 {
        0
    }
    fn word(&self) -> &str {
        &self.word
    }
    fn similarity(&self) -> f64 {
        self.similarity
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SimMatcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    sim_processed_table_list: Vec<SimProcessedTable>,
}

impl SimMatcher {
    pub fn new(sim_table_list: &[SimTable]) -> SimMatcher {
        let mut process_type_list = Vec::with_capacity(sim_table_list.len());
        let mut sim_processed_table_list = Vec::with_capacity(sim_table_list.len());

        for sim_table in sim_table_list {
            process_type_list.push(sim_table.process_type);
            sim_processed_table_list.push(SimProcessedTable {
                table_id: sim_table.table_id,
                match_id: sim_table.match_id,
                process_type: sim_table.process_type,
                sim_match_type: sim_table.sim_match_type,
                word_list: sim_table
                    .word_list
                    .iter()
                    .map(|&word| word.to_owned())
                    .collect::<Vec<String>>(),
                threshold: sim_table.threshold,
            })
        }

        let process_type_tree = build_process_type_tree(&process_type_list);

        SimMatcher {
            process_type_tree,
            sim_processed_table_list,
        }
    }
}

impl<'a> TextMatcherTrait<'a, SimResult<'a>> for SimMatcher {
    fn is_match(&'a self, text: &'a str) -> bool {
        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._is_match_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    fn _is_match_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, id_set::IdSet)],
    ) -> bool {
        for (processed_text, process_type_set) in processed_text_process_type_set {
            for sim_processed_table in &self.sim_processed_table_list {
                if !process_type_set.contains(sim_processed_table.process_type.bits() as usize) {
                    continue;
                }
                let is_match = match sim_processed_table.sim_match_type {
                    SimMatchType::Levenshtein => sim_processed_table.word_list.iter().any(|text| {
                        distance::levenshtein::normalized_similarity_with_args(
                            text.chars(),
                            processed_text.chars(),
                            &distance::levenshtein::Args::default()
                                .score_cutoff(sim_processed_table.threshold),
                        )
                        .is_some()
                    }),
                };

                if is_match {
                    return true;
                }
            }
        }

        false
    }

    fn process(&'a self, text: &'a str) -> Vec<SimResult<'a>> {
        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._process_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    fn _process_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> Vec<SimResult<'a>> {
        let mut result_list = Vec::new();
        let mut table_id_index_set = IntSet::default();

        for (processed_text, process_type_set) in processed_text_process_type_set {
            for sim_processed_table in &self.sim_processed_table_list {
                if !process_type_set.contains(sim_processed_table.process_type.bits() as usize) {
                    continue;
                }
                match sim_processed_table.sim_match_type {
                    SimMatchType::Levenshtein => {
                        for (index, text) in sim_processed_table.word_list.iter().enumerate() {
                            let table_id_index =
                                ((sim_processed_table.table_id as u64) << 32) | (index as u64);

                            if table_id_index_set.insert(table_id_index) {
                                if let Some(similarity) =
                                    distance::levenshtein::normalized_similarity_with_args(
                                        text.chars(),
                                        processed_text.chars(),
                                        &distance::levenshtein::Args::default()
                                            .score_cutoff(sim_processed_table.threshold),
                                    )
                                {
                                    result_list.push(SimResult {
                                        match_id: sim_processed_table.match_id,
                                        table_id: sim_processed_table.table_id,
                                        word_id: index as u32,
                                        word: Cow::Borrowed(text),
                                        similarity,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        result_list
    }
}
