use std::borrow::Cow;
use std::collections::HashMap;

use crate::process::ProcessType;

use super::state::{ScanContext, SimpleMatchState};
use super::{SearchMode, SimpleResult};

pub type SimpleTable<'a> = HashMap<ProcessType, HashMap<u32, &'a str>>;
pub type SimpleTableSerde<'a> = HashMap<ProcessType, HashMap<u32, Cow<'a, str>>>;

pub(super) const DIRECT_RULE_BIT: u32 = 1 << 31;
pub(super) const BITMASK_CAPACITY: usize = 64;
pub(super) const PROCESS_TYPE_TABLE_SIZE: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum PatternKind {
    Simple = 0,
    And = 1,
    Not = 2,
}

#[derive(Debug, Clone)]
pub(super) struct RuleHot {
    pub(super) segment_counts: Vec<i32>,
    pub(super) and_count: usize,
    pub(super) use_matrix: bool,
    pub(super) has_not: bool,
}

#[derive(Debug, Clone)]
pub(super) struct RuleCold {
    pub(super) word_id: u32,
    pub(super) word: String,
}

#[derive(Debug, Clone)]
pub(super) struct PatternEntry {
    pub(super) rule_idx: u32,
    pub(super) offset: u16,
    pub(super) pt_index: u8,
    pub(super) kind: PatternKind,
}

#[derive(Clone)]
pub(super) struct RuleSet {
    hot: Vec<RuleHot>,
    cold: Vec<RuleCold>,
}

#[derive(Clone)]
pub(super) struct PatternIndex {
    entries: Vec<PatternEntry>,
    ranges: Vec<(usize, usize)>,
}

pub(super) enum PatternDispatch<'a> {
    DirectRule(usize),
    SingleEntry(&'a PatternEntry),
    Entries(&'a [PatternEntry]),
}

impl RuleSet {
    pub(super) fn new(hot: Vec<RuleHot>, cold: Vec<RuleCold>) -> Self {
        Self { hot, cold }
    }

    #[inline(always)]
    pub(super) fn len(&self) -> usize {
        self.hot.len()
    }

    #[inline(always)]
    pub(super) fn has_match(&self, state: &SimpleMatchState) -> bool {
        state
            .touched_indices()
            .iter()
            .any(|&rule_idx| state.rule_is_satisfied(rule_idx))
    }

    #[inline(always)]
    pub(super) fn push_result_if_new<'a>(
        &'a self,
        rule_idx: usize,
        state: &mut SimpleMatchState,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        if state.mark_positive(rule_idx) {
            self.push_result(rule_idx, results);
        }
    }

    pub(super) fn collect_matches<'a>(
        &'a self,
        state: &SimpleMatchState,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        for &rule_idx in state.touched_indices() {
            if state.rule_is_satisfied(rule_idx) {
                self.push_result(rule_idx, results);
            }
        }
    }

    #[inline(always)]
    pub(super) fn process_entry<const SINGLE_PT: bool>(
        &self,
        entry: &PatternEntry,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        let generation = state.generation();
        let &PatternEntry {
            rule_idx,
            offset,
            pt_index,
            kind,
        } = entry;

        let rule_idx = rule_idx as usize;

        if !SINGLE_PT && ctx.process_type_mask & (1u64 << pt_index) == 0 {
            return false;
        }

        debug_assert!(rule_idx < state.word_states.len());
        debug_assert!(rule_idx < self.hot.len());

        match kind {
            PatternKind::Simple => {
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };
                if word_state.positive_generation == generation {
                    return ctx.exit_early;
                }
                if word_state.matrix_generation != generation {
                    word_state.matrix_generation = generation;
                    word_state.positive_generation = generation;
                    state.touched_indices.push(rule_idx);
                    return ctx.exit_early;
                }
            }
            PatternKind::And => {
                let offset = offset as usize;
                let rule = unsafe { self.hot.get_unchecked(rule_idx) };
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };

                if word_state.not_generation == generation {
                    return false;
                }
                if word_state.positive_generation == generation {
                    if !rule.has_not && ctx.exit_early {
                        return true;
                    }
                    return false;
                }

                if word_state.matrix_generation != generation {
                    state.init_rule(rule, rule_idx, ctx);
                }

                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };
                let is_satisfied = if rule.use_matrix {
                    let flat_matrix = unsafe { state.matrix.get_unchecked_mut(rule_idx) };
                    let flat_status = unsafe { state.matrix_status.get_unchecked_mut(rule_idx) };
                    let counter = &mut flat_matrix[offset * ctx.num_variants + ctx.text_index];
                    *counter -= 1;
                    if flat_status[offset] == 0 && *counter <= 0 {
                        flat_status[offset] = 1;
                        word_state.remaining_and -= 1;
                        if word_state.remaining_and == 0 {
                            word_state.positive_generation = generation;
                        }
                    }
                    word_state.positive_generation == generation
                } else if rule.and_count == 1 {
                    word_state.positive_generation = generation;
                    true
                } else {
                    let bit = 1u64 << offset;
                    if word_state.satisfied_mask & bit == 0 {
                        word_state.satisfied_mask |= bit;
                        word_state.remaining_and -= 1;
                        if word_state.remaining_and == 0 {
                            word_state.positive_generation = generation;
                        }
                    }
                    word_state.positive_generation == generation
                };

                if ctx.exit_early
                    && is_satisfied
                    && !rule.has_not
                    && word_state.not_generation != generation
                {
                    return true;
                }
            }
            PatternKind::Not => {
                let offset = offset as usize;
                let rule = unsafe { self.hot.get_unchecked(rule_idx) };
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };

                if word_state.not_generation == generation {
                    return false;
                }

                if word_state.matrix_generation != generation {
                    state.init_rule(rule, rule_idx, ctx);
                }

                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };
                if rule.use_matrix {
                    let flat_matrix = unsafe { state.matrix.get_unchecked_mut(rule_idx) };
                    let flat_status = unsafe { state.matrix_status.get_unchecked_mut(rule_idx) };
                    let counter = &mut flat_matrix[offset * ctx.num_variants + ctx.text_index];
                    *counter += 1;
                    if flat_status[offset] == 0 && *counter > 0 {
                        flat_status[offset] = 1;
                        word_state.not_generation = generation;
                    }
                } else {
                    word_state.not_generation = generation;
                }
            }
        }

        false
    }

    #[inline(always)]
    fn push_result<'a>(&'a self, rule_idx: usize, results: &mut Vec<SimpleResult<'a>>) {
        debug_assert!(rule_idx < self.cold.len());
        let cold = unsafe { self.cold.get_unchecked(rule_idx) };
        results.push(SimpleResult {
            word_id: cold.word_id,
            word: Cow::Borrowed(&cold.word),
        });
    }
}

impl PatternIndex {
    pub(super) fn new(dedup_entries: Vec<Vec<PatternEntry>>) -> Self {
        let mut entries = Vec::with_capacity(dedup_entries.iter().map(|bucket| bucket.len()).sum());
        let mut ranges = Vec::with_capacity(dedup_entries.len());

        for bucket in dedup_entries {
            let start = entries.len();
            let len = bucket.len();
            entries.extend(bucket);
            ranges.push((start, len));
        }

        Self { entries, ranges }
    }

    #[inline(always)]
    pub(super) fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    #[inline(always)]
    pub(super) fn all_simple(&self) -> bool {
        self.entries
            .iter()
            .all(|entry| entry.kind == PatternKind::Simple)
    }

    pub(super) fn build_value_map(&self, mode: SearchMode) -> Vec<u32> {
        let use_direct_rule = matches!(
            mode,
            SearchMode::AllSimple | SearchMode::SingleProcessType { .. }
        );
        let mut value_map = Vec::with_capacity(self.ranges.len());

        for (dedup_idx, &(start, len)) in self.ranges.iter().enumerate() {
            if use_direct_rule && len == 1 {
                let entry = unsafe { self.entries.get_unchecked(start) };
                if entry.kind == PatternKind::Simple {
                    value_map.push(entry.rule_idx | DIRECT_RULE_BIT);
                    continue;
                }
            }
            value_map.push(dedup_idx as u32);
        }

        value_map
    }

    #[inline(always)]
    pub(super) fn dispatch<const SINGLE_PT: bool>(&self, raw_value: u32) -> PatternDispatch<'_> {
        if SINGLE_PT && raw_value & DIRECT_RULE_BIT != 0 {
            return PatternDispatch::DirectRule((raw_value & !DIRECT_RULE_BIT) as usize);
        }

        let pattern_idx = raw_value as usize;
        debug_assert!(pattern_idx < self.ranges.len());
        let &(start, len) = unsafe { self.ranges.get_unchecked(pattern_idx) };
        debug_assert!(start + len <= self.entries.len());

        if len == 1 {
            PatternDispatch::SingleEntry(unsafe { self.entries.get_unchecked(start) })
        } else {
            PatternDispatch::Entries(unsafe { self.entries.get_unchecked(start..start + len) })
        }
    }
}
