//! Hot-path scan and rule evaluation for [`super::SimpleMatcher`].
//!
//! Implements the two-pass matching loop: Pass 1 scans each text variant and updates
//! per-rule state; Pass 2 iterates touched rules and collects the satisfied ones.

use std::borrow::Cow;

use tinyvec::TinyVec;

use crate::process::ProcessedTextMasks;

use super::types::{
    AsciiMatcher, NonAsciiMatcher, PatternEntry, PatternKind, SIMPLE_MATCH_STATE, ScanContext,
    SimpleMatchState,
};
use super::{SimpleMatcher, SimpleResult};

impl SimpleMatcher {
    /// Runs both Pass 1 and Pass 2, appending all satisfied rules to `results`.
    pub(super) fn process_preprocessed_into<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        let mut state = SIMPLE_MATCH_STATE.borrow_mut();
        state.prepare(self.rule_hot.len());

        self.scan_all_variants(processed_text_process_type_masks, &mut state, false);

        let generation = state.generation;

        for &rule_idx in &state.touched_indices {
            let word_state = &state.word_states[rule_idx];
            if word_state.positive_generation == generation
                && word_state.not_generation != generation
            {
                let cold = &self.rule_cold[rule_idx];
                results.push(SimpleResult {
                    word_id: cold.word_id,
                    word: Cow::Borrowed(&cold.word),
                });
            }
        }
    }

    /// Pass 1: scans all text variants, updating [`SimpleMatchState`].
    ///
    /// Dispatches to the const-generic inner function based on whether all rules share
    /// a single process type. When `SINGLE_PT=true`, the per-entry process-type mask
    /// check is eliminated at compile time.
    ///
    /// Returns `true` only when `exit_early` is `true` and at least one rule fired early.
    pub(super) fn scan_all_variants<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        state: &mut SimpleMatchState,
        exit_early: bool,
    ) -> bool {
        if self.single_pt_index.is_some() {
            self.scan_all_variants_inner::<true>(
                processed_text_process_type_masks,
                state,
                exit_early,
            )
        } else {
            self.scan_all_variants_inner::<false>(
                processed_text_process_type_masks,
                state,
                exit_early,
            )
        }
    }

    #[inline(always)]
    fn scan_all_variants_inner<'a, const SINGLE_PT: bool>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        state: &mut SimpleMatchState,
        exit_early: bool,
    ) -> bool {
        if self.ac_dedup_ranges.is_empty() {
            return false;
        }

        let num_variants = processed_text_process_type_masks.len();

        for (index, tv) in processed_text_process_type_masks.iter().enumerate() {
            if tv.mask == 0 {
                continue;
            }
            let ctx = ScanContext {
                text_index: index,
                process_type_mask: tv.mask,
                num_variants,
                exit_early,
                is_ascii: tv.is_ascii,
            };
            if self.scan_variant::<SINGLE_PT>(tv.text.as_ref(), ctx, state) {
                return true;
            }
        }
        false
    }

    /// Scans one pre-processed text variant through the relevant matcher engines.
    ///
    /// Generic over `SINGLE_PT`: when `true`, the per-entry process-type mask check
    /// in `process_match` is dead code and the compiler eliminates it entirely.
    ///
    /// Returns `true` if a rule was fully satisfied and `ctx.exit_early` is set; `false` otherwise.
    #[inline(always)]
    pub(super) fn scan_variant<const SINGLE_PT: bool>(
        &self,
        processed_text: &str,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        if ctx.is_ascii {
            if let Some(ref ascii_matcher) = self.ascii_matcher {
                match ascii_matcher {
                    #[cfg(feature = "dfa")]
                    AsciiMatcher::AcDfa { matcher, to_dedup } => {
                        for hit in matcher.find_overlapping_iter(processed_text) {
                            let dedup_idx = to_dedup[hit.pattern().as_usize()] as usize;
                            if self.process_match::<SINGLE_PT>(dedup_idx, ctx, state) {
                                return true;
                            }
                        }
                    }
                    AsciiMatcher::DaacBytewise(daac_matcher) => {
                        for hit in daac_matcher.find_overlapping_iter(processed_text) {
                            let dedup_idx = hit.value() as usize;
                            if self.process_match::<SINGLE_PT>(dedup_idx, ctx, state) {
                                return true;
                            }
                        }
                    }
                }
            }
        } else if let Some(ref non_ascii_matcher) = self.non_ascii_matcher {
            match non_ascii_matcher {
                NonAsciiMatcher::DaacCharwise(daac_matcher) => {
                    for hit in daac_matcher.find_overlapping_iter(processed_text) {
                        let dedup_idx = hit.value() as usize;
                        if self.process_match::<SINGLE_PT>(dedup_idx, ctx, state) {
                            return true;
                        }
                    }
                }
            }
        } else if let Some(ref ascii_matcher) = self.ascii_matcher {
            match ascii_matcher {
                #[cfg(feature = "dfa")]
                AsciiMatcher::AcDfa { matcher, to_dedup } => {
                    for hit in matcher.find_overlapping_iter(processed_text) {
                        let dedup_idx = to_dedup[hit.pattern().as_usize()] as usize;
                        if self.process_match::<SINGLE_PT>(dedup_idx, ctx, state) {
                            return true;
                        }
                    }
                }
                AsciiMatcher::DaacBytewise(daac_matcher) => {
                    for hit in daac_matcher.find_overlapping_iter(processed_text) {
                        let dedup_idx = hit.value() as usize;
                        if self.process_match::<SINGLE_PT>(dedup_idx, ctx, state) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Updates rule counters for a single automaton hit (called from Pass 1).
    ///
    /// Generic over `SINGLE_PT`: when `true`, all rules share the same process type,
    /// so the per-entry `pt_index` mask check is compiled away.
    #[inline(always)]
    pub(super) fn process_match<const SINGLE_PT: bool>(
        &self,
        pattern_idx: usize,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        let generation = state.generation;
        let (start, len) = self.ac_dedup_ranges[pattern_idx];
        for entry in &self.ac_dedup_entries[start..start + len] {
            let &PatternEntry {
                rule_idx,
                offset,
                pt_index,
                kind,
            } = entry;

            let rule_idx = rule_idx as usize;

            if !SINGLE_PT && ctx.process_type_mask & (1u64 << pt_index) == 0 {
                continue;
            }

            match kind {
                PatternKind::Simple => {
                    let word_state = &mut state.word_states[rule_idx];
                    if word_state.positive_generation == generation {
                        if ctx.exit_early {
                            return true;
                        }
                        continue;
                    }
                    if word_state.matrix_generation != generation {
                        word_state.matrix_generation = generation;
                        word_state.positive_generation = generation;
                        state.touched_indices.push(rule_idx);
                        if ctx.exit_early {
                            return true;
                        }
                    }
                }
                PatternKind::And => {
                    let offset = offset as usize;
                    let rule = &self.rule_hot[rule_idx];
                    let word_state = &mut state.word_states[rule_idx];

                    if word_state.not_generation == generation {
                        continue;
                    }
                    if word_state.positive_generation == generation {
                        if !rule.has_not && ctx.exit_early {
                            return true;
                        }
                        continue;
                    }

                    if word_state.matrix_generation != generation {
                        word_state.matrix_generation = generation;
                        word_state.positive_generation =
                            if rule.and_count == 0 { generation } else { 0 };
                        word_state.remaining_and = rule.and_count as u16;
                        word_state.satisfied_mask = 0;
                        state.touched_indices.push(rule_idx);

                        if rule.use_matrix {
                            Self::init_matrix(
                                &mut state.matrix[rule_idx],
                                &mut state.matrix_status[rule_idx],
                                &rule.segment_counts,
                                ctx.num_variants,
                            );
                        }
                    }

                    let is_satisfied = if rule.use_matrix {
                        let flat_matrix = &mut state.matrix[rule_idx];
                        let flat_status = &mut state.matrix_status[rule_idx];
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
                    let rule = &self.rule_hot[rule_idx];
                    let word_state = &mut state.word_states[rule_idx];

                    if word_state.not_generation == generation {
                        continue;
                    }

                    if word_state.matrix_generation != generation {
                        word_state.matrix_generation = generation;
                        word_state.positive_generation =
                            if rule.and_count == 0 { generation } else { 0 };
                        word_state.remaining_and = rule.and_count as u16;
                        word_state.satisfied_mask = 0;
                        state.touched_indices.push(rule_idx);

                        if rule.use_matrix {
                            Self::init_matrix(
                                &mut state.matrix[rule_idx],
                                &mut state.matrix_status[rule_idx],
                                &rule.segment_counts,
                                ctx.num_variants,
                            );
                        }
                    }

                    if rule.use_matrix {
                        let flat_matrix = &mut state.matrix[rule_idx];
                        let flat_status = &mut state.matrix_status[rule_idx];
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
        }
        false
    }

    /// Initializes the flat counter matrix for a rule on its first touch in a generation.
    #[cold]
    #[inline(never)]
    pub(super) fn init_matrix(
        flat_matrix: &mut TinyVec<[i32; 16]>,
        flat_status: &mut TinyVec<[u8; 16]>,
        segment_counts: &[i32],
        num_variants: usize,
    ) {
        let num_splits = segment_counts.len();
        flat_matrix.clear();
        flat_matrix.resize(num_splits * num_variants, 0i32);
        flat_status.clear();
        flat_status.resize(num_splits, 0u8);
        for (s, &bit) in segment_counts.iter().enumerate() {
            let row_start = s * num_variants;
            flat_matrix[row_start..row_start + num_variants].fill(bit);
        }
    }
}
