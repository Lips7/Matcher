//! Hot-path scan and rule evaluation for [`super::SimpleMatcher`].
//!
//! Implements the two-pass matching loop: Pass 1 scans each text variant and updates
//! per-rule state; Pass 2 iterates touched rules and collects the satisfied ones.

use std::borrow::Cow;

use tinyvec::TinyVec;

use crate::process::ProcessedTextMasks;

use super::types::{
    BITMASK_CAPACITY, BytewiseMatcher, PatternEntry, RuleHot, SIMPLE_MATCH_STATE, ScanContext,
    SimpleMatchState, WordState,
};
use super::{SimpleMatcher, SimpleResult};

impl SimpleMatcher {
    /// Runs both Pass 1 and Pass 2, appending all satisfied rules to `results`.
    pub(super) fn process_preprocessed_into<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        SIMPLE_MATCH_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.prepare(self.rule_hot.len());

            self.scan_all_variants(processed_text_process_type_masks, &mut state, false);

            let generation = state.generation;
            let num_variants = processed_text_process_type_masks.len();

            for &rule_idx in &state.touched_indices {
                if state.word_states[rule_idx].not_generation == generation {
                    continue;
                }
                if Self::is_rule_satisfied(
                    &self.rule_hot[rule_idx],
                    &state.word_states,
                    &state.matrix,
                    rule_idx,
                    num_variants,
                ) {
                    let cold = &self.rule_cold[rule_idx];
                    results.push(SimpleResult {
                        word_id: cold.word_id,
                        word: Cow::Borrowed(&cold.word),
                    });
                }
            }
        });
    }

    /// Pass 1: scans all text variants, updating [`SimpleMatchState`].
    ///
    /// For each text variant in `processed_text_process_type_masks`, the matcher scans the
    /// bytewise engine first and then, when needed, the charwise engine. Each hit is
    /// dispatched to [`Self::process_match`], which updates the affected rule's counters.
    /// If `exit_early` is `true`, scanning halts as soon as a rule becomes fully satisfied
    /// on a path where early exit is sound (used by `is_match`).
    ///
    /// Returns `true` only when `exit_early` is `true` and at least one rule fired early.
    pub(super) fn scan_all_variants<'a>(
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
            if self.scan_variant(tv.text.as_ref(), ctx, state) {
                return true;
            }
        }
        false
    }

    /// Scans one pre-processed text variant through the relevant matcher engines.
    ///
    /// This is the inner loop of Pass 1 for a single text variant produced by the
    /// transformation pipeline.
    ///
    /// # Arguments
    /// * `processed_text` — the transformed text variant to scan.
    /// * `ctx` — scan context bundling the variant index, process-type mask, variant count,
    ///   early-exit flag, and ASCII flag (see [`ScanContext`]).
    /// * `state` — mutable per-call match state (word states, counters, touched list).
    ///
    /// Returns `true` if a rule was fully satisfied and `ctx.exit_early` is set; `false` otherwise.
    #[inline(always)]
    pub(super) fn scan_variant(
        &self,
        processed_text: &str,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        // Bytewise engine handles all ASCII-only patterns. Scan every variant.
        if let Some(ref bytewise) = self.bytewise_matcher {
            match bytewise {
                #[cfg(feature = "dfa")]
                BytewiseMatcher::AcDfa { matcher, to_dedup } => {
                    for hit in matcher.find_overlapping_iter(processed_text) {
                        // AC DFA assigns sequential IDs; translate to global dedup index.
                        let dedup_idx = to_dedup[hit.pattern().as_usize()] as usize;
                        if self.process_match(dedup_idx, ctx, state) {
                            return true;
                        }
                    }
                }
                BytewiseMatcher::DaacBytewise(daac_matcher) => {
                    for hit in daac_matcher.find_overlapping_iter(processed_text) {
                        // DAAC value IS the global dedup index — no indirection needed.
                        let dedup_idx = hit.value() as usize;
                        if self.process_match(dedup_idx, ctx, state) {
                            return true;
                        }
                    }
                }
            }
        }

        // Charwise DAAC handles non-ASCII (CJK, etc.) patterns. Non-ASCII patterns can never
        // match pure-ASCII text, so skip the scan entirely when the text is all ASCII.
        // `is_ascii` is pre-computed by walk_process_tree to avoid a redundant byte scan here.
        if !ctx.is_ascii
            && let Some(ref ac_matcher) = self.charwise_matcher
        {
            for ac_dedup_result in ac_matcher.find_overlapping_iter(processed_text) {
                // DAAC value IS the global dedup index — no indirection needed.
                let dedup_idx = ac_dedup_result.value() as usize;
                if self.process_match(dedup_idx, ctx, state) {
                    return true;
                }
            }
        }

        false
    }

    /// Updates rule counters for a single automaton hit (called from Pass 1).
    ///
    /// Looks up all [`PatternEntry`] records for `pattern_idx`, skipping any rule whose
    /// process-type mask does not include the current text variant or that has already been
    /// disqualified in this generation.
    ///
    /// For an AND sub-pattern hit: decrements the counter and sets the bit in `satisfied_mask`
    /// when the counter reaches ≤0. For a NOT sub-pattern hit: sets `not_generation` to
    /// permanently disqualify the rule. Returns `true` if `exit_early` and a rule just became
    /// fully satisfied.
    ///
    /// Repeated sub-patterns such as `a&a&a` are represented as counters rather than booleans,
    /// so the rule is satisfied only after enough hits arrive. Rules that do not fit the
    /// simple bitmask path fall back to the per-rule matrix, which tracks counts per text
    /// variant.
    #[inline(always)]
    pub(super) fn process_match(
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
            } = entry;

            let rule_idx = rule_idx as usize;
            let offset = offset as usize;

            // Check that this text variant was produced by the pattern entry's process type.
            // Sequential pt_index encodes the same information as the former u64 mask field,
            // but using 1 byte instead of 8, halving PatternEntry size.
            if ctx.process_type_mask & (1u64 << pt_index) == 0
                || state.word_states[rule_idx].not_generation == generation
            {
                continue;
            }

            let rule = &self.rule_hot[rule_idx];

            if state.word_states[rule_idx].satisfied_generation == generation {
                if ctx.exit_early {
                    return true;
                }
                continue;
            }

            if state.word_states[rule_idx].matrix_generation != generation {
                state.word_states[rule_idx].matrix_generation = generation;
                state.touched_indices.push(rule_idx);
                state.word_states[rule_idx].satisfied_mask = 0;

                if rule.use_matrix {
                    Self::init_matrix(
                        &mut state.matrix[rule_idx],
                        &rule.segment_counts,
                        ctx.num_variants,
                    );
                }
            }

            let is_satisfied = if rule.use_matrix {
                let flat_matrix = &mut state.matrix[rule_idx];
                let counter = &mut flat_matrix[offset * ctx.num_variants + ctx.text_index];
                if offset < rule.and_count {
                    *counter -= 1; // AND segment: counts down toward satisfaction (≤0 = satisfied)
                } else {
                    *counter += 1; // NOT segment: counts up toward disqualification (>0 = fired)
                }

                if offset < rule.and_count {
                    if *counter <= 0 && offset < BITMASK_CAPACITY {
                        state.word_states[rule_idx].satisfied_mask |= 1u64 << offset;
                    }
                } else if *counter > 0 {
                    state.word_states[rule_idx].not_generation = generation;
                }

                Self::is_rule_satisfied(
                    rule,
                    &state.word_states,
                    &state.matrix,
                    rule_idx,
                    ctx.num_variants,
                )
            } else if offset < rule.and_count {
                if offset < BITMASK_CAPACITY {
                    state.word_states[rule_idx].satisfied_mask |= 1u64 << offset;
                }
                let expected_mask = rule.expected_mask;
                let satisfied = state.word_states[rule_idx].satisfied_mask == expected_mask;
                if satisfied && rule.and_count == rule.num_splits as usize {
                    state.word_states[rule_idx].satisfied_generation = generation;
                }
                satisfied
            } else {
                state.word_states[rule_idx].not_generation = generation;
                false
            };

            if ctx.exit_early
                && is_satisfied
                && rule.and_count == rule.num_splits as usize
                && state.word_states[rule_idx].not_generation != generation
            {
                return true;
            }
        }
        false
    }

    /// Returns `true` if `rule` is fully satisfied for this generation.
    ///
    /// Uses the bitmask fast-path when `expected_mask > 0` (rules with at most 64 distinct
    /// AND segments): every required bit must be set in `satisfied_mask`.
    ///
    /// Falls back to the counter matrix when `expected_mask == 0`, iterating all segments.
    /// For AND segments the counter must be ≤0 in at least one variant. For NOT segments
    /// the counter must stay ≤0 in every variant. Callers in Pass 2 typically pre-filter
    /// disqualified rules via `not_generation` before reaching this check.
    #[inline(always)]
    pub(super) fn is_rule_satisfied(
        rule: &RuleHot,
        word_states: &[WordState],
        matrix: &[TinyVec<[i32; 16]>],
        rule_idx: usize,
        num_variants: usize,
    ) -> bool {
        let expected_mask = rule.expected_mask;
        if expected_mask > 0 {
            return word_states[rule_idx].satisfied_mask == expected_mask;
        }
        let num_splits = rule.num_splits as usize;
        let flat_matrix = &matrix[rule_idx];
        (0..num_splits).all(|s| {
            flat_matrix[s * num_variants..(s + 1) * num_variants]
                .iter()
                .any(|&bit| bit <= 0)
        })
    }

    /// Initializes the flat counter matrix for a rule on its first touch in a generation.
    ///
    /// Marked `#[cold]` because the matrix path is uncommon. Extracting it helps the
    /// compiler keep the simple bitmask path compact.
    #[cold]
    #[inline(never)]
    pub(super) fn init_matrix(
        flat_matrix: &mut TinyVec<[i32; 16]>,
        segment_counts: &[i32],
        num_variants: usize,
    ) {
        let num_splits = segment_counts.len();
        flat_matrix.clear();
        flat_matrix.resize(num_splits * num_variants, 0i32);
        for (s, &bit) in segment_counts.iter().enumerate() {
            let row_start = s * num_variants;
            flat_matrix[row_start..row_start + num_variants].fill(bit);
        }
    }
}
