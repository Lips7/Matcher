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
//! - **`is_match_inner`** — general path with transform traversal and rule state.
//! - **`process_simple`** — all-simple matchers collecting results. Each hit is a
//!   completed rule; no need for the full state machine.
//!
//! # Safety
//!
//! All functions in this module obtain `&mut SimpleMatchState` from
//! [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`. This is safe because the static is
//! `#[thread_local]` (no cross-thread sharing) and the functions are not re-entrant.
//! See [`SIMPLE_MATCH_STATE`] for the full safety argument.

use std::borrow::Cow;

use crate::process::ProcessedTextMasks;
use crate::process::step::TransformStep;
use crate::process::variant::return_string_to_pool;

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

    /// Scans every transformed variant through the automaton and evaluates rule hits.
    ///
    /// Iterates over each text variant in `processed_text_process_type_masks`, skipping
    /// variants with a zero mask (unused process-type slots). For each variant, constructs
    /// a [`ScanContext`] and calls [`scan_variant`](Self::scan_variant).
    ///
    /// Returns `true` if early exit was triggered (only possible when `exit_early` is `true`).
    pub(super) fn scan_all_variants<'a>(
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
            if self.scan_variant(text_variant.text.as_ref(), ctx, state) {
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
    pub(super) fn scan_variant(
        &self,
        processed_text: &str,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        self.scan
            .for_each_match_value(processed_text, ctx.is_ascii, |raw_value| {
                self.process_match(raw_value, ctx, state)
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
    pub(super) fn process_match(
        &self,
        raw_value: u32,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        match self.scan.patterns().dispatch(raw_value) {
            PatternDispatch::DirectRule { rule_idx, pt_index } => {
                if ctx.process_type_mask & (1u64 << pt_index) == 0 {
                    return false;
                }
                state.mark_positive(rule_idx);
                ctx.exit_early
            }
            PatternDispatch::SingleEntry(entry) => self.rules.process_entry(entry, ctx, state),
            PatternDispatch::Entries(entries) => {
                for entry in entries {
                    if self.rules.process_entry(entry, ctx, state) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Lazy `is_match` path that streams transform byte iterators directly into the AC
    /// scan engine, avoiding intermediate `String` allocation for leaf tree nodes.
    ///
    /// Walks the process-type tree manually instead of using [`walk_process_tree`]:
    ///
    /// - **Leaf terminals where step is a no-op** (Fanjian/Normalize/PinYin on ASCII):
    ///   Scan the parent text with the child's mask, reusing the parent variant index.
    /// - **Leaf terminals with real transforms**: Stream a byte iterator into AC.
    /// - **Non-leaf terminals**: Materialize as usual (children need the text).
    ///
    /// # No-op detection
    ///
    /// When the parent text is pure ASCII, certain transforms are guaranteed no-ops:
    /// - Fanjian: only maps non-ASCII CJK codepoints
    /// - Normalize: all patterns contain non-ASCII characters
    /// - PinYin/PinYinChar: only maps non-ASCII CJK codepoints
    ///
    /// For these cases the child's text equals the parent's, so we reuse the parent's
    /// variant index and scan with only the child's mask bits, avoiding both allocation
    /// and redundant AC scanning.
    ///
    /// # Safety
    ///
    /// Obtains `&mut SimpleMatchState` from [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`.
    /// See module-level safety documentation.
    #[inline(always)]
    pub(super) fn is_match_lazy(&self, text: &str) -> bool {
        let tree = self.process.tree();
        let num_variants = tree.len();
        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());

        if self.scan.patterns().is_empty() {
            return false;
        }

        let root_is_ascii = text.is_ascii();

        // Scan root (ProcessType::None) if it terminates here.
        if tree[0].pt_index_mask != 0 {
            let ctx = ScanContext {
                text_index: 0,
                process_type_mask: tree[0].pt_index_mask,
                num_variants,
                exit_early: true,
                is_ascii: root_is_ascii,
            };
            if self.scan_variant(text, ctx, state) {
                return true;
            }
        }

        if tree[0].children.is_empty() {
            return self.rules.has_match(state);
        }

        // Arena for materialized non-leaf texts. Index 0 = root (borrowed).
        let mut texts: Vec<Cow<'_, str>> = Vec::with_capacity(num_variants);
        texts.push(Cow::Borrowed(text));
        let mut ascii_flags: Vec<bool> = Vec::with_capacity(num_variants);
        ascii_flags.push(root_is_ascii);

        // Maps tree node index -> arena index for its text.
        let mut node_arena: Vec<usize> = vec![0; num_variants];
        // Maps tree node index -> variant index used in ScanContext::text_index.
        let mut node_variant: Vec<usize> = vec![0; num_variants];
        let mut variant_counter = 1usize;
        let mut stopped = false;

        'walk: for node_idx in 0..tree.len() {
            let num_children = tree[node_idx].children.len();
            if num_children == 0 {
                continue;
            }
            let parent_aidx = node_arena[node_idx];
            let parent_vi = node_variant[node_idx];

            for ci in 0..num_children {
                let child_idx = tree[node_idx].children[ci];
                let child = &tree[child_idx];
                let step = child
                    .step
                    .expect("non-root process tree nodes always cache a transform step");
                let is_leaf = child.children.is_empty();
                let parent_ascii = ascii_flags[parent_aidx];

                if is_leaf {
                    if child.pt_index_mask != 0 {
                        // Check if the step is guaranteed to be a no-op on ASCII text.
                        // Fanjian/Normalize/PinYin only operate on non-ASCII codepoints,
                        // so pure-ASCII input passes through unchanged.
                        let is_noop = parent_ascii
                            && matches!(
                                step,
                                TransformStep::Fanjian(_)
                                    | TransformStep::Normalize(_)
                                    | TransformStep::PinYin(_)
                                    | TransformStep::PinYinChar(_)
                            );

                        stopped = if is_noop {
                            // No-op: scan parent text with child's mask,
                            // reusing the parent's variant index.
                            let ctx = ScanContext {
                                text_index: parent_vi,
                                process_type_mask: child.pt_index_mask,
                                num_variants,
                                exit_early: true,
                                is_ascii: parent_ascii,
                            };
                            self.scan_variant(texts[parent_aidx].as_ref(), ctx, state)
                        } else {
                            // Real transform: stream byte iterator into AC.
                            let vi = variant_counter;
                            variant_counter += 1;
                            let ctx = ScanContext {
                                text_index: vi,
                                process_type_mask: child.pt_index_mask,
                                num_variants,
                                exit_early: true,
                                is_ascii: parent_ascii,
                            };
                            self.scan_variant_streaming(
                                step,
                                texts[parent_aidx].as_ref(),
                                ctx,
                                state,
                            )
                        };

                        if stopped {
                            break 'walk;
                        }
                    }
                } else {
                    // Non-leaf: materialize for children.
                    let output = step.apply(texts[parent_aidx].as_ref(), parent_ascii);
                    let (child_aidx, child_vi) = match output.changed {
                        Some(s) => {
                            let idx = texts.len();
                            texts.push(Cow::Owned(s));
                            ascii_flags.push(output.is_ascii);
                            let vi = variant_counter;
                            variant_counter += 1;
                            (idx, vi)
                        }
                        None => (parent_aidx, parent_vi),
                    };
                    node_arena[child_idx] = child_aidx;
                    node_variant[child_idx] = child_vi;

                    // Scan if this node terminates.
                    if child.pt_index_mask != 0 {
                        let ctx = ScanContext {
                            text_index: child_vi,
                            process_type_mask: child.pt_index_mask,
                            num_variants,
                            exit_early: true,
                            is_ascii: ascii_flags[child_aidx],
                        };
                        stopped = self.scan_variant(texts[child_aidx].as_ref(), ctx, state);
                        if stopped {
                            break 'walk;
                        }
                    }
                }
            }
        }

        // Return owned strings to pool.
        for cow in texts {
            if let Cow::Owned(s) = cow {
                return_string_to_pool(s);
            }
        }

        if stopped {
            return true;
        }

        self.rules.has_match(state)
    }

    /// Streams a transform step's byte iterator through the AC scan engine.
    ///
    /// Instead of materializing the transform output into a `String` and scanning it,
    /// creates a byte-by-byte iterator from the step's inner matcher and feeds it into
    /// [`ScanPlan::for_each_match_value_from_iter`](super::engine::ScanPlan::for_each_match_value_from_iter).
    ///
    /// For [`TransformStep::None`], falls back to scanning `parent_text` directly.
    #[inline(always)]
    fn scan_variant_streaming(
        &self,
        step: &TransformStep,
        parent_text: &str,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        match step {
            TransformStep::None => self.scan_variant(parent_text, ctx, state),
            TransformStep::Fanjian(matcher) => self.scan.for_each_match_value_from_iter(
                matcher.byte_iter(parent_text),
                ctx.is_ascii,
                |raw_value| self.process_match(raw_value, ctx, state),
            ),
            TransformStep::Delete(matcher) => self.scan.for_each_match_value_from_iter(
                matcher.byte_iter(parent_text),
                ctx.is_ascii,
                |raw_value| self.process_match(raw_value, ctx, state),
            ),
            TransformStep::Normalize(matcher) => self.scan.for_each_match_value_from_iter(
                matcher.byte_iter(parent_text),
                ctx.is_ascii,
                |raw_value| self.process_match(raw_value, ctx, state),
            ),
            TransformStep::PinYin(matcher) | TransformStep::PinYinChar(matcher) => {
                self.scan.for_each_match_value_from_iter(
                    matcher.byte_iter(parent_text),
                    ctx.is_ascii,
                    |raw_value| self.process_match(raw_value, ctx, state),
                )
            }
        }
    }
}
