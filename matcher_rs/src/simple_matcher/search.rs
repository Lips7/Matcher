//! Hot-path scan and rule evaluation for [`super::SimpleMatcher`].
//!
//! This module implements the runtime half of the two-pass matching pipeline. Given a
//! compiled [`SimpleMatcher`], it:
//!
//! 1. Obtains a `&mut` reference to the thread-local [`SIMPLE_MATCH_STATE`].
//! 2. Walks the process-type tree, transforming and scanning each variant immediately.
//! 3. Dispatches each raw match value into the rule state machine
//!    ([`RuleSet::process_entry`](super::rule::RuleSet::process_entry)).
//! 4. Collects or checks results depending on the caller (`is_match` vs `process`).
//!
//! # Fast paths
//!
//! - **`is_match_simple`** — all rules are single-literal, no transforms. Delegates
//!   directly to the automaton's `is_match`.
//! - **`process_simple`** — all-simple matchers collecting results. Each hit is a
//!   completed rule; no need for the full state machine.
//!
//! # Unified tree walk
//!
//! The general path uses [`walk_and_scan`](SimpleMatcher::walk_and_scan), which walks the
//! process-type trie once, scanning each variant as soon as it is produced. Leaf nodes
//! that are no-ops on ASCII input reuse the parent text; otherwise they materialize via
//! `TransformStep::apply`. Non-leaf nodes materialize their output for children. An
//! `exit_early` flag controls whether the walk stops on the first satisfied rule
//! (`is_match`) or exhausts all variants (`process`).
//!
//! # Safety
//!
//! All functions in this module obtain `&mut SimpleMatchState` from
//! [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`. This is safe because the static is
//! `#[thread_local]` (no cross-thread sharing) and the functions are not re-entrant.
//! See [`SIMPLE_MATCH_STATE`] for the full safety argument.

use std::borrow::Cow;

use tinyvec::TinyVec;

use crate::process::string_pool::return_string_to_pool;

use super::rule::{
    DIRECT_PT_MASK, DIRECT_PT_SHIFT, DIRECT_RULE_BIT, DIRECT_RULE_MASK, PatternDispatch,
};
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
    /// All patterns have [`DIRECT_RULE_BIT`] encoding
    /// in all-simple mode, so every hit is resolved inline via the bit-packed value.
    ///
    /// # Safety
    ///
    /// Obtains `&mut SimpleMatchState` from [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`.
    /// See module-level safety documentation.
    pub(super) fn process_simple<'a>(&'a self, text: &'a str, results: &mut Vec<SimpleResult<'a>>) {
        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());

        self.scan
            .for_each_rule_idx_simple(text, text.is_ascii(), |rule_idx| {
                self.rules.push_result_if_new(rule_idx, state, results);
            });
    }

    /// Scans one processed text variant and forwards each raw hit into rule evaluation.
    ///
    /// Delegates to [`ScanPlan::for_each_match_value`](super::engine::ScanPlan::for_each_match_value)
    /// with [`process_match`](Self::process_match) as the callback. Returns `true` if
    /// the callback triggered early exit.
    #[inline(always)]
    fn scan_variant(
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
    /// Checks [`DIRECT_RULE_BIT`] inline for the common direct-rule case (marks positive
    /// immediately, returns `exit_early`). Falls through to
    /// [`PatternIndex::dispatch_indirect`](super::rule::PatternIndex::dispatch_indirect)
    /// for non-direct values (`SingleEntry` / `Entries`).
    ///
    /// Returns `true` when the caller should stop scanning (early exit satisfied).
    #[inline(always)]
    fn process_match(
        &self,
        raw_value: u32,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        if raw_value & DIRECT_RULE_BIT != 0 {
            let pt_index = ((raw_value & DIRECT_PT_MASK) >> DIRECT_PT_SHIFT) as u8;
            if ctx.process_type_mask & (1u64 << pt_index) == 0 {
                return false;
            }
            let rule_idx = (raw_value & DIRECT_RULE_MASK) as usize;
            if state.mark_positive(rule_idx) {
                state.resolved_count += 1;
            }
            return ctx.exit_early;
        }
        match self.scan.patterns().dispatch_indirect(raw_value) {
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

    /// Unified tree walk that transforms, scans, and evaluates rules in a single pass.
    ///
    /// Walks the process-type trie built at construction time, scanning each text variant
    /// as soon as it is produced:
    ///
    /// - **Root**: Scanned directly if it terminates (`pt_index_mask != 0`).
    /// - **Leaf + ASCII no-op** (currently only Fanjian on ASCII text):
    ///   Reuses the parent's text and variant index, scans with the child's mask bits.
    /// - **Leaf + real transform**: Materializes the transform output, scans, and returns
    ///   the string to the pool.
    /// - **Non-leaf**: Materializes the transform output for children, scans if the node
    ///   also terminates.
    ///
    /// When `exit_early` is `true` (used by `is_match`), the walk stops as soon as any
    /// rule is satisfied. When `false` (used by `process`), all variants are exhausted and
    /// results are collected into `results`.
    ///
    /// # Safety
    ///
    /// Obtains `&mut SimpleMatchState` from [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`.
    /// See module-level safety documentation.
    #[inline(always)]
    pub(super) fn walk_and_scan<'a>(
        &'a self,
        text: &'a str,
        exit_early: bool,
        results: Option<&mut Vec<SimpleResult<'a>>>,
    ) -> bool {
        let tree = &self.tree;
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
                exit_early,
                is_ascii: root_is_ascii,
            };
            if self.scan_variant(text, ctx, state) {
                return true;
            }
            if !exit_early
                && !self.rules.has_not_rules()
                && state.resolved_count >= self.rules.len()
            {
                if let Some(results) = results {
                    self.rules.collect_matches(state, results);
                }
                return self.rules.has_match(state);
            }
        }

        if tree[0].children.is_empty() {
            if let Some(results) = results {
                self.rules.collect_matches(state, results);
            }
            return self.rules.has_match(state);
        }

        // Arena for materialized non-leaf texts. Index 0 = root (borrowed).
        let mut texts: Vec<Cow<'_, str>> = Vec::with_capacity(num_variants);
        texts.push(Cow::Borrowed(text));
        // `ascii_flags[i]` — whether the text at arena index `i` is pure ASCII.
        // TinyVec inlines up to 16 entries, covering all practical trees.
        let mut ascii_flags: TinyVec<[bool; 16]> = TinyVec::new();
        ascii_flags.push(root_is_ascii);

        // Maps tree node index -> arena index for its text.
        let mut node_arena: TinyVec<[usize; 16]> = TinyVec::new();
        node_arena.resize(num_variants, 0);
        // Maps tree node index -> variant index used in ScanContext::text_index.
        let mut node_variant: TinyVec<[usize; 16]> = TinyVec::new();
        node_variant.resize(num_variants, 0);
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
                        let is_noop = parent_ascii && step.is_noop_on_ascii_input();

                        // Try to produce a changed variant; skip apply() entirely
                        // when the step is a known no-op on ASCII input.
                        let changed = if !is_noop {
                            let output = step.apply(texts[parent_aidx].as_ref(), parent_ascii);
                            output.changed.map(|s| (s, output.is_ascii))
                        } else {
                            None
                        };

                        stopped = if let Some((s, is_ascii)) = changed {
                            let vi = variant_counter;
                            variant_counter += 1;
                            let ctx = ScanContext {
                                text_index: vi,
                                process_type_mask: child.pt_index_mask,
                                num_variants,
                                exit_early,
                                is_ascii,
                            };
                            let result = self.scan_variant(&s, ctx, state);
                            return_string_to_pool(s);
                            result
                        } else {
                            let ctx = ScanContext {
                                text_index: parent_vi,
                                process_type_mask: child.pt_index_mask,
                                num_variants,
                                exit_early,
                                is_ascii: parent_ascii,
                            };
                            self.scan_variant(texts[parent_aidx].as_ref(), ctx, state)
                        };

                        if stopped {
                            break 'walk;
                        }
                        if !exit_early
                            && !self.rules.has_not_rules()
                            && state.resolved_count >= self.rules.len()
                        {
                            break 'walk;
                        }
                    }
                } else {
                    // Non-leaf: materialize for children.
                    let output = step.apply(texts[parent_aidx].as_ref(), parent_ascii);
                    let (child_aidx, child_vi) = match output.changed {
                        Some(s) => {
                            let idx = texts.len();
                            ascii_flags.push(output.is_ascii);
                            texts.push(Cow::Owned(s));
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
                            exit_early,
                            is_ascii: ascii_flags[child_aidx],
                        };
                        stopped = self.scan_variant(texts[child_aidx].as_ref(), ctx, state);
                        if stopped {
                            break 'walk;
                        }
                        if !exit_early
                            && !self.rules.has_not_rules()
                            && state.resolved_count >= self.rules.len()
                        {
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

        if let Some(results) = results {
            self.rules.collect_matches(state, results);
        }
        self.rules.has_match(state)
    }
}
