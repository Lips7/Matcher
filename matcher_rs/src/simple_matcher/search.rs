//! Hot-path scan and rule evaluation for [`super::SimpleMatcher`].
//!
//! This module implements the runtime half of the two-pass matching pipeline.
//! Given a compiled [`SimpleMatcher`], it:
//!
//! 1. Obtains a `&mut` reference to the thread-local [`SIMPLE_MATCH_STATE`].
//! 2. Walks the process-type tree, transforming and scanning each variant
//!    immediately.
//! 3. Dispatches each raw match value into the rule state machine
//!    (`RuleSet::eval_hit`).
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
    pattern::{DIRECT_ENCODED_BIT, PatternDispatch, decode_direct},
    rule::RuleSet,
    scan::{CHARWISE_DENSITY_THRESHOLD, text_char_density},
    state::{SIMPLE_MATCH_STATE, ScanContext, ScanState, WalkConfig},
    tree::ProcessTypeBitNode,
};
use crate::process::step::TransformStep;

/// Parent node state passed to leaf and non-leaf child handlers.
///
/// Bundles the materialized text, variant index, density estimate, and root
/// flag for the parent node being expanded during the tree walk.
#[derive(Clone, Copy)]
struct ParentNode<'a> {
    text: &'a str,
    variant_index: usize,
    density: f32,
    is_root: bool,
}

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
/// The `if` guards (`start > 0`, `end < text.len()`) prove the array indices
/// are in bounds. `assert_unchecked` communicates this to the optimizer so
/// the subsequent plain indexing compiles without bounds checks.
#[inline(always)]
fn check_word_boundary(text: &[u8], start: usize, end: usize, flags: u8) -> bool {
    if flags & BOUNDARY_LEFT != 0 && start > 0 {
        // SAFETY: `start > 0` guard above; `start` is a valid match offset within
        // `text`.
        unsafe { core::hint::assert_unchecked(start < text.len()) };
        let prev = text[start - 1];
        let curr = text[start];
        if WORD_BYTE_LUT[prev as usize] != 0 && WORD_BYTE_LUT[curr as usize] != 0 {
            return false;
        }
    }
    if flags & BOUNDARY_RIGHT != 0 && end < text.len() {
        // SAFETY: `end < text.len()` guard above; match end is always >= 1 (non-empty
        // pattern).
        unsafe { core::hint::assert_unchecked(end >= 1) };
        let prev = text[end - 1];
        let next = text[end];
        if WORD_BYTE_LUT[prev as usize] != 0 && WORD_BYTE_LUT[next as usize] != 0 {
            return false;
        }
    }
    true
}

/// Recursively folds no-op children's `process_type_index_mask` into the
/// parent's mask.
///
/// When the parent text is pure ASCII, certain transforms (VariantNorm,
/// Romanize, RomanizeChar, EmojiNorm) are guaranteed no-ops — the child's text
/// is identical to the parent's. Scanning that text again with a different mask
/// wastes an entire DFA traversal. By folding the child's mask into the
/// parent's scan, we eliminate redundant scans while preserving correctness:
///
/// - Each `PatternEntry` has a fixed `process_type_index` → hits pass exactly
///   one mask branch.
/// - `mark_positive` / `satisfied_mask |= bit` are idempotent (bitmask path).
/// - Matrix path uses the same `text_index` (parent_variant) → same column,
///   same counters.
/// - The AC engine reports each position exactly once per scan → no
///   double-counting.
fn fold_noop_children_masks(
    tree: &[ProcessTypeBitNode],
    node_idx: usize,
    parent_ascii: bool,
) -> u64 {
    let mut mask = tree[node_idx].process_type_index_mask;
    if !parent_ascii {
        return mask;
    }
    for &child_node_idx in &tree[node_idx].children {
        let child = &tree[child_node_idx];
        if child.process_type_index_mask != 0
            && child.step.is_some_and(|s| s.is_noop_on_ascii_input())
        {
            mask |= fold_noop_children_masks(tree, child_node_idx, true);
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
        self.scan
            .for_each_match_value(processed_text, ctx.char_density, |raw_value, start, end| {
                self.process_match(raw_value, text_bytes, start, end, ctx, ss)
            })
    }

    /// Processes one raw match value reported by the scan engine.
    ///
    /// Two dispatch paths:
    /// - **Direct** (`DIRECT_ENCODED_BIT` set): bit-packed value decoded via
    ///   `decode_direct` into `(rule_idx, kind, offset)`, then forwarded to
    ///   `RuleSet::eval_hit`.
    /// - **Indirect**: delegates to `PatternIndex::dispatch_indirect` for
    ///   multi-entry or matrix patterns, then forwards each entry to
    ///   `RuleSet::eval_hit`.
    ///
    /// Returns `true` when the caller should stop scanning.
    #[cfg_attr(feature = "_profile_boundaries", inline(never))]
    #[cfg_attr(not(feature = "_profile_boundaries"), inline(always))]
    fn process_match(
        &self,
        raw_value: u32,
        text: &[u8],
        start: usize,
        end: usize,
        ctx: ScanContext,
        ss: &mut ScanState<'_>,
    ) -> bool {
        if raw_value & DIRECT_ENCODED_BIT != 0 {
            let (process_type_index, boundary, kind, offset, rule_idx) = decode_direct(raw_value);
            if ctx.process_type_mask & (1u64 << process_type_index) == 0 {
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
                if ctx.process_type_mask & (1u64 << entry.process_type_index) == 0 {
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
                    if ctx.process_type_mask & (1u64 << entry.process_type_index) == 0 {
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

    /// Scans a leaf child node, choosing between delete dual-scan, fused
    /// streaming, or materialize-then-scan based on the transform type and
    /// engine capabilities.
    ///
    /// Called only for non-noop leaves with a non-zero
    /// `process_type_index_mask`. Returns `true` when the caller should
    /// stop scanning.
    #[inline(always)]
    fn scan_leaf_child(
        &self,
        step: &'static TransformStep,
        parent: ParentNode<'_>,
        child_mask: u64,
        walk: WalkConfig,
        variant_counter: &mut usize,
        ss: &mut ScanState<'_>,
    ) -> bool {
        // ── Delete dual-scan (root child only) ───────────────────────
        // Delete is the only non-bijective transform — patterns are stored
        // verbatim and may contain deletable characters. When Delete is a
        // direct root child, scan both deleted and original text. Non-root
        // parents already scan pre-Delete text as intermediates.
        if parent.is_root && step.is_non_bijective() {
            let changed = step.apply(parent.text, parent.density);
            return if let Some((deleted, child_density)) = changed {
                let new_variant = *variant_counter;
                *variant_counter += 1;
                let del_ctx = walk.scan_ctx(new_variant, child_mask, child_density);
                if self.scan_variant(&deleted, del_ctx, ss) {
                    return true;
                }
                let orig_ctx = walk.scan_ctx(parent.variant_index, child_mask, parent.density);
                self.scan_variant(parent.text, orig_ctx, ss)
            } else {
                let ctx = walk.scan_ctx(parent.variant_index, child_mask, parent.density);
                self.scan_variant(parent.text, ctx, ss)
            };
        }

        // ── Fused streaming or materialize-then-scan ─────────────────
        // Fused streaming pipes the transform's byte iterator directly into
        // the AC engine, avoiding full materialization. Disabled when the DFA
        // has a Teddy prefilter — Teddy's SIMD skip outperforms streaming.
        let use_fused = !(cfg!(feature = "dfa")
            && parent.density >= CHARWISE_DENSITY_THRESHOLD
            && self.scan.has_dfa_prefilter());

        if use_fused && let Some(iter) = step.filter_bytes(parent.text) {
            let new_variant = *variant_counter;
            *variant_counter += 1;
            let ctx = walk.scan_ctx(new_variant, child_mask, parent.density);
            let fused_text_bytes = parent.text.as_bytes();
            return self.scan.for_each_match_value_from_iter(
                iter,
                ctx.char_density,
                |v, start, end| self.process_match(v, fused_text_bytes, start, end, ctx, ss),
            );
        }

        // ── Materialize path ─────────────────────────────────────────
        // Apply transform, then scan the result.
        let changed = step.apply(parent.text, parent.density);
        if let Some((s, child_density)) = changed {
            let new_variant = *variant_counter;
            *variant_counter += 1;
            let ctx = walk.scan_ctx(new_variant, child_mask, child_density);
            self.scan_variant(&s, ctx, ss)
        } else {
            let ctx = walk.scan_ctx(parent.variant_index, child_mask, parent.density);
            self.scan_variant(parent.text, ctx, ss)
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
    /// # Decision path per child node
    ///
    /// ```text
    /// child node
    /// │
    /// ├─ build-time no-op? (is_noop_on_ascii_input && parent is ASCII)
    /// │  └─ YES → folded into parent scan (fold_noop_children_masks) → SKIP
    /// │
    /// ├─ step.apply returns None (runtime no-op, text unchanged)
    /// │  └─ scan parent text (1 scan — only scan for this PT)
    /// │
    /// └─ step.apply returns Some (text changed)
    ///    │
    ///    ├─ bijective transform (VariantNorm / Normalize / Romanize / …)
    ///    │  └─ scan changed text (1 scan)
    ///    │
    ///    └─ Delete (non-bijective, patterns stored verbatim)
    ///       │
    ///       ├─ parent is NOT root (e.g. VariantNorm → Delete)
    ///       │  └─ scan deleted text (1 scan; parent already scanned
    ///       │     pre-Delete text as an intermediate node)
    ///       │
    ///       └─ parent IS root
    ///          └─ scan deleted text + scan original text (2 scans;
    ///             root has no mask, so original needs explicit scan
    ///             for patterns containing deletable characters)
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if a non-root node in the transform trie lacks a cached
    /// [`TransformStep`]. This is a
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
        F: FnOnce(&'a RuleSet, &ScanState<'_>) -> R,
    {
        let tree = &self.tree;
        let walk = WalkConfig {
            num_variants: tree.len(),
            exit_early,
        };
        // SAFETY: `#[thread_local]` guarantees single-thread ownership; not re-entrant.
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());
        let mut ss = state.as_scan_state();

        let mut collect = Some(collect);

        // One SIMD pass: character density for engine dispatch.
        // density >= 1.0 ↔ text is pure ASCII.
        let root_density = text_char_density(text);

        // Fold no-op children's masks into the root scan to eliminate redundant
        // DFA traversals. On ASCII text, transforms like VariantNorm/Romanize
        // produce identical text — scanning it again with a different mask is
        // pure waste. Folding merges those masks into one scan.
        let root_scan_mask = fold_noop_children_masks(tree, 0, root_density >= 1.0);

        // Scan root text if any PT terminates here (including folded no-ops).
        if root_scan_mask != 0 {
            let ctx = walk.scan_ctx(0, root_scan_mask, root_density);
            if self.scan_variant(text, ctx, &mut ss) {
                return (true, None);
            }
        }

        if tree[0].children.is_empty() {
            let r = collect.take().map(|f| f(&self.rules, &ss));
            return (ss.has_match(), r);
        }

        // Arena for materialized non-leaf texts. Index 0 = root (borrowed).
        let mut texts: Vec<Cow<'_, str>> = Vec::with_capacity(walk.num_variants);
        texts.push(Cow::Borrowed(text));
        // density_flags[i] — character density for arena index i.
        let mut density_flags: Vec<f32> = Vec::new();
        density_flags.push(root_density);

        // Maps tree node index -> arena index for its text.
        let mut node_arena: Vec<usize> = vec![0; walk.num_variants];
        // Maps tree node index -> variant index used in ScanContext::text_index.
        let mut node_variant: Vec<usize> = vec![0; walk.num_variants];
        let mut variant_counter = 1usize;
        let mut stopped = false;

        'walk: for node_idx in 0..tree.len() {
            let num_children = tree[node_idx].children.len();
            if num_children == 0 {
                continue;
            }
            let parent_arena_idx = node_arena[node_idx];
            let parent_variant = node_variant[node_idx];
            let parent_density = density_flags[parent_arena_idx];
            let is_root = node_idx == 0;

            for child_pos in 0..num_children {
                let child_idx = tree[node_idx].children[child_pos];
                let child = &tree[child_idx];
                let step = child
                    .step
                    .expect("non-root node must have cached TransformStep");
                let is_noop = parent_density >= 1.0 && step.is_noop_on_ascii_input();

                // ── Leaf node ────────────────────────────────────
                if child.children.is_empty() {
                    if child.process_type_index_mask == 0 || is_noop {
                        continue;
                    }
                    let parent = ParentNode {
                        text: texts[parent_arena_idx].as_ref(),
                        variant_index: parent_variant,
                        density: parent_density,
                        is_root,
                    };
                    stopped = self.scan_leaf_child(
                        step,
                        parent,
                        child.process_type_index_mask,
                        walk,
                        &mut variant_counter,
                        &mut ss,
                    );
                    if stopped {
                        break 'walk;
                    }
                    continue;
                }

                // ── Non-leaf node: materialize for children ──────
                let changed = step.apply(texts[parent_arena_idx].as_ref(), parent_density);
                let (child_arena_idx, child_variant) = match changed {
                    Some((s, child_density)) => {
                        let idx = texts.len();
                        density_flags.push(child_density);
                        texts.push(Cow::Owned(s));
                        let new_variant = variant_counter;
                        variant_counter += 1;
                        (idx, new_variant)
                    }
                    None => (parent_arena_idx, parent_variant),
                };
                node_arena[child_idx] = child_arena_idx;
                node_variant[child_idx] = child_variant;

                // Scan if this node terminates a process type. No-op non-leaves
                // were already folded into the parent scan — skip. Also fold
                // this node's own no-op children into its mask.
                if child.process_type_index_mask != 0 && !is_noop {
                    let child_ascii = density_flags[child_arena_idx] >= 1.0;
                    let scan_mask = fold_noop_children_masks(tree, child_idx, child_ascii);
                    let ctx =
                        walk.scan_ctx(child_variant, scan_mask, density_flags[child_arena_idx]);
                    stopped = self.scan_variant(texts[child_arena_idx].as_ref(), ctx, &mut ss);
                    if stopped {
                        break 'walk;
                    }

                    // Non-bijective dual-scan: scan original text when Delete
                    // changed it and parent is root.
                    if is_root && step.is_non_bijective() && child_arena_idx != parent_arena_idx {
                        let orig_ctx = walk.scan_ctx(parent_variant, scan_mask, parent_density);
                        stopped =
                            self.scan_variant(texts[parent_arena_idx].as_ref(), orig_ctx, &mut ss);
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
        (ss.has_match(), r)
    }
}
