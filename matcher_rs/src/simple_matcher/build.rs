//! Construction of [`super::SimpleMatcher`] — rule parsing, emitted-pattern deduplication,
//! and matcher compilation.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use crate::process::process_matcher::reduce_text_process_emit;
use crate::process::{ProcessType, build_process_type_tree};

use super::engine::ScanPlan;
use super::rule::{
    BITMASK_CAPACITY, PROCESS_TYPE_TABLE_SIZE, PatternEntry, PatternKind, RuleCold, RuleHot,
    RuleSet,
};
use super::{ProcessPlan, SearchMode, SimpleMatcher};

pub(super) struct ParsedRules<'a> {
    pub(super) dedup_patterns: Vec<Cow<'a, str>>,
    pub(super) dedup_entries: Vec<Vec<PatternEntry>>,
    pub(super) rules: RuleSet,
}

impl SimpleMatcher {
    pub fn new<'a, I, S1, S2>(
        process_type_word_map: &'a HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str> + 'a,
    {
        let pt_index_table = Self::build_pt_index_table(process_type_word_map.keys().copied());

        let process_type_set: HashSet<ProcessType> =
            process_type_word_map.keys().copied().collect();
        let single_pt_index = if process_type_set.len() == 1 {
            process_type_set
                .iter()
                .next()
                .map(|pt| pt_index_table[pt.bits() as usize])
        } else {
            None
        };

        let parsed = Self::parse_rules(process_type_word_map, &pt_index_table);

        let mut process_type_tree = build_process_type_tree(&process_type_set);
        for node in &mut process_type_tree {
            node.recompute_mask_with_index(&pt_index_table);
        }

        let base_mode = if let Some(pt_index) = single_pt_index {
            SearchMode::SingleProcessType { pt_index }
        } else {
            SearchMode::General
        };
        let scan = ScanPlan::compile(&parsed.dedup_patterns, parsed.dedup_entries, base_mode);
        let mode = if process_type_tree[0].children.is_empty() && scan.patterns().all_simple() {
            SearchMode::AllSimple
        } else {
            base_mode
        };

        SimpleMatcher {
            process: ProcessPlan::new(process_type_tree, mode),
            scan,
            rules: parsed.rules,
        }
    }

    fn build_pt_index_table(
        process_type_keys: impl Iterator<Item = ProcessType>,
    ) -> [u8; PROCESS_TYPE_TABLE_SIZE] {
        let mut pt_index_table = [u8::MAX; PROCESS_TYPE_TABLE_SIZE];
        let mut next_pt_idx: u8 = 0;

        pt_index_table[ProcessType::None.bits() as usize] = next_pt_idx;
        next_pt_idx += 1;

        for pt in process_type_keys {
            let bits = pt.bits() as usize;
            if bits < PROCESS_TYPE_TABLE_SIZE && pt_index_table[bits] == u8::MAX {
                pt_index_table[bits] = next_pt_idx;
                next_pt_idx += 1;
            }
        }

        pt_index_table
    }

    fn parse_rules<'a, I, S1, S2>(
        process_type_word_map: &'a HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
        pt_index_table: &[u8; PROCESS_TYPE_TABLE_SIZE],
    ) -> ParsedRules<'a>
    where
        I: AsRef<str> + 'a,
    {
        let word_size: usize = process_type_word_map.values().map(|map| map.len()).sum();

        let mut dedup_entries: Vec<Vec<PatternEntry>> = Vec::with_capacity(word_size);
        let mut rule_hot: Vec<RuleHot> = Vec::with_capacity(word_size);
        let mut rule_cold: Vec<RuleCold> = Vec::with_capacity(word_size);
        let mut word_id_to_idx: HashMap<(ProcessType, u32), usize> =
            HashMap::with_capacity(word_size);

        let mut next_pattern_id: usize = 0;
        let mut dedup_patterns = Vec::with_capacity(word_size);
        let mut pattern_id_map: HashMap<Cow<'_, str>, usize> = HashMap::with_capacity(word_size);

        for (&process_type, simple_word_map) in process_type_word_map {
            let word_process_type = process_type - ProcessType::Delete;

            for (&simple_word_id, simple_word) in simple_word_map {
                if simple_word.as_ref().is_empty() {
                    continue;
                }

                let mut and_splits: HashMap<&str, i32> = HashMap::new();
                let mut not_splits: HashMap<&str, i32> = HashMap::new();

                let mut start = 0;
                let mut current_is_not = false;

                let mut add_sub_word = |word: &'a str, is_not: bool| {
                    if word.is_empty() {
                        return;
                    }
                    if is_not {
                        let entry = not_splits.entry(word).or_insert(1);
                        *entry -= 1;
                    } else {
                        let entry = and_splits.entry(word).or_insert(0);
                        *entry += 1;
                    }
                };

                for (index, marker) in simple_word.as_ref().match_indices(['&', '~']) {
                    add_sub_word(&simple_word.as_ref()[start..index], current_is_not);
                    current_is_not = marker == "~";
                    start = index + 1;
                }
                add_sub_word(&simple_word.as_ref()[start..], current_is_not);

                if and_splits.is_empty() && not_splits.is_empty() {
                    continue;
                }

                let and_count = and_splits.len();
                let segment_counts = and_splits
                    .values()
                    .copied()
                    .chain(not_splits.values().copied())
                    .collect::<Vec<i32>>();

                let use_matrix = and_count > BITMASK_CAPACITY
                    || segment_counts.len() > BITMASK_CAPACITY
                    || segment_counts[..and_count].iter().any(|&value| value != 1)
                    || segment_counts[and_count..].iter().any(|&value| value != 0);
                let has_not = and_count != segment_counts.len();

                let rule_idx = if let Some(&existing_idx) =
                    word_id_to_idx.get(&(process_type, simple_word_id))
                {
                    rule_hot[existing_idx] = RuleHot {
                        segment_counts,
                        and_count,
                        use_matrix,
                        has_not,
                    };
                    rule_cold[existing_idx] = RuleCold {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                    };
                    existing_idx
                } else {
                    let idx = rule_hot.len();
                    word_id_to_idx.insert((process_type, simple_word_id), idx);
                    rule_hot.push(RuleHot {
                        segment_counts,
                        and_count,
                        use_matrix,
                        has_not,
                    });
                    rule_cold.push(RuleCold {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                    });
                    idx
                };

                let is_simple = and_count == 1 && !has_not && !use_matrix;

                for (offset, &split_word) in and_splits.keys().chain(not_splits.keys()).enumerate()
                {
                    let kind = if is_simple {
                        PatternKind::Simple
                    } else if offset < and_count {
                        PatternKind::And
                    } else {
                        PatternKind::Not
                    };

                    for ac_word in reduce_text_process_emit(word_process_type, split_word) {
                        let pt_index = pt_index_table[process_type.bits() as usize];
                        let Some(&dedup_id) = pattern_id_map.get(ac_word.as_ref()) else {
                            pattern_id_map.insert(ac_word.clone(), next_pattern_id);
                            dedup_entries.push(vec![PatternEntry {
                                rule_idx: rule_idx as u32,
                                offset: offset as u16,
                                pt_index,
                                kind,
                            }]);
                            dedup_patterns.push(ac_word);
                            next_pattern_id += 1;
                            continue;
                        };
                        dedup_entries[dedup_id].push(PatternEntry {
                            rule_idx: rule_idx as u32,
                            offset: offset as u16,
                            pt_index,
                            kind,
                        });
                    }
                }
            }
        }

        ParsedRules {
            dedup_patterns,
            dedup_entries,
            rules: RuleSet::new(rule_hot, rule_cold),
        }
    }
}
