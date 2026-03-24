//! Construction of [`super::SimpleMatcher`] — rule parsing, deduplication, and automaton compilation.
//!
//! [`SimpleMatcher::new`](super::SimpleMatcher::new) is the entry point; the helpers in this
//! module perform the four stages: parse → deduplicate → compile automata → flatten entries.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

#[cfg(feature = "dfa")]
use aho_corasick::{AhoCorasickBuilder, AhoCorasickKind};
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    DoubleArrayAhoCorasickBuilder, MatchKind as DoubleArrayAhoCorasickMatchKind,
};

use crate::process::process_matcher::reduce_text_process_emit;
use crate::process::{ProcessType, build_process_type_tree};

use super::SimpleMatcher;
use super::types::{
    AC_DFA_PATTERN_THRESHOLD, BITMASK_CAPACITY, BytewiseMatcher, PROCESS_TYPE_TABLE_SIZE,
    PatternEntry, RuleCold, RuleHot,
};

/// Intermediate outputs of [`SimpleMatcher::parse_rules`], bundling all data
/// that [`SimpleMatcher::new`] needs to proceed to automaton compilation.
pub(super) struct ParsedRules<'a> {
    pub(super) dedup_patterns: Vec<Cow<'a, str>>,
    pub(super) dedup_entries: Vec<Vec<PatternEntry>>,
    pub(super) rule_hot: Vec<RuleHot>,
    pub(super) rule_cold: Vec<RuleCold>,
}

impl SimpleMatcher {
    /// Compiles a new [`SimpleMatcher`] from a `{ProcessType → {word_id → pattern}}` map.
    ///
    /// Prefer [`crate::SimpleMatcherBuilder`] for a more ergonomic API.
    ///
    /// Construction is O(patterns × normalized_variants) and should happen once at startup.
    /// The steps are:
    /// 1. Parse `&`/`~` operators in each pattern into AND and NOT sub-patterns.
    /// 2. For each sub-pattern, generate all normalized text variants via
    ///    [`reduce_text_process_emit`].
    /// 3. Deduplicate all variants across all rules and process types into a single
    ///    pattern set.
    /// 4. Compile the pattern set into an Aho-Corasick automaton.
    /// 5. Build the transformation trie (`ProcessTypeBitNode` tree) for fast text
    ///    pre-processing at match time.
    ///
    /// One subtle detail: sub-patterns are indexed under `process_type - ProcessType::Delete`,
    /// not the full `process_type`. Each sub-pattern is transformed with all steps except
    /// Delete, so it lives in the same coordinate space as the delete-transformed input text.
    /// Applying Delete a second time to the sub-pattern would corrupt the match.
    ///
    /// # Arguments
    /// * `process_type_word_map` — input rule table; the value type `I` must implement
    ///   `AsRef<str>` so both `&str` and `Cow<str>` are accepted.
    ///
    /// # Panics
    /// Panics if the Aho-Corasick automaton fails to compile. This should
    /// only happen if the de-duplicated pattern set is internally inconsistent, which cannot
    /// occur with well-formed input.
    pub fn new<'a, I, S1, S2>(
        process_type_word_map: &'a HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str> + 'a,
    {
        let pt_index_table = Self::build_pt_index_table(process_type_word_map.keys().copied());

        let process_type_set: HashSet<ProcessType> =
            process_type_word_map.keys().copied().collect();

        let parsed = Self::parse_rules(process_type_word_map, &pt_index_table);

        let (bytewise_matcher, charwise_matcher) = Self::compile_automata(&parsed.dedup_patterns);

        let (ac_dedup_entries, ac_dedup_ranges) = Self::flatten_dedup_entries(parsed.dedup_entries);

        let mut process_type_tree = build_process_type_tree(&process_type_set);
        for node in &mut process_type_tree {
            node.recompute_mask_with_index(&pt_index_table);
        }

        SimpleMatcher {
            process_type_tree,
            bytewise_matcher,
            charwise_matcher,
            ac_dedup_entries,
            ac_dedup_ranges,
            rule_hot: parsed.rule_hot,
            rule_cold: parsed.rule_cold,
        }
    }

    /// Builds the sequential [`ProcessType`] index table.
    ///
    /// Maps `pt.bits()` → a compact sequential index (0, 1, 2, …) for every composite
    /// `ProcessType` used in `process_type_word_map`. [`ProcessType::None`] always gets
    /// index 0. Unused slots contain `u8::MAX`.
    fn build_pt_index_table(
        process_type_keys: impl Iterator<Item = ProcessType>,
    ) -> [u8; PROCESS_TYPE_TABLE_SIZE] {
        let mut pt_index_table = [u8::MAX; PROCESS_TYPE_TABLE_SIZE];
        let mut next_pt_idx: u8 = 0;
        // None first — it always occupies a slot (root node always emits it).
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

    /// Parses all rules, deduplicates sub-patterns, and builds `PatternEntry` records.
    ///
    /// For each word in `process_type_word_map`:
    /// - splits the pattern string on `&` and `~` operators into AND and NOT sub-patterns
    /// - generates all normalized text variants of each sub-pattern via [`reduce_text_process_emit`]
    /// - deduplicates variants across all rules into a flat pattern list
    /// - records a [`PatternEntry`] linking each variant back to its rule and sub-pattern offset
    fn parse_rules<'a, I, S1, S2>(
        process_type_word_map: &'a HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
        pt_index_table: &[u8; PROCESS_TYPE_TABLE_SIZE],
    ) -> ParsedRules<'a>
    where
        I: AsRef<str> + 'a,
    {
        let word_size: usize = process_type_word_map.values().map(|m| m.len()).sum();

        let mut dedup_entries: Vec<Vec<PatternEntry>> = Vec::with_capacity(word_size);
        let mut rule_hot: Vec<RuleHot> = Vec::with_capacity(word_size);
        let mut rule_cold: Vec<RuleCold> = Vec::with_capacity(word_size);
        let mut word_id_to_idx: HashMap<(ProcessType, u32), usize> =
            HashMap::with_capacity(word_size);

        let mut next_pattern_id: usize = 0;
        let mut dedup_patterns = Vec::with_capacity(word_size);
        let mut pattern_id_map: HashMap<Cow<str>, usize> = HashMap::with_capacity(word_size);

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

                for (index, char) in simple_word.as_ref().match_indices(['&', '~']) {
                    add_sub_word(&simple_word.as_ref()[start..index], current_is_not);
                    current_is_not = char == "~";
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

                let expected_mask = if and_count > 0 && and_count <= BITMASK_CAPACITY {
                    u64::MAX >> (BITMASK_CAPACITY - and_count)
                } else {
                    0
                };

                let num_splits = segment_counts.len() as u16;
                let use_matrix = and_count > BITMASK_CAPACITY
                    || segment_counts.len() > BITMASK_CAPACITY
                    || segment_counts[..and_count].iter().any(|&v| v != 1)
                    || segment_counts[and_count..].iter().any(|&v| v != 0);

                let rule_idx = if let Some(&existing_idx) =
                    word_id_to_idx.get(&(process_type, simple_word_id))
                {
                    rule_hot[existing_idx] = RuleHot {
                        segment_counts,
                        and_count,
                        expected_mask,
                        use_matrix,
                        num_splits,
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
                        expected_mask,
                        use_matrix,
                        num_splits,
                    });
                    rule_cold.push(RuleCold {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                    });
                    idx
                };

                for (offset, &split_word) in and_splits.keys().chain(not_splits.keys()).enumerate()
                {
                    for ac_word in reduce_text_process_emit(word_process_type, split_word) {
                        let pt_index = pt_index_table[process_type.bits() as usize];
                        let Some(&existing_dedup_id) = pattern_id_map.get(ac_word.as_ref()) else {
                            pattern_id_map.insert(ac_word.clone(), next_pattern_id);
                            dedup_entries.push(vec![PatternEntry {
                                rule_idx: rule_idx as u32,
                                offset: offset as u16,
                                pt_index,
                            }]);
                            dedup_patterns.push(ac_word);
                            next_pattern_id += 1;
                            continue;
                        };
                        dedup_entries[existing_dedup_id].push(PatternEntry {
                            rule_idx: rule_idx as u32,
                            offset: offset as u16,
                            pt_index,
                        });
                    }
                }
            }
        }

        ParsedRules {
            dedup_patterns,
            dedup_entries,
            rule_hot,
            rule_cold,
        }
    }

    /// Partitions deduplicated patterns by character content and compiles automata.
    ///
    /// ASCII-only patterns go to the bytewise engine (fast for English text);
    /// patterns with any non-ASCII byte go to the charwise DAAC (fast for CJK text).
    fn compile_automata(
        dedup_patterns: &[Cow<'_, str>],
    ) -> (
        Option<BytewiseMatcher>,
        Option<CharwiseDoubleArrayAhoCorasick<u32>>,
    ) {
        let mut bytewise_patvals: Vec<(&str, u32)> = Vec::new();
        let mut charwise_patvals: Vec<(&str, u32)> = Vec::new();
        #[cfg(feature = "dfa")]
        let mut bytewise_ac_to_dedup: Vec<u32> = Vec::new();

        for (dedup_id, pattern) in dedup_patterns.iter().enumerate() {
            if pattern.as_ref().is_ascii() {
                #[cfg(feature = "dfa")]
                bytewise_ac_to_dedup.push(dedup_id as u32);
                bytewise_patvals.push((pattern.as_ref(), dedup_id as u32));
            } else {
                charwise_patvals.push((pattern.as_ref(), dedup_id as u32));
            }
        }

        let bytewise_matcher = if !bytewise_patvals.is_empty() {
            #[cfg(feature = "dfa")]
            let engine = if bytewise_patvals.len() <= AC_DFA_PATTERN_THRESHOLD {
                BytewiseMatcher::AcDfa {
                    matcher: AhoCorasickBuilder::new()
                        .kind(Some(AhoCorasickKind::DFA))
                        .build(bytewise_patvals.iter().map(|(p, _)| p))
                        .unwrap(),
                    to_dedup: bytewise_ac_to_dedup,
                }
            } else {
                BytewiseMatcher::DaacBytewise(
                    DoubleArrayAhoCorasickBuilder::new()
                        .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                        .build_with_values(bytewise_patvals)
                        .unwrap(),
                )
            };
            #[cfg(not(feature = "dfa"))]
            let engine = BytewiseMatcher::DaacBytewise(
                DoubleArrayAhoCorasickBuilder::new()
                    .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                    .build_with_values(bytewise_patvals)
                    .unwrap(),
            );
            Some(engine)
        } else {
            None
        };

        let charwise_matcher = if !charwise_patvals.is_empty() {
            Some(
                CharwiseDoubleArrayAhoCorasickBuilder::new()
                    .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                    .build_with_values(charwise_patvals)
                    .unwrap(),
            )
        } else {
            None
        };

        (bytewise_matcher, charwise_matcher)
    }

    /// Flattens `Vec<Vec<PatternEntry>>` into a single SOA layout for cache-friendly scan.
    ///
    /// Returns `(ac_dedup_entries, ac_dedup_ranges)` where `ac_dedup_ranges[i] = (start, len)`
    /// maps automaton pattern index `i` to its slice of [`PatternEntry`] records.
    fn flatten_dedup_entries(
        dedup_entries: Vec<Vec<PatternEntry>>,
    ) -> (Vec<PatternEntry>, Vec<(usize, usize)>) {
        let mut ac_dedup_entries = Vec::with_capacity(dedup_entries.iter().map(|v| v.len()).sum());
        let mut ac_dedup_ranges = Vec::with_capacity(dedup_entries.len());
        for entries in dedup_entries {
            let start = ac_dedup_entries.len();
            let len = entries.len();
            ac_dedup_entries.extend(entries);
            ac_dedup_ranges.push((start, len));
        }
        (ac_dedup_entries, ac_dedup_ranges)
    }
}
