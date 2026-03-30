//! Hot-path scan and rule evaluation for [`super::SimpleMatcher`].
//!
//! This module implements the runtime half of the two-pass matching pipeline. Given a
//! compiled [`SimpleMatcher`], it:
//!
//! 1. Obtains a `&mut` reference to the thread-local [`SIMPLE_MATCH_STATE`].
//! 2. Transforms the input text through the process-type tree (or skips transformation
//!    for [`SearchMode::AllSimple`](super::SearchMode::AllSimple) matchers).
//! 3. Scans each text variant through the automata ([`ScanPlan`](super::engine::ScanPlan)).
//! 4. Dispatches each raw match value into the rule state machine
//!    ([`RuleSet::process_entry`](super::rule::RuleSet::process_entry)).
//! 5. Collects or checks results depending on the caller (`is_match` vs `process`).
//!
//! # Fast paths
//!
//! - **`is_match_simple`** — all rules are single-literal, no transforms. Delegates
//!   directly to the automaton's `is_match`.
//! - **`is_match_inner<true>`** — single process type. The `SINGLE_PT` const generic
//!   compiles out the process-type mask check in `process_entry`.
//! - **`process_simple`** — all-simple matchers collecting results. Each hit is a
//!   completed rule; no need for the full state machine.
//!
//! # Safety
//!
//! All functions in this module obtain `&mut SimpleMatchState` from
//! [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`. This is safe because the static is
//! `#[thread_local]` (no cross-thread sharing) and the functions are not re-entrant.
//! See [`SIMPLE_MATCH_STATE`] for the full safety argument.

use crate::process::variant::return_string_to_pool;
use crate::process::{ProcessedTextMasks, return_processed_string_to_pool, walk_process_tree};

use super::rule::PatternDispatch;
use super::state::{SIMPLE_MATCH_STATE, ScanContext, SimpleMatchState};
use super::{SimpleMatcher, SimpleResult};

/// Hot-path search helpers layered on top of the compiled scan engines.
impl SimpleMatcher {
    /// Fast path for matchers that contain only direct simple literal rules.
    ///
    /// No state machine, no thread-local state, no transform tree — just a single
    /// automaton `is_match` call. Used when [`SearchMode::AllSimple`](super::SearchMode::AllSimple)
    /// is active.
    pub(super) fn is_match_simple(&self, text: &str) -> bool {
        self.scan.is_match(text)
    }

    /// Specialized `is_match` for matchers with a single-bit ProcessType (tree = [root, child]).
    ///
    /// Bypasses the full `walk_process_tree` machinery: no `TRANSFORM_STATE` TLS access,
    /// no masks pool, no `scanned_masks` tracking, no `dedup_insert`. Instead, applies the
    /// single transform step directly and scans at most two texts.
    ///
    /// # Safety
    ///
    /// Obtains `&mut SimpleMatchState` from [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`.
    /// See module-level safety documentation.
    #[inline(always)]
    pub(super) fn is_match_single_step(&self, text: &str) -> bool {
        let tree = self.process.tree();
        debug_assert!(
            tree.len() == 2,
            "is_match_single_step requires exactly 2 tree nodes"
        );

        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());

        let root_is_ascii = text.is_ascii();
        let root_mask = tree[0].pt_index_mask;
        let child_node = &tree[1];
        let child_mask = child_node.pt_index_mask;

        // Apply the single transform step.
        let step = child_node
            .step
            .expect("non-root process tree nodes always cache a transform step");
        let output = step.apply(text, root_is_ascii);

        match output.changed {
            None => {
                // Transform was a no-op — single unique text with merged mask.
                let merged_mask = root_mask | child_mask;
                if merged_mask != 0 {
                    let ctx = ScanContext {
                        text_index: 0,
                        process_type_mask: merged_mask,
                        num_variants: 1,
                        exit_early: true,
                        is_ascii: root_is_ascii,
                    };
                    if self.scan_variant::<true>(text, ctx, state) {
                        return true;
                    }
                }
            }
            Some(transformed) => {
                // Two distinct variants: root (original) and child (transformed).
                if root_mask != 0 {
                    let ctx = ScanContext {
                        text_index: 0,
                        process_type_mask: root_mask,
                        num_variants: 2,
                        exit_early: true,
                        is_ascii: root_is_ascii,
                    };
                    if self.scan_variant::<true>(text, ctx, state) {
                        return_string_to_pool(transformed);
                        return true;
                    }
                }
                let ctx = ScanContext {
                    text_index: 1,
                    process_type_mask: child_mask,
                    num_variants: 2,
                    exit_early: true,
                    is_ascii: output.is_ascii,
                };
                let matched = self.scan_variant::<true>(&transformed, ctx, state);
                return_string_to_pool(transformed);
                if matched {
                    return true;
                }
            }
        }

        self.rules.has_match(state)
    }

    /// Specialized `process` for matchers with a single-bit ProcessType (tree = [root, child]).
    ///
    /// Bypasses the full `walk_process_tree` machinery: no `TRANSFORM_STATE` TLS access,
    /// no masks pool, no `scanned_masks` tracking, no `dedup_insert`. Instead, applies the
    /// single transform step directly and scans at most two texts, then collects all matches.
    ///
    /// # Safety
    ///
    /// Obtains `&mut SimpleMatchState` from [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`.
    /// See module-level safety documentation.
    #[inline(always)]
    pub(super) fn process_single_step<'a>(
        &'a self,
        text: &'a str,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        let tree = self.process.tree();
        debug_assert!(
            tree.len() == 2,
            "process_single_step requires exactly 2 tree nodes"
        );

        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());

        let root_is_ascii = text.is_ascii();
        let root_mask = tree[0].pt_index_mask;
        let child_node = &tree[1];
        let child_mask = child_node.pt_index_mask;

        let step = child_node
            .step
            .expect("non-root process tree nodes always cache a transform step");
        let output = step.apply(text, root_is_ascii);

        match output.changed {
            None => {
                let merged_mask = root_mask | child_mask;
                if merged_mask != 0 {
                    let ctx = ScanContext {
                        text_index: 0,
                        process_type_mask: merged_mask,
                        num_variants: 1,
                        exit_early: false,
                        is_ascii: root_is_ascii,
                    };
                    self.scan_variant::<true>(text, ctx, state);
                }
            }
            Some(transformed) => {
                if root_mask != 0 {
                    let ctx = ScanContext {
                        text_index: 0,
                        process_type_mask: root_mask,
                        num_variants: 2,
                        exit_early: false,
                        is_ascii: root_is_ascii,
                    };
                    self.scan_variant::<true>(text, ctx, state);
                }
                if child_mask != 0 {
                    let ctx = ScanContext {
                        text_index: 1,
                        process_type_mask: child_mask,
                        num_variants: 2,
                        exit_early: false,
                        is_ascii: output.is_ascii,
                    };
                    self.scan_variant::<true>(&transformed, ctx, state);
                }
                return_string_to_pool(transformed);
            }
        }

        self.rules.collect_matches(state, results);
    }

    /// General `is_match` path for matchers that need transform traversal or rule state.
    ///
    /// Walks the process-type tree with [`walk_process_tree`], scanning each transformed
    /// variant as it is produced. If any variant scan requests early exit (a rule is
    /// already satisfied), the tree walk stops and returns `true` immediately. Otherwise,
    /// after all variants are scanned, the touched rules are checked via
    /// [`RuleSet::has_match`](super::rule::RuleSet::has_match).
    ///
    /// # Const generic `SINGLE_PT`
    ///
    /// When `true`, the process-type mask check inside
    /// [`RuleSet::process_entry`](super::rule::RuleSet::process_entry) is compiled out.
    ///
    /// # Safety
    ///
    /// Obtains `&mut SimpleMatchState` from [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`.
    /// See module-level safety documentation.
    #[inline(always)]
    pub(super) fn is_match_inner<const SINGLE_PT: bool>(&self, text: &str) -> bool {
        let tree = self.process.tree();
        let max_pt = tree.len();
        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());
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
        let result = self.rules.has_match(state);
        return_processed_string_to_pool(text_masks);
        result
    }

    /// Collects matches for an all-simple matcher without building transformed variants.
    ///
    /// Every automaton hit is a completed rule, so results are emitted immediately
    /// via [`RuleSet::push_result_if_new`](super::rule::RuleSet::push_result_if_new).
    /// Deduplication is handled by the generation stamp in [`SimpleMatchState::mark_positive`].
    ///
    /// All patterns have [`DIRECT_RULE_BIT`](super::rule::DIRECT_RULE_BIT) encoding
    /// in all-simple mode, so every hit resolves to [`PatternDispatch::DirectRule`].
    ///
    /// # Safety
    ///
    /// Obtains `&mut SimpleMatchState` from [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`.
    /// See module-level safety documentation.
    pub(super) fn process_simple<'a>(&'a self, text: &'a str, results: &mut Vec<SimpleResult<'a>>) {
        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());

        let _ = self
            .scan
            .for_each_match_value(text, text.is_ascii(), |raw_value| {
                match self.scan.patterns().dispatch(raw_value) {
                    PatternDispatch::DirectRule { rule_idx, .. } => {
                        self.rules.push_result_if_new(rule_idx, state, results);
                    }
                    PatternDispatch::SingleEntry(entry) => {
                        self.rules
                            .push_result_if_new(entry.rule_idx as usize, state, results);
                    }
                    PatternDispatch::Entries(entries) => {
                        for entry in entries {
                            self.rules
                                .push_result_if_new(entry.rule_idx as usize, state, results);
                        }
                    }
                }
                false
            });
    }

    /// Collects matches from a precomputed list of transformed text variants.
    ///
    /// Used by [`SimpleMatcher::process_into`](super::SimpleMatcher::process_into) after
    /// the process-type tree has been walked and all variants are available. Scans every
    /// variant, then collects all satisfied rules into `results`.
    ///
    /// # Safety
    ///
    /// Obtains `&mut SimpleMatchState` from [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`.
    /// See module-level safety documentation.
    pub(super) fn process_preprocessed_into<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());

        self.scan_all_variants(processed_text_process_type_masks, state, false);
        self.rules.collect_matches(state, results);
    }

    /// Scans every transformed variant, selecting the single-process-type fast path when possible.
    ///
    /// Dispatches to [`scan_all_variants_inner`](Self::scan_all_variants_inner) with the
    /// appropriate `SINGLE_PT` const generic based on the matcher's [`SearchMode`](super::SearchMode).
    ///
    /// Returns `true` if early exit was triggered (only possible when `exit_early` is `true`).
    pub(super) fn scan_all_variants<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        state: &mut SimpleMatchState,
        exit_early: bool,
    ) -> bool {
        if self.process.mode().single_pt_index().is_some() {
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

    /// Shared variant-scan loop for both general and single-process-type modes.
    ///
    /// Iterates over each text variant in `processed_text_process_type_masks`, skipping
    /// variants with a zero mask (unused process-type slots). For each variant, constructs
    /// a [`ScanContext`] and calls [`scan_variant`](Self::scan_variant).
    ///
    /// Returns `true` if any variant scan triggered early exit.
    #[inline(always)]
    fn scan_all_variants_inner<'a, const SINGLE_PT: bool>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        state: &mut SimpleMatchState,
        exit_early: bool,
    ) -> bool {
        if self.scan.patterns().is_empty() {
            return false;
        }

        let num_variants = processed_text_process_type_masks.len();

        for (index, text_variant) in processed_text_process_type_masks.iter().enumerate() {
            if text_variant.mask == 0 {
                continue;
            }
            let ctx = ScanContext {
                text_index: index,
                process_type_mask: text_variant.mask,
                num_variants,
                exit_early,
                is_ascii: text_variant.is_ascii,
            };
            if self.scan_variant::<SINGLE_PT>(text_variant.text.as_ref(), ctx, state) {
                return true;
            }
        }

        false
    }

    /// Scans one processed text variant and forwards each raw hit into rule evaluation.
    ///
    /// Delegates to [`ScanPlan::for_each_match_value`](super::engine::ScanPlan::for_each_match_value)
    /// with [`process_match`](Self::process_match) as the callback. Returns `true` if
    /// the callback triggered early exit.
    #[inline(always)]
    pub(super) fn scan_variant<const SINGLE_PT: bool>(
        &self,
        processed_text: &str,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        self.scan
            .for_each_match_value(processed_text, ctx.is_ascii, |raw_value| {
                self.process_match::<SINGLE_PT>(raw_value, ctx, state)
            })
    }

    /// Processes one raw match value reported by the scan engine.
    ///
    /// Dispatches the raw value through [`PatternIndex::dispatch`](super::rule::PatternIndex::dispatch)
    /// and handles each [`PatternDispatch`] variant:
    ///
    /// - [`DirectRule`](PatternDispatch::DirectRule) — marks the rule positive immediately
    ///   and returns `exit_early` (no state machine needed).
    /// - [`SingleEntry`](PatternDispatch::SingleEntry) — feeds the single entry into
    ///   [`RuleSet::process_entry`](super::rule::RuleSet::process_entry).
    /// - [`Entries`](PatternDispatch::Entries) — iterates all entries, short-circuiting
    ///   if any one triggers early exit.
    ///
    /// Returns `true` when the caller should stop scanning (early exit satisfied).
    #[inline(always)]
    pub(super) fn process_match<const SINGLE_PT: bool>(
        &self,
        raw_value: u32,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        match self.scan.patterns().dispatch(raw_value) {
            PatternDispatch::DirectRule { rule_idx, pt_index } => {
                if !SINGLE_PT && ctx.process_type_mask & (1u64 << pt_index) == 0 {
                    return false;
                }
                state.mark_positive(rule_idx);
                ctx.exit_early
            }
            PatternDispatch::SingleEntry(entry) => {
                self.rules.process_entry::<SINGLE_PT>(entry, ctx, state)
            }
            PatternDispatch::Entries(entries) => {
                for entry in entries {
                    if self.rules.process_entry::<SINGLE_PT>(entry, ctx, state) {
                        return true;
                    }
                }
                false
            }
        }
    }
}
