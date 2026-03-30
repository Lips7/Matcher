//! Construction of [`super::SimpleMatcher`] — rule parsing, emitted-pattern deduplication,
//! and matcher compilation.
//!
//! The construction pipeline has three stages:
//!
//! 1. **Index table** ([`SimpleMatcher::build_pt_index_table`]) — assigns a compact
//!    sequential index (0..N) to each distinct [`ProcessType`] present in the input.
//!
//! 2. **Rule parsing** ([`SimpleMatcher::parse_rules`]) — splits each rule string on
//!    `&`/`~` operators, counts repeated sub-patterns, determines bitmask-vs-matrix mode,
//!    emits transformed sub-patterns via [`reduce_text_process_emit`], and deduplicates
//!    them into a global pattern table with attached [`PatternEntry`] metadata.
//!
//! 3. **Engine compilation** ([`ScanPlan::compile`]) — builds Aho-Corasick automata from
//!    the deduplicated patterns and wires up the value map that connects automaton hits
//!    back to rule entries.
//!
//! The final [`SimpleMatcher`] stores three immutable pieces: the [`ProcessPlan`] tree
//! for text transformation, the [`ScanPlan`] automata for pattern scanning, and the
//! [`RuleSet`] for result production.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use crate::process::{ProcessType, build_process_type_tree, reduce_text_process_emit};

use super::engine::ScanPlan;
use super::rule::{
    BITMASK_CAPACITY, PROCESS_TYPE_TABLE_SIZE, PatternEntry, PatternKind, RuleCold, RuleHot,
    RuleSet, RuleShape,
};
use super::{ProcessPlan, SearchMode, SimpleMatcher};

/// Fully parsed matcher construction output before scan-engine compilation.
///
/// This is the intermediate representation produced by [`SimpleMatcher::parse_rules`]
/// and consumed by [`ScanPlan::compile`]. It owns the deduplicated pattern strings,
/// their attached [`PatternEntry`] metadata, and the compiled [`RuleSet`].
pub(super) struct ParsedRules<'a> {
    /// Deduplicated emitted patterns in scan order.
    ///
    /// Index `i` corresponds to `dedup_entries[i]`. Patterns may be borrowed from the
    /// input table or owned when text transformation produced a new string.
    pub(super) dedup_patterns: Vec<Cow<'a, str>>,
    /// Rule entries attached to each deduplicated pattern.
    ///
    /// `dedup_entries[i]` lists every [`PatternEntry`] that should fire when
    /// `dedup_patterns[i]` is matched by the automaton.
    pub(super) dedup_entries: Vec<Vec<PatternEntry>>,
    /// Per-rule hot and cold metadata used by the scan and result phases.
    pub(super) rules: RuleSet,
}

/// Construction helpers for turning rule tables into an executable matcher.
impl SimpleMatcher {
    /// Compiles the provided process-type rule table into a [`SimpleMatcher`].
    ///
    /// Prefer [`crate::SimpleMatcherBuilder`] at call sites; this entry point exists mainly
    /// for direct table construction and serde-driven use cases.
    ///
    /// # Construction flow
    ///
    /// 1. Build the compact process-type index table.
    /// 2. Parse all rules into deduplicated patterns and rule metadata.
    /// 3. Build the process-type transformation tree and recompute masks using compact indices.
    /// 4. Choose the search mode — `AllSimple` when the tree has no children and every
    ///    pattern is simple; `SingleProcessType` when only one process type is used;
    ///    `General` otherwise.
    /// 5. Compile Aho-Corasick automata via the scan plan.
    /// 6. Assemble and return the immutable [`SimpleMatcher`].
    pub fn new<'a, I, S1, S2>(
        process_type_word_map: &'a HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
    ) -> Result<SimpleMatcher, crate::MatcherError>
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
        let scan = ScanPlan::compile(&parsed.dedup_patterns, parsed.dedup_entries, base_mode)?;
        let mode = if process_type_tree[0].children.is_empty() && scan.patterns().all_simple() {
            SearchMode::AllSimple
        } else {
            base_mode
        };

        Ok(SimpleMatcher {
            process: ProcessPlan::new(process_type_tree, mode),
            scan,
            rules: parsed.rules,
        })
    }

    /// Assigns each used composite process type a compact sequential index.
    ///
    /// The returned array is indexed by raw [`ProcessType::bits()`] and maps to a dense
    /// `u8` index starting at 0. [`ProcessType::None`] always gets index 0. Unused
    /// entries are set to `u8::MAX`.
    ///
    /// These compact indices are stored in [`PatternEntry::pt_index`] and used to build
    /// the `process_type_mask` bitmask in [`ScanContext`](super::state::ScanContext).
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

    /// Parses the raw rule table into deduplicated emitted patterns and rule metadata.
    ///
    /// This stage handles logical operators (`&`/`~`), repeated sub-pattern counts, and
    /// the delete-adjusted emit behavior described in the design docs.
    ///
    /// # Delete-adjusted indexing
    ///
    /// Each rule is indexed under `process_type - ProcessType::Delete` rather than the
    /// full `ProcessType`. Delete-normalized text is what the automaton scans, so patterns
    /// must NOT themselves be Delete-transformed before indexing — they are stored verbatim
    /// and matched against the already-deleted text variants. The sub-patterns are then
    /// emitted through [`reduce_text_process_emit`] which applies only the non-Delete
    /// portion of the transformation (Fanjian, Normalize, PinYin, etc.).
    ///
    /// # Deduplication
    ///
    /// Identical emitted pattern strings (after transformation) are assigned a single
    /// automaton slot. Multiple [`PatternEntry`] values referencing different rules are
    /// collected in the same bucket of `dedup_entries`.
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
                    });
                    rule_cold.push(RuleCold {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                    });
                    idx
                };

                let is_simple = and_count == 1 && !has_not && !use_matrix;
                let shape = match (use_matrix, and_count == 1, has_not) {
                    (true, _, true) => RuleShape::MatrixNot,
                    (true, _, false) => RuleShape::Matrix,
                    (false, true, true) => RuleShape::SingleAndNot,
                    (false, true, false) => RuleShape::SingleAnd,
                    (false, false, true) => RuleShape::BitmaskNot,
                    (false, false, false) => RuleShape::Bitmask,
                };

                for (offset, &split_word) in and_splits.keys().chain(not_splits.keys()).enumerate()
                {
                    debug_assert!(
                        offset < 256,
                        "rule has {offset} segments; PatternEntry::offset is u8 (max 255)"
                    );

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
                                offset: offset as u8,
                                pt_index,
                                kind,
                                shape,
                            }]);
                            dedup_patterns.push(ac_word);
                            next_pattern_id += 1;
                            continue;
                        };
                        dedup_entries[dedup_id].push(PatternEntry {
                            rule_idx: rule_idx as u32,
                            offset: offset as u8,
                            pt_index,
                            kind,
                            shape,
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
