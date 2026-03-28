//! Hot-path scan and rule evaluation for [`super::SimpleMatcher`].
//!
//! Implements the two-pass matching loop: Pass 1 scans each text variant and updates
//! per-rule state; Pass 2 iterates touched rules and collects the satisfied ones.

use std::borrow::Cow;

use tinyvec::TinyVec;

use crate::process::ProcessedTextMasks;

use super::types::{
    AsciiMatcher, NonAsciiMatcher, PATTERN_SIMPLE_LITERAL, PatternEntry, SIMPLE_MATCH_STATE,
    ScanContext, SimpleMatchState,
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
    /// For each text variant in `processed_text_process_type_masks`, the matcher scans the
    /// ASCII engine first and then, when needed, the charwise engine. Each hit is
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
        if ctx.is_ascii {
            // ASCII text: bytewise matcher is optimal (1 transition per byte).
            if let Some(ref ascii_matcher) = self.ascii_matcher {
                match ascii_matcher {
                    #[cfg(feature = "dfa")]
                    AsciiMatcher::AcDfa { matcher, to_dedup } => {
                        for hit in matcher.find_overlapping_iter(processed_text) {
                            let dedup_idx = to_dedup[hit.pattern().as_usize()] as usize;
                            if self.process_match(dedup_idx, ctx, state) {
                                return true;
                            }
                        }
                    }
                    AsciiMatcher::DaacBytewise(daac_matcher) => {
                        for hit in daac_matcher.find_overlapping_iter(processed_text) {
                            let dedup_idx = hit.value() as usize;
                            if self.process_match(dedup_idx, ctx, state) {
                                return true;
                            }
                        }
                    }
                }
            }
        } else if let Some(ref non_ascii_matcher) = self.non_ascii_matcher {
            // Non-ASCII text: charwise DAAC does 1 transition per Unicode codepoint.
            // When both ASCII and non-ASCII patterns exist, this matcher contains
            // the full pattern set, so one scan covers everything.
            match non_ascii_matcher {
                NonAsciiMatcher::DaacCharwise(daac_matcher) => {
                    for hit in daac_matcher.find_overlapping_iter(processed_text) {
                        let dedup_idx = hit.value() as usize;
                        if self.process_match(dedup_idx, ctx, state) {
                            return true;
                        }
                    }
                }
            }
        } else if let Some(ref ascii_matcher) = self.ascii_matcher {
            // Fallback: no non-ASCII patterns exist, but ASCII patterns can still
            // appear in non-ASCII text. Bytewise scan works on UTF-8 directly.
            match ascii_matcher {
                #[cfg(feature = "dfa")]
                AsciiMatcher::AcDfa { matcher, to_dedup } => {
                    for hit in matcher.find_overlapping_iter(processed_text) {
                        let dedup_idx = to_dedup[hit.pattern().as_usize()] as usize;
                        if self.process_match(dedup_idx, ctx, state) {
                            return true;
                        }
                    }
                }
                AsciiMatcher::DaacBytewise(daac_matcher) => {
                    for hit in daac_matcher.find_overlapping_iter(processed_text) {
                        let dedup_idx = hit.value() as usize;
                        if self.process_match(dedup_idx, ctx, state) {
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
                flags,
            } = entry;

            let rule_idx = rule_idx as usize;

            if self.single_pt_index.is_none() && ctx.process_type_mask & (1u64 << pt_index) == 0 {
                continue;
            }

            // Fast path for simple literal rules (and_count==1, no NOT, no matrix).
            // Skips all counter/bitmask logic — just mark the rule as satisfied.
            if flags & PATTERN_SIMPLE_LITERAL != 0 {
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
                continue;
            }

            let offset = offset as usize;
            let rule = &self.rule_hot[rule_idx];
            let word_state = &mut state.word_states[rule_idx];
            if word_state.not_generation == generation {
                continue;
            }
            if !rule.has_not && word_state.positive_generation == generation {
                if ctx.exit_early {
                    return true;
                }
                continue;
            }
            if offset < rule.and_count && word_state.positive_generation == generation {
                continue;
            }

            if word_state.matrix_generation != generation {
                word_state.matrix_generation = generation;
                word_state.positive_generation = if rule.and_count == 0 { generation } else { 0 };
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
                if offset < rule.and_count {
                    *counter -= 1;
                    if flat_status[offset] == 0 && *counter <= 0 {
                        flat_status[offset] = 1;
                        word_state.remaining_and -= 1;
                        if word_state.remaining_and == 0 {
                            word_state.positive_generation = generation;
                        }
                    }
                } else {
                    *counter += 1;
                    if flat_status[offset] == 0 && *counter > 0 {
                        flat_status[offset] = 1;
                        word_state.not_generation = generation;
                    }
                }
                word_state.positive_generation == generation
            } else if offset < rule.and_count {
                if rule.and_count == 1 {
                    word_state.positive_generation = generation;
                } else {
                    let bit = 1u64 << offset;
                    if word_state.satisfied_mask & bit == 0 {
                        word_state.satisfied_mask |= bit;
                        word_state.remaining_and -= 1;
                        if word_state.remaining_and == 0 {
                            word_state.positive_generation = generation;
                        }
                    }
                }
                word_state.positive_generation == generation
            } else {
                word_state.not_generation = generation;
                false
            };

            if ctx.exit_early
                && is_satisfied
                && !rule.has_not
                && word_state.not_generation != generation
            {
                return true;
            }
        }
        false
    }

    /// Initializes the flat counter matrix for a rule on its first touch in a generation.
    ///
    /// Marked `#[cold]` because the matrix path is uncommon. Extracting it helps the
    /// compiler keep the simple bitmask path compact.
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
