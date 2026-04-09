//! Hot-path scan and rule evaluation for [`super::SimpleMatcher`].
//!
//! This module implements the runtime half of the two-pass matching pipeline.
//! Given a compiled [`SimpleMatcher`], it:
//!
//! 1. Obtains a `&mut` reference to the thread-local [`SIMPLE_MATCH_STATE`].
//! 2. Walks the process-type tree, transforming and scanning each variant
//!    immediately.
//! 3. Dispatches each raw match value into the rule state machine
//!    ([`RuleSet::process_entry`](super::rule::RuleSet::process_entry)).
//! 4. Collects or checks results depending on the caller (`is_match` vs
//!    `process`).
//!
//! # Unified tree walk
//!
//! The general path uses [`walk_and_scan`](SimpleMatcher::walk_and_scan), which
//! walks the process-type trie once, scanning each variant as soon as it is
//! produced. Leaf nodes that are no-ops on ASCII input reuse the parent text;
//! otherwise they materialize via `TransformStep::apply`. Non-leaf nodes
//! materialize their output for children. An `exit_early` flag controls whether
//! the walk stops on the first satisfied rule (`is_match`) or exhausts all
//! variants (`process`).
//!
//! # Safety
//!
//! All functions in this module obtain `&mut SimpleMatchState` from
//! [`SIMPLE_MATCH_STATE`] via `UnsafeCell::get()`. This is safe because the
//! static is `#[thread_local]` (no cross-thread sharing) and the functions are
//! not re-entrant. See [`SIMPLE_MATCH_STATE`] for the full safety argument.

use std::{borrow::Cow, vec};

use super::{
    SimpleMatcher, SimpleResult,
    build::{BOUNDARY_LEFT, BOUNDARY_RIGHT},
    encoding::{DIRECT_RULE_BIT, decode_direct},
    pattern::PatternDispatch,
    scan::{CHARWISE_DENSITY_THRESHOLD, text_non_ascii_density},
    state::{SIMPLE_MATCH_STATE, ScanContext, ScanState},
    tree::ProcessTypeBitNode,
};

/// Lookup table: entry is non-zero iff the byte is a word character
/// (alphanumeric, underscore, or non-ASCII ≥ 0x80). Replaces per-byte
/// multi-branch checks with a single indexed load.
static WORD_BYTE_LUT: [u8; 256] = {
    let mut lut = [0u8; 256];
    let mut i = 0u16;
    while i < 256 {
        let b = i as u8;
        lut[i as usize] = if b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80 {
            1
        } else {
            0
        };
        i += 1;
    }
    lut
};

/// Checks whether word boundaries are satisfied at the given match position.
///
/// # Safety (internal)
///
/// Uses `get_unchecked` after explicit bounds guards: `start > 0` ensures
/// `start - 1` and `start` are valid; `end < text.len()` ensures `end - 1`
/// and `end` are valid (match spans are always non-empty, so `end >= 1`).
#[inline(always)]
fn check_word_boundary(text: &[u8], start: usize, end: usize, flags: u8) -> bool {
    if flags & BOUNDARY_LEFT != 0 && start > 0 {
        // SAFETY: `start > 0` guarantees `start - 1` in bounds;
        // `start <= end <= text.len()` guarantees `start` in bounds.
        let prev = unsafe { *text.get_unchecked(start - 1) };
        // SAFETY: same guard — `start` is at most `text.len() - 1`.
        let curr = unsafe { *text.get_unchecked(start) };
        if WORD_BYTE_LUT[prev as usize] != 0 && WORD_BYTE_LUT[curr as usize] != 0 {
            return false;
        }
    }
    if flags & BOUNDARY_RIGHT != 0 && end < text.len() {
        // SAFETY: `end >= 1` (non-empty match) guarantees `end - 1` in bounds.
        let prev = unsafe { *text.get_unchecked(end - 1) };
        // SAFETY: `end < text.len()` guarantees `end` in bounds.
        let next = unsafe { *text.get_unchecked(end) };
        if WORD_BYTE_LUT[prev as usize] != 0 && WORD_BYTE_LUT[next as usize] != 0 {
            return false;
        }
    }
    true
}

/// Recursively folds no-op children's `pt_index_mask` into the parent's mask.
///
/// When the parent text is pure ASCII, certain transforms (VariantNorm,
/// Romanize, RomanizeChar, EmojiNorm) are guaranteed no-ops — the child's text
/// is identical to the parent's. Scanning that text again with a different mask
/// wastes an entire DFA traversal. By folding the child's mask into the
/// parent's scan, we eliminate redundant scans while preserving correctness:
///
/// - Each `PatternEntry` has a fixed `pt_index` → hits pass exactly one mask
///   branch.
/// - `mark_positive` / `satisfied_mask |= bit` are idempotent (bitmask path).
/// - Matrix path uses the same `text_index` (parent_vi) → same column, same
///   counters.
/// - The AC engine reports each position exactly once per scan → no
///   double-counting.
fn fold_noop_children_masks(
    tree: &[ProcessTypeBitNode],
    node_idx: usize,
    parent_ascii: bool,
) -> u64 {
    let mut mask = tree[node_idx].pt_index_mask;
    if !parent_ascii {
        return mask;
    }
    for &ci in &tree[node_idx].children {
        let child = &tree[ci];
        if child.pt_index_mask != 0 && child.step.is_some_and(|s| s.is_noop_on_ascii_input()) {
            // Recurse: a no-op non-leaf may itself have no-op children whose
            // masks should also fold up to the same scan point.
            mask |= fold_noop_children_masks(tree, ci, true);
        }
    }
    mask
}

/// Hot-path search helpers layered on top of the compiled scan engines.
impl SimpleMatcher {
    /// Scans one processed text variant and forwards each raw hit into rule
    /// evaluation via [`process_match`](Self::process_match).
    ///
    /// Returns `true` when the caller should stop scanning (i.e., `exit_early`
    /// is set and at least one rule was satisfied).
    #[inline(always)]
    fn scan_variant(&self, processed_text: &str, ctx: ScanContext, ss: &mut ScanState<'_>) -> bool {
        let text_bytes = processed_text.as_bytes();
        self.scan.for_each_match_value(
            processed_text,
            ctx.non_ascii_density,
            |raw_value, start, end| self.process_match(raw_value, text_bytes, start, end, ctx, ss),
        )
    }

    /// Processes one raw match value reported by the scan engine.
    ///
    /// Two dispatch paths:
    /// - **Direct** (`DIRECT_RULE_BIT` set): bit-packed value decoded via
    ///   [`DirectValue::decode`] into `(rule_idx, kind, offset)`, then
    ///   forwarded to [`RuleSet::eval_hit`].
    /// - **Indirect**: delegates to [`PatternIndex::dispatch_indirect`] for
    ///   multi-entry or matrix patterns, then forwards each entry to
    ///   [`RuleSet::eval_hit`].
    ///
    /// Returns `true` when the caller should stop scanning.
    #[inline(always)]
    fn process_match(
        &self,
        raw_value: u32,
        text: &[u8],
        start: usize,
        end: usize,
        ctx: ScanContext,
        ss: &mut ScanState<'_>,
    ) -> bool {
        if raw_value & DIRECT_RULE_BIT != 0 {
            let (pt_index, boundary, kind, offset, rule_idx) = decode_direct(raw_value);
            if ctx.process_type_mask & (1u64 << pt_index) == 0 {
                return false;
            }
            if boundary != 0 && !check_word_boundary(text, start, end, boundary) {
                return false;
            }
            return self.rules.eval_hit(rule_idx, kind, offset, ctx, ss);
        }
        match self.scan.patterns().dispatch_indirect(raw_value) {
            PatternDispatch::SingleEntry(entry) => {
                if entry.boundary != 0 && !check_word_boundary(text, start, end, entry.boundary) {
                    return false;
                }
                if ctx.process_type_mask & (1u64 << entry.pt_index) == 0 {
                    return false;
                }
                self.rules.eval_hit(
                    entry.rule_idx as usize,
                    entry.kind,
                    entry.offset as usize,
                    ctx,
                    ss,
                )
            }
            PatternDispatch::Entries(entries) => {
                for entry in entries {
                    if entry.boundary != 0 && !check_word_boundary(text, start, end, entry.boundary)
                    {
                        continue;
                    }
                    if ctx.process_type_mask & (1u64 << entry.pt_index) == 0 {
                        continue;
                    }
                    if self.rules.eval_hit(
                        entry.rule_idx as usize,
                        entry.kind,
                        entry.offset as usize,
                        ctx,
                        ss,
                    ) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Unified tree walk that transforms, scans, and evaluates rules in a
    /// single pass. Delegates to
    /// [`walk_and_scan_with`](Self::walk_and_scan_with) with a Vec-based
    /// collector.
    #[inline]
    pub(super) fn walk_and_scan<'a>(
        &'a self,
        text: &'a str,
        exit_early: bool,
        results: Option<&mut Vec<SimpleResult<'a>>>,
    ) -> bool {
        self.walk_and_scan_with(text, exit_early, |rules, ss| {
            if let Some(results) = results {
                rules.collect_matches(ss, results);
            }
        })
        .0
    }

    /// Generalized tree walk: scans all variants, then calls `collect` inside
    /// the TLS scope to harvest results.
    ///
    /// Returns `(has_match, collect_result)`. The `collect` closure receives
    /// the `RuleSet` and `ScanState` while TLS is still held, so it can
    /// read touched indices and rule satisfaction without copying state out.
    ///
    /// `collect` is wrapped in `Option` internally because `FnOnce` can only
    /// fire once yet there are three potential collection sites (two early-out
    /// + one post-scan).
    ///
    /// # Panics
    ///
    /// Panics if a non-root node in the transform trie lacks a cached
    /// [`TransformStep`](crate::process::step::TransformStep). This is a
    /// construction invariant maintained by
    /// [`build_process_type_tree`](super::tree::build_process_type_tree).
    #[inline]
    pub(super) fn walk_and_scan_with<'a, F, R>(
        &'a self,
        text: &'a str,
        exit_early: bool,
        collect: F,
    ) -> (bool, Option<R>)
    where
        F: FnOnce(&'a super::rule::RuleSet, &ScanState<'_>) -> R,
    {
        let tree = &self.tree;
        let num_variants = tree.len();
        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());
        let mut ss = state.as_scan_state();

        let mut collect = Some(collect);

        // One SIMD pass: exact non-ASCII byte density for engine dispatch.
        // density == 0.0 ↔ text is pure ASCII (replaces text.is_ascii()).
        let root_density = text_non_ascii_density(text);

        // Fold no-op children's masks into the root scan to eliminate redundant
        // DFA traversals. On ASCII text, transforms like VariantNorm/Romanize
        // produce identical text — scanning it again with a different mask is
        // pure waste. Folding merges those masks into one scan.
        let root_scan_mask = fold_noop_children_masks(tree, 0, root_density == 0.0);

        // Scan root text if any PT terminates here (including folded no-ops).
        if root_scan_mask != 0 {
            let ctx = ScanContext {
                text_index: 0,
                process_type_mask: root_scan_mask,
                num_variants,
                exit_early,
                non_ascii_density: root_density,
            };
            if self.scan_variant(text, ctx, &mut ss) {
                return (true, None);
            }
        }

        if tree[0].children.is_empty() {
            let r = collect.take().map(|f| f(&self.rules, &ss));
            return (self.rules.has_match(&ss), r);
        }

        // Arena for materialized non-leaf texts. Index 0 = root (borrowed).
        let mut texts: Vec<Cow<'_, str>> = Vec::with_capacity(num_variants);
        texts.push(Cow::Borrowed(text));
        // density_flags[i] — non-ASCII byte density for arena index i.
        // density == 0.0 means pure ASCII (used for both engine dispatch and
        // transform correctness: is_noop_on_ascii_input, step.apply).
        let mut density_flags: Vec<f32> = Vec::new();
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
                // Invariant: non-root tree nodes always cache a transform step.
                let Some(step) = child.step else {
                    unreachable!()
                };
                let is_leaf = child.children.is_empty();
                let parent_density = density_flags[parent_aidx];
                let parent_ascii = parent_density == 0.0;

                let is_noop = parent_ascii && step.is_noop_on_ascii_input();

                if is_leaf {
                    if child.pt_index_mask != 0 {
                        // No-op leaves were already folded into the parent's scan
                        // mask by fold_noop_children_masks — skip entirely.
                        if is_noop {
                            continue;
                        }

                        // Fused transform-scan dispatch:
                        //
                        // - DFA available + low density: skip fused, fall through to materialize
                        //   path — DFA+Teddy is 2–5× faster than DAAC bytewise streaming on
                        //   ASCII-heavy text.
                        // - No DFA + low density: stream via DAAC bytewise.
                        // - High density: stream via DAAC charwise.
                        //
                        // Fused paths cover Delete/Normalize/VariantNorm/Romanize.
                        // Parent density is the correct estimate for all fused transforms.
                        // Note: is_noop leaves are already skipped above.
                        let use_fused =
                            !(self.scan.has_dfa() && parent_density <= CHARWISE_DENSITY_THRESHOLD);
                        let fused_result = if use_fused {
                            let parent_text = texts[parent_aidx].as_ref();
                            step.filter_bytes(parent_text).map(|iter| {
                                let vi = variant_counter;
                                let ctx = ScanContext {
                                    text_index: vi,
                                    process_type_mask: child.pt_index_mask,
                                    num_variants,
                                    exit_early,
                                    non_ascii_density: parent_density,
                                };
                                let fused_text_bytes = parent_text.as_bytes();
                                variant_counter += 1;
                                self.scan.for_each_match_value_from_iter(
                                    iter,
                                    ctx.non_ascii_density,
                                    |v, start, end| {
                                        self.process_match(
                                            v,
                                            fused_text_bytes,
                                            start,
                                            end,
                                            ctx,
                                            &mut ss,
                                        )
                                    },
                                )
                            })
                        } else {
                            None
                        };

                        stopped = if let Some(result) = fused_result {
                            result
                        } else {
                            // Normal path: materialize then scan.
                            // Note: is_noop leaves are skipped above, so apply()
                            // always runs here.
                            let changed = step.apply(texts[parent_aidx].as_ref(), parent_density);

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
                                self.scan_variant(&s, ctx, &mut ss)
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

                    // Scan if this node terminates. No-op non-leaves were
                    // already folded into the parent's scan — skip.
                    // Also fold this node's own no-op children into its mask.
                    if child.pt_index_mask != 0 && !is_noop {
                        let child_ascii = density_flags[child_aidx] == 0.0;
                        let scan_mask = fold_noop_children_masks(tree, child_idx, child_ascii);
                        let ctx = ScanContext {
                            text_index: child_vi,
                            process_type_mask: scan_mask,
                            num_variants,
                            exit_early,
                            non_ascii_density: density_flags[child_aidx],
                        };
                        stopped = self.scan_variant(texts[child_aidx].as_ref(), ctx, &mut ss);
                        if stopped {
                            break 'walk;
                        }
                    }
                }
            }
        }

        if stopped {
            return (true, None);
        }

        let r = collect.take().map(|f| f(&self.rules, &ss));
        (self.rules.has_match(&ss), r)
    }
}
