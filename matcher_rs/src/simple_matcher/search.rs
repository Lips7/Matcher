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

use crate::process::step::TransformStep;
use crate::process::string_pool::return_string_to_pool;

use super::engine::{CHARWISE_DENSITY_THRESHOLD, text_non_ascii_density};
use super::rule::{
    DIRECT_PT_MASK, DIRECT_PT_SHIFT, DIRECT_RULE_BIT, DIRECT_RULE_MASK, PatternDispatch,
};
use super::state::{SIMPLE_MATCH_STATE, ScanContext, ScanState};
use super::{SimpleMatcher, SimpleResult};

/// Hot-path search helpers layered on top of the compiled scan engines.
impl SimpleMatcher {
    /// Fast path for matchers that contain only direct simple literal rules.
    pub(super) fn is_match_simple(&self, text: &str) -> bool {
        self.scan.is_match(text)
    }

    /// Collects matches for an all-simple matcher without building transformed variants.
    pub(super) fn process_simple<'a>(&'a self, text: &'a str, results: &mut Vec<SimpleResult<'a>>) {
        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());
        let mut ss = state.as_scan_state();

        let density = text_non_ascii_density(text);
        self.scan
            .for_each_rule_idx_simple(text, density, |rule_idx| {
                self.rules.push_result_if_new(rule_idx, &mut ss, results);
            });
    }

    /// Scans one processed text variant and forwards each raw hit into rule evaluation.
    #[inline(always)]
    fn scan_variant(&self, processed_text: &str, ctx: ScanContext, ss: &mut ScanState<'_>) -> bool {
        self.scan
            .for_each_match_value(processed_text, ctx.non_ascii_density, |raw_value| {
                self.process_match(raw_value, ctx, ss)
            })
    }

    /// Processes one raw match value reported by the scan engine.
    #[inline(always)]
    fn process_match(&self, raw_value: u32, ctx: ScanContext, ss: &mut ScanState<'_>) -> bool {
        if raw_value & DIRECT_RULE_BIT != 0 {
            let pt_index = ((raw_value & DIRECT_PT_MASK) >> DIRECT_PT_SHIFT) as u8;
            if ctx.process_type_mask & (1u64 << pt_index) == 0 {
                return false;
            }
            let rule_idx = (raw_value & DIRECT_RULE_MASK) as usize;
            if ss.mark_positive(rule_idx) {
                ss.resolved_count += 1;
            }
            return ctx.exit_early;
        }
        match self.scan.patterns().dispatch_indirect(raw_value) {
            PatternDispatch::SingleEntry(entry) => self.rules.process_entry(entry, ctx, ss),
            PatternDispatch::Entries(entries) => {
                for entry in entries {
                    if self.rules.process_entry(entry, ctx, ss) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Unified tree walk that transforms, scans, and evaluates rules in a single pass.
    #[inline]
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
        let mut ss = state.as_scan_state();

        if self.scan.patterns().is_empty() {
            return false;
        }

        // One SIMD pass: exact non-ASCII byte density for engine dispatch.
        // density == 0.0 ↔ text is pure ASCII (replaces text.is_ascii()).
        let root_density = text_non_ascii_density(text);

        // Scan root (ProcessType::None) if it terminates here.
        if tree[0].pt_index_mask != 0 {
            let ctx = ScanContext {
                text_index: 0,
                process_type_mask: tree[0].pt_index_mask,
                num_variants,
                exit_early,
                non_ascii_density: root_density,
            };
            if self.scan_variant(text, ctx, &mut ss) {
                return true;
            }
            if !exit_early && !self.rules.has_not_rules() && ss.resolved_count >= self.rules.len() {
                if let Some(results) = results {
                    self.rules.collect_matches(&ss, results);
                }
                return self.rules.has_match(&ss);
            }
        }

        if tree[0].children.is_empty() {
            if let Some(results) = results {
                self.rules.collect_matches(&ss, results);
            }
            return self.rules.has_match(&ss);
        }

        // Arena for materialized non-leaf texts. Index 0 = root (borrowed).
        let mut texts: Vec<Cow<'_, str>> = Vec::with_capacity(num_variants);
        texts.push(Cow::Borrowed(text));
        // density_flags[i] — non-ASCII byte density for arena index i.
        // density == 0.0 means pure ASCII (used for both engine dispatch and
        // transform correctness: is_noop_on_ascii_input, step.apply).
        let mut density_flags: TinyVec<[f32; 16]> = TinyVec::new();
        density_flags.push(root_density);

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
                let parent_density = density_flags[parent_aidx];
                let parent_ascii = parent_density == 0.0;

                if is_leaf {
                    if child.pt_index_mask != 0 {
                        let is_noop = parent_ascii && step.is_noop_on_ascii_input();

                        // Fused transform-scan dispatch:
                        //
                        // - DFA available + low density: skip fused, fall through to
                        //   materialize path — DFA+Teddy is 2–5× faster than DAAC
                        //   bytewise streaming on ASCII-heavy text.
                        // - No DFA + low density: stream via DAAC bytewise.
                        // - High density: stream via DAAC charwise.
                        //
                        // Fused paths only cover Delete/Normalize/VariantNorm (never
                        // Romanize), so parent_density is the correct density estimate.
                        let use_fused = !(is_noop
                            || self.scan.has_dfa() && parent_density <= CHARWISE_DENSITY_THRESHOLD);
                        let fused_result = if use_fused {
                            let parent_text = texts[parent_aidx].as_ref();
                            let vi = variant_counter;
                            let ctx = ScanContext {
                                text_index: vi,
                                process_type_mask: child.pt_index_mask,
                                num_variants,
                                exit_early,
                                non_ascii_density: parent_density,
                            };
                            macro_rules! fused {
                                ($m:expr) => {
                                    Some(self.scan.for_each_match_value_from_iter(
                                        $m.filter_bytes(parent_text),
                                        ctx.non_ascii_density,
                                        |v| self.process_match(v, ctx, &mut ss),
                                    ))
                                };
                            }
                            match step {
                                TransformStep::Delete(m) => fused!(m),
                                TransformStep::Normalize(m) => fused!(m),
                                TransformStep::VariantNorm(m) => fused!(m),
                                _ => None,
                            }
                            .inspect(|_| {
                                variant_counter += 1;
                            })
                        } else {
                            None
                        };

                        stopped = if let Some(result) = fused_result {
                            result
                        } else {
                            // Normal path: materialize then scan.
                            let changed = if !is_noop {
                                step.apply(texts[parent_aidx].as_ref(), parent_density)
                            } else {
                                None
                            };

                            if let Some((s, child_density)) = changed {
                                let vi = variant_counter;
                                variant_counter += 1;
                                let ctx = ScanContext {
                                    text_index: vi,
                                    process_type_mask: child.pt_index_mask,
                                    num_variants,
                                    exit_early,
                                    non_ascii_density: child_density,
                                };
                                let result = self.scan_variant(&s, ctx, &mut ss);
                                return_string_to_pool(s);
                                result
                            } else {
                                let ctx = ScanContext {
                                    text_index: parent_vi,
                                    process_type_mask: child.pt_index_mask,
                                    num_variants,
                                    exit_early,
                                    non_ascii_density: parent_density,
                                };
                                self.scan_variant(texts[parent_aidx].as_ref(), ctx, &mut ss)
                            }
                        };

                        if stopped {
                            break 'walk;
                        }
                        if !exit_early
                            && !self.rules.has_not_rules()
                            && ss.resolved_count >= self.rules.len()
                        {
                            break 'walk;
                        }
                    }
                } else {
                    // Non-leaf: materialize for children.
                    let changed = step.apply(texts[parent_aidx].as_ref(), parent_density);
                    let (child_aidx, child_vi) = match changed {
                        Some((s, child_density)) => {
                            let idx = texts.len();
                            density_flags.push(child_density);
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
                            non_ascii_density: density_flags[child_aidx],
                        };
                        stopped = self.scan_variant(texts[child_aidx].as_ref(), ctx, &mut ss);
                        if stopped {
                            break 'walk;
                        }
                        if !exit_early
                            && !self.rules.has_not_rules()
                            && ss.resolved_count >= self.rules.len()
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
            self.rules.collect_matches(&ss, results);
        }
        self.rules.has_match(&ss)
    }
}
