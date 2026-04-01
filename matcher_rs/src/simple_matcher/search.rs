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
//! stream transform byte iterators directly into the AC engine (zero allocation), while
//! non-leaf nodes materialize their output for children. An `exit_early` flag controls
//! whether the walk stops on the first satisfied rule (`is_match`) or exhausts all
//! variants (`process`).
//!
//! # Safety
//!
//! All functions in this module obtain `&mut SimpleMatchState` from
//! [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`. This is safe because the static is
//! `#[thread_local]` (no cross-thread sharing) and the functions are not re-entrant.
//! See [`SIMPLE_MATCH_STATE`] for the full safety argument.

use std::borrow::Cow;

use crate::process::step::TransformStep;
use crate::process::string_pool::return_string_to_pool;
use crate::process::transform::simd::multibyte_density;

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

        let use_bytewise =
            multibyte_density(text.as_bytes()) < self.scan.charwise_density_threshold();
        let _ = self
            .scan
            .for_each_match_value(text, use_bytewise, |raw_value| {
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
            .for_each_match_value(processed_text, ctx.use_bytewise, |raw_value| {
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
    fn process_match(
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

    /// Unified tree walk that transforms, scans, and evaluates rules in a single pass.
    ///
    /// Walks the process-type trie built at construction time, scanning each text variant
    /// as soon as it is produced:
    ///
    /// - **Root**: Scanned directly if it terminates (`pt_index_mask != 0`).
    /// - **Leaf + ASCII no-op** (currently only Fanjian on ASCII text):
    ///   Reuses the parent's text and variant index, scans with the child's mask bits.
    /// - **Leaf + real transform**: Streams a byte iterator directly into the AC engine
    ///   via [`scan_variant_streaming`](Self::scan_variant_streaming), avoiding allocation.
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
        let tree = self.process.tree();
        let num_variants = tree.len();
        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());

        if self.scan.patterns().is_empty() {
            return false;
        }

        let root_density = multibyte_density(text.as_bytes());

        // Scan root (ProcessType::None) if it terminates here.
        if tree[0].pt_index_mask != 0 {
            let ctx = ScanContext {
                text_index: 0,
                process_type_mask: tree[0].pt_index_mask,
                num_variants,
                exit_early,
                use_bytewise: root_density < self.scan.charwise_density_threshold(),
            };
            if self.scan_variant(text, ctx, state) {
                return true;
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
        // `density_flags[i]` — multi-byte density of the text at arena index `i`.
        // `density == 0.0` ≡ pure ASCII (used for no-op detection and step.apply).
        // `density < scan.charwise_density_threshold()` ≡ use bytewise engine.
        let mut density_flags: Vec<f32> = Vec::with_capacity(num_variants);
        density_flags.push(root_density);

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
                let parent_density = density_flags[parent_aidx];
                let parent_ascii = parent_density == 0.0;
                let parent_use_bytewise = parent_density < self.scan.charwise_density_threshold();

                if is_leaf {
                    if child.pt_index_mask != 0 {
                        let is_noop = parent_ascii && step.is_noop_on_ascii_input();

                        stopped = if is_noop {
                            let ctx = ScanContext {
                                text_index: parent_vi,
                                process_type_mask: child.pt_index_mask,
                                num_variants,
                                exit_early,
                                use_bytewise: parent_use_bytewise,
                            };
                            self.scan_variant(texts[parent_aidx].as_ref(), ctx, state)
                        } else {
                            let vi = variant_counter;
                            variant_counter += 1;
                            // For streaming leaves, derive use_bytewise from the step's
                            // output characteristics; ASCII parent → always bytewise.
                            let child_use_bytewise = if parent_ascii {
                                true
                            } else {
                                step.output_use_bytewise(parent_use_bytewise)
                            };
                            let ctx = ScanContext {
                                text_index: vi,
                                process_type_mask: child.pt_index_mask,
                                num_variants,
                                exit_early,
                                use_bytewise: child_use_bytewise,
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
                    let output = step.apply(texts[parent_aidx].as_ref(), parent_density);
                    let (child_aidx, child_vi) = match output.changed {
                        Some(s) => {
                            let idx = texts.len();
                            density_flags.push(output.output_density);
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
                        let child_density = density_flags[child_aidx];
                        let ctx = ScanContext {
                            text_index: child_vi,
                            process_type_mask: child.pt_index_mask,
                            num_variants,
                            exit_early,
                            use_bytewise: child_density < self.scan.charwise_density_threshold(),
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

        if let Some(results) = results {
            self.rules.collect_matches(state, results);
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
                ctx.use_bytewise,
                |raw_value| self.process_match(raw_value, ctx, state),
            ),
            TransformStep::Delete(matcher) => self.scan.for_each_match_value_from_iter(
                matcher.byte_iter(parent_text),
                ctx.use_bytewise,
                |raw_value| self.process_match(raw_value, ctx, state),
            ),
            TransformStep::Normalize(matcher) => self.scan.for_each_match_value_from_iter(
                matcher.byte_iter(parent_text),
                ctx.use_bytewise,
                |raw_value| self.process_match(raw_value, ctx, state),
            ),
            TransformStep::PinYin(matcher) | TransformStep::PinYinChar(matcher) => {
                self.scan.for_each_match_value_from_iter(
                    matcher.byte_iter(parent_text),
                    ctx.use_bytewise,
                    |raw_value| self.process_match(raw_value, ctx, state),
                )
            }
        }
    }
}
