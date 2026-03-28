//! Hot-path scan and rule evaluation for [`super::SimpleMatcher`].
//!
//! Implements the two-pass matching loop: Pass 1 scans each text variant and updates
//! per-rule state; Pass 2 iterates touched rules and collects the satisfied ones.

use std::borrow::Cow;

use tinyvec::TinyVec;

use crate::process::{ProcessedTextMasks, return_processed_string_to_pool, walk_process_tree};

use super::types::{
    AsciiMatcher, DIRECT_RULE_BIT, NonAsciiMatcher, PatternEntry, PatternKind, SIMPLE_MATCH_STATE,
    ScanContext, SimpleMatchState,
};
use super::{SimpleMatcher, SimpleResult};

impl SimpleMatcher {
    /// Fast path for `is_match` when all patterns are simple literals under a single
    /// process type with no tree walk needed. Avoids TLS state, generation counters,
    /// and overlapping iteration entirely.
    pub(super) fn is_match_simple(&self, text: &str) -> bool {
        // When all patterns are ASCII, the bytewise engine handles any text without
        // an O(N) is_ascii scan.
        if self.non_ascii_matcher.is_none() {
            return if let Some(ref m) = self.ascii_matcher {
                match m {
                    #[cfg(feature = "dfa")]
                    AsciiMatcher::AcDfa { matcher, .. } => matcher.is_match(text),
                    AsciiMatcher::DaacBytewise(d) => d.find_iter(text).next().is_some(),
                }
            } else {
                false
            };
        }
        if text.is_ascii() {
            if let Some(ref m) = self.ascii_matcher {
                return match m {
                    #[cfg(feature = "dfa")]
                    AsciiMatcher::AcDfa { matcher, .. } => matcher.is_match(text),
                    AsciiMatcher::DaacBytewise(d) => d.find_iter(text).next().is_some(),
                };
            }
        } else if let Some(ref m) = self.non_ascii_matcher {
            return match m {
                NonAsciiMatcher::DaacCharwise(d) => d.find_iter(text).next().is_some(),
            };
        }
        false
    }

    #[inline(always)]
    pub(super) fn is_match_inner<const SINGLE_PT: bool>(&self, text: &str) -> bool {
        let tree = &self.process_type_tree;
        let max_pt = tree.len();
        // SAFETY: #[thread_local] guarantees single-threaded access.
        // is_match_inner is never called re-entrantly.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rule_hot.len());
        let (text_masks, stopped) =
            walk_process_tree::<true, _>(tree, text, &mut |txt, idx, mask, is_ascii| {
                let ctx = ScanContext {
                    text_index: idx,
                    process_type_mask: mask,
                    num_variants: max_pt,
                    exit_early: true,
                    is_ascii,
                };
                self.scan_variant::<SINGLE_PT>(txt, ctx, state)
            });
        if stopped {
            return_processed_string_to_pool(text_masks);
            return true;
        }
        let generation = state.generation;
        let result = state.touched_indices.iter().any(|&rule_idx| {
            // SAFETY: rule_idx was pushed from a valid PatternEntry.rule_idx < rule count.
            debug_assert!(rule_idx < state.word_states.len());
            let word_state = unsafe { state.word_states.get_unchecked(rule_idx) };
            word_state.positive_generation == generation && word_state.not_generation != generation
        });
        return_processed_string_to_pool(text_masks);
        result
    }

    /// Fast path for `process`/`process_into` when all patterns are simple literals
    /// under a single process type with no tree walk needed.
    ///
    /// Skips `walk_process_tree` and `TRANSFORM_STATE` entirely. Uses generation-based
    /// deduplication from `SIMPLE_MATCH_STATE` to avoid emitting duplicate results when
    /// the same pattern appears multiple times in the text.
    pub(super) fn process_simple<'a>(&'a self, text: &'a str, results: &mut Vec<SimpleResult<'a>>) {
        // SAFETY: #[thread_local] guarantees single-threaded access.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rule_hot.len());
        let generation = state.generation;

        // Shared emit logic for each automaton hit.
        let mut emit = |raw_value: u32| {
            if raw_value & DIRECT_RULE_BIT != 0 {
                let rule_idx = (raw_value & !DIRECT_RULE_BIT) as usize;
                debug_assert!(rule_idx < state.word_states.len());
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };
                if word_state.positive_generation != generation {
                    word_state.positive_generation = generation;
                    debug_assert!(rule_idx < self.rule_cold.len());
                    let cold = unsafe { self.rule_cold.get_unchecked(rule_idx) };
                    results.push(SimpleResult {
                        word_id: cold.word_id,
                        word: Cow::Borrowed(&cold.word),
                    });
                }
                return;
            }
            let dedup_idx = raw_value as usize;
            debug_assert!(dedup_idx < self.ac_dedup_ranges.len());
            let &(start, len) = unsafe { self.ac_dedup_ranges.get_unchecked(dedup_idx) };
            debug_assert!(start + len <= self.ac_dedup_entries.len());
            let entries = unsafe { self.ac_dedup_entries.get_unchecked(start..start + len) };
            for entry in entries {
                let rule_idx = entry.rule_idx as usize;
                debug_assert!(rule_idx < state.word_states.len());
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };
                if word_state.positive_generation != generation {
                    word_state.positive_generation = generation;
                    debug_assert!(rule_idx < self.rule_cold.len());
                    let cold = unsafe { self.rule_cold.get_unchecked(rule_idx) };
                    results.push(SimpleResult {
                        word_id: cold.word_id,
                        word: Cow::Borrowed(&cold.word),
                    });
                }
            }
        };

        // When all patterns are ASCII, the bytewise engine handles any text without
        // an O(N) is_ascii scan.
        if self.non_ascii_matcher.is_none() || text.is_ascii() {
            if let Some(ref m) = self.ascii_matcher {
                match m {
                    #[cfg(feature = "dfa")]
                    AsciiMatcher::AcDfa { matcher, to_value } => {
                        for hit in matcher.find_overlapping_iter(text) {
                            let raw_value =
                                unsafe { *to_value.get_unchecked(hit.pattern().as_usize()) };
                            emit(raw_value);
                        }
                    }
                    AsciiMatcher::DaacBytewise(d) => {
                        for hit in d.find_overlapping_iter(text) {
                            emit(hit.value());
                        }
                    }
                }
            }
        } else if let Some(ref m) = self.non_ascii_matcher {
            match m {
                NonAsciiMatcher::DaacCharwise(d) => {
                    for hit in d.find_overlapping_iter(text) {
                        emit(hit.value());
                    }
                }
            }
        }
    }

    /// Runs both Pass 1 and Pass 2, appending all satisfied rules to `results`.
    pub(super) fn process_preprocessed_into<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        // SAFETY: #[thread_local] guarantees single-threaded access.
        // process_preprocessed_into is never called re-entrantly.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rule_hot.len());

        self.scan_all_variants(processed_text_process_type_masks, state, false);

        let generation = state.generation;

        for &rule_idx in &state.touched_indices {
            // SAFETY: rule_idx was pushed from a valid PatternEntry.rule_idx < rule count.
            debug_assert!(rule_idx < state.word_states.len());
            let word_state = unsafe { state.word_states.get_unchecked(rule_idx) };
            if word_state.positive_generation == generation
                && word_state.not_generation != generation
            {
                debug_assert!(rule_idx < self.rule_cold.len());
                let cold = unsafe { self.rule_cold.get_unchecked(rule_idx) };
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
        // When all patterns are ASCII, the bytewise engine handles any text, so
        // skip the is_ascii branch and avoid the third fallback arm entirely.
        let use_ascii = self.non_ascii_matcher.is_none() || ctx.is_ascii;
        if use_ascii {
            if let Some(ref ascii_matcher) = self.ascii_matcher {
                match ascii_matcher {
                    #[cfg(feature = "dfa")]
                    AsciiMatcher::AcDfa { matcher, to_value } => {
                        for hit in matcher.find_overlapping_iter(processed_text) {
                            let raw_value =
                                unsafe { *to_value.get_unchecked(hit.pattern().as_usize()) };
                            if self.process_match::<SINGLE_PT>(raw_value, ctx, state) {
                                return true;
                            }
                        }
                    }
                    AsciiMatcher::DaacBytewise(daac_matcher) => {
                        for hit in daac_matcher.find_overlapping_iter(processed_text) {
                            if self.process_match::<SINGLE_PT>(hit.value(), ctx, state) {
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
                        if self.process_match::<SINGLE_PT>(hit.value(), ctx, state) {
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
    /// so the per-entry `pt_index` mask check is compiled away, and values with
    /// [`DIRECT_RULE_BIT`] set bypass the dedup indirection chain entirely.
    #[inline(always)]
    pub(super) fn process_match<const SINGLE_PT: bool>(
        &self,
        raw_value: u32,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        // Direct path: single-entry Simple pattern with rule_idx encoded in the value.
        // When SINGLE_PT=false, the compiler eliminates this entire branch.
        if SINGLE_PT && raw_value & DIRECT_RULE_BIT != 0 {
            let rule_idx = (raw_value & !DIRECT_RULE_BIT) as usize;
            let generation = state.generation;
            debug_assert!(rule_idx < state.word_states.len());
            let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };
            if word_state.positive_generation != generation {
                word_state.matrix_generation = generation;
                word_state.positive_generation = generation;
                state.touched_indices.push(rule_idx);
            }
            return ctx.exit_early;
        }

        // Indirect path: look up dedup_ranges → entries.
        let pattern_idx = raw_value as usize;
        debug_assert!(pattern_idx < self.ac_dedup_ranges.len());
        let &(start, len) = unsafe { self.ac_dedup_ranges.get_unchecked(pattern_idx) };
        debug_assert!(start + len <= self.ac_dedup_entries.len());
        if len == 1 {
            let entry = unsafe { self.ac_dedup_entries.get_unchecked(start) };
            self.process_entry::<SINGLE_PT>(entry, ctx, state)
        } else {
            let entries = unsafe { self.ac_dedup_entries.get_unchecked(start..start + len) };
            for entry in entries {
                if self.process_entry::<SINGLE_PT>(entry, ctx, state) {
                    return true;
                }
            }
            false
        }
    }

    /// Processes a single [`PatternEntry`] against the current scan state.
    ///
    /// Returns `true` when the owning rule is fully satisfied and `ctx.exit_early` is set.
    /// The `continue` semantics from the old loop body become `return false` here.
    #[inline(always)]
    fn process_entry<const SINGLE_PT: bool>(
        &self,
        entry: &PatternEntry,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        let generation = state.generation;
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

        // SAFETY: rule_idx from PatternEntry is always < rule count (set during construction).
        debug_assert!(rule_idx < state.word_states.len());
        debug_assert!(rule_idx < self.rule_hot.len());

        match kind {
            PatternKind::Simple => {
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };
                if word_state.positive_generation == generation {
                    if ctx.exit_early {
                        return true;
                    }
                    return false;
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
                let rule = unsafe { self.rule_hot.get_unchecked(rule_idx) };
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
                    word_state.matrix_generation = generation;
                    word_state.positive_generation =
                        if rule.and_count == 0 { generation } else { 0 };
                    word_state.remaining_and = rule.and_count as u16;
                    word_state.satisfied_mask = 0;
                    state.touched_indices.push(rule_idx);

                    if rule.use_matrix {
                        Self::init_matrix(
                            unsafe { state.matrix.get_unchecked_mut(rule_idx) },
                            unsafe { state.matrix_status.get_unchecked_mut(rule_idx) },
                            &rule.segment_counts,
                            ctx.num_variants,
                        );
                    }
                }

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
                let rule = unsafe { self.rule_hot.get_unchecked(rule_idx) };
                let word_state = unsafe { state.word_states.get_unchecked_mut(rule_idx) };

                if word_state.not_generation == generation {
                    return false;
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
                            unsafe { state.matrix.get_unchecked_mut(rule_idx) },
                            unsafe { state.matrix_status.get_unchecked_mut(rule_idx) },
                            &rule.segment_counts,
                            ctx.num_variants,
                        );
                    }
                }

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
