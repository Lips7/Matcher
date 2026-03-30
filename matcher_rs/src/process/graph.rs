//! Transformation graph construction and traversal.
//!
//! The "graph" is a flat-array trie where each node represents one single-bit
//! [`ProcessType`] step. [`build_process_type_tree`] constructs the trie from a set of
//! composite bitmasks, merging shared prefixes so that intermediate results are computed
//! once. [`walk_process_tree`] then traverses the array in parent-before-child order,
//! applying each transformation step exactly once per reachable prefix and collecting
//! the resulting [`TextVariant`]s.
//!
//! # Example trie
//!
//! Given the set `{Fanjian, Fanjian|Delete, Fanjian|Delete|Normalize}`, the trie is:
//!
//! ```text
//!   [0] root (None)
//!    └─[1] Fanjian          ← terminates: {Fanjian}
//!       └─[2] Delete        ← terminates: {Fanjian|Delete}
//!          └─[3] Normalize  ← terminates: {Fanjian|Delete|Normalize}
//! ```
//!
//! Walking this trie for input `"妳！好Ａ"` computes Fanjian once, then Delete on the
//! Fanjian output, then Normalize on the Delete output — reusing each intermediate.

use std::borrow::Cow;
use std::collections::HashSet;

use tinyvec::TinyVec;

use crate::process::process_type::ProcessType;
use crate::process::registry::get_transform_step;
use crate::process::step::TransformStep;
use crate::process::variant::{
    ProcessedTextMasks, TRANSFORM_STATE, TextVariant, return_string_to_pool,
};

/// A node in the flat-array transformation trie used by [`walk_process_tree`].
///
/// The trie is built once by [`build_process_type_tree`] and stored inside
/// [`SimpleMatcher`](crate::SimpleMatcher). Each node represents a single transformation
/// step (`process_type_bit`) reachable from its parent. Nodes form a tree:
///
/// - **Root** (index 0) always has `process_type_bit = ProcessType::None` and no `step`.
/// - **Inner / leaf nodes** each carry a cached `&'static TransformStep` so the hot
///   traversal loop avoids a registry lookup.
/// - **`process_type_list`** records which composite [`ProcessType`] values terminate at
///   this node, so the traversal can tag output text variants with the correct bitmask.
/// - **`pt_index_mask`** is the pre-computed OR of `1u64 << pt.bits()` for all entries in
///   `process_type_list`, avoiding a per-call fold in the hot loop.
///
/// All fields are `pub(crate)` or private; users obtain instances exclusively through
/// [`build_process_type_tree`].
///
/// # Examples
///
/// ```rust
/// use std::collections::HashSet;
/// use matcher_rs::{ProcessType, build_process_type_tree};
///
/// let types = HashSet::from([ProcessType::None, ProcessType::Fanjian]);
/// let tree = build_process_type_tree(&types);
///
/// // Root node is always at index 0; at least one child for Fanjian.
/// assert!(tree.len() >= 2);
/// ```
#[derive(Clone)]
pub struct ProcessTypeBitNode {
    /// Composite [`ProcessType`] values whose decomposed bit-path terminates at this node.
    ///
    /// A non-empty list means that one or more rules emit a text variant here. For example,
    /// if both `Fanjian` and `Fanjian|Delete` are in the set, the Fanjian node's list
    /// contains `Fanjian`, and the Delete child's list contains `Fanjian|Delete`.
    process_type_list: TinyVec<[ProcessType; 4]>,
    /// The single-bit [`ProcessType`] step this node represents (the edge label from parent).
    ///
    /// For the root node this is [`ProcessType::None`].
    pub(crate) process_type_bit: ProcessType,
    /// Flat-array indices of child nodes (the next transformation steps reachable from here).
    pub(crate) children: TinyVec<[usize; 4]>,
    /// Cached reference to the compiled transform step for this node's `process_type_bit`.
    ///
    /// [`None`] only for the root node. All other nodes cache their step at construction
    /// time to avoid a registry lookup in the hot [`walk_process_tree`] loop.
    pub(crate) step: Option<&'static TransformStep>,
    /// Pre-computed OR of `1u64 << pt.bits()` for every `pt` in `process_type_list`.
    ///
    /// Avoids a per-traversal fold in the hot [`walk_process_tree`] loop. After
    /// [`recompute_mask_with_index`](Self::recompute_mask_with_index) is called, the
    /// encoding switches from raw bit positions to sequential indices.
    pub(crate) pt_index_mask: u64,
}

/// Post-construction helpers for [`ProcessTypeBitNode`].
impl ProcessTypeBitNode {
    /// Re-encodes [`pt_index_mask`](Self::pt_index_mask) using a sequential index table.
    ///
    /// The default encoding stores `1u64 << pt.bits()`, which can scatter bits across the
    /// full `u64` range for composite [`ProcessType`] values. A sequential index keeps bit
    /// positions small (`0..N` where `N` is the number of unique composite types) so that
    /// downstream data structures (e.g. `PatternEntry`) can store the index as a `u8`
    /// rather than a `u64`, halving entry size.
    ///
    /// `pt_index_table[pt.bits()]` must contain the sequential index for every composite
    /// [`ProcessType`] that terminates at any node (i.e. every type in the original
    /// `process_type_set` plus [`ProcessType::None`]).
    ///
    /// Called by [`SimpleMatcher::new()`](crate::SimpleMatcher) after
    /// [`build_process_type_tree`] returns.
    pub(crate) fn recompute_mask_with_index(&mut self, pt_index_table: &[u8; 64]) {
        self.pt_index_mask = self.process_type_list.iter().fold(0u64, |acc, pt| {
            acc | (1u64 << pt_index_table[pt.bits() as usize])
        });
    }
}

/// Builds a flat-array trie from a set of composite [`ProcessType`] bitmasks.
///
/// The trie encodes every unique prefix path among the given composite types. A root node
/// with `process_type_bit = ProcessType::None` is always present at index 0. For each
/// composite type (e.g. `Fanjian | Delete`), the constructor walks its constituent bits in
/// ascending order, reusing existing child nodes where the path overlaps with previously
/// inserted types and creating new child nodes when a path diverges.
///
/// The resulting flat `Vec<ProcessTypeBitNode>` is passed to [`walk_process_tree`], which
/// scans the node array in parent-before-child order to compute all required text variants
/// while reusing common prefixes.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashSet;
/// use matcher_rs::{ProcessType, build_process_type_tree};
///
/// let types = HashSet::from([
///     ProcessType::None,
///     ProcessType::Fanjian,
///     ProcessType::Fanjian | ProcessType::Delete,
/// ]);
/// let tree = build_process_type_tree(&types);
///
/// // Root (None) + Fanjian node + Delete node = at least 3 nodes.
/// assert!(tree.len() >= 3);
/// ```
pub fn build_process_type_tree(process_type_set: &HashSet<ProcessType>) -> Vec<ProcessTypeBitNode> {
    let max_nodes: usize = 1 + process_type_set
        .iter()
        .map(|pt| pt.bits().count_ones() as usize)
        .sum::<usize>();
    let mut process_type_tree = Vec::with_capacity(max_nodes);
    let mut root = ProcessTypeBitNode {
        process_type_list: TinyVec::new(),
        process_type_bit: ProcessType::None,
        children: TinyVec::new(),
        step: None,
        pt_index_mask: 0,
    };
    if process_type_set.contains(&ProcessType::None) {
        root.process_type_list.push(ProcessType::None);
        root.pt_index_mask |= 1u64 << ProcessType::None.bits();
    }
    process_type_tree.push(root);
    for &process_type in process_type_set.iter() {
        let mut current_node_index = 0;
        for process_type_bit in process_type.iter() {
            let current_node = &process_type_tree[current_node_index];
            if current_node.process_type_bit == process_type_bit {
                continue;
            }

            let found_child = current_node
                .children
                .iter()
                .find(|&&idx| process_type_tree[idx].process_type_bit == process_type_bit)
                .copied();

            if let Some(child_idx) = found_child {
                current_node_index = child_idx;
                process_type_tree[current_node_index]
                    .process_type_list
                    .push(process_type);
                process_type_tree[current_node_index].pt_index_mask |= 1u64 << process_type.bits();
            } else {
                let mut child = ProcessTypeBitNode {
                    process_type_list: TinyVec::new(),
                    process_type_bit,
                    children: TinyVec::new(),
                    step: Some(get_transform_step(process_type_bit)),
                    pt_index_mask: 0,
                };
                child.process_type_list.push(process_type);
                child.pt_index_mask |= 1u64 << process_type.bits();
                process_type_tree.push(child);
                let new_node_index = process_type_tree.len() - 1;
                process_type_tree[current_node_index]
                    .children
                    .push(new_node_index);
                current_node_index = new_node_index;
            }
        }
    }
    process_type_tree
}

/// Inserts a transformed text into `text_masks`, deduplicating against existing entries.
///
/// If `changed` is `Some(text)` and an equal string already exists in `text_masks`, the
/// duplicate is returned to the thread-local pool and the existing index is returned.
/// Otherwise the new text is appended and the new index is returned. If `changed` is
/// [`None`] (the step was a no-op), `current_index` is returned unchanged.
///
/// `is_ascii` indicates whether the transformed text consists entirely of ASCII bytes;
/// it is stored alongside the text so callers can skip the charwise automaton without a
/// redundant byte scan.
///
/// This deduplication keeps [`walk_process_tree`] in "unique string" space even when
/// different trie paths converge onto the same transformed text (e.g. when Fanjian and
/// Normalize both produce the same output for a particular input).
fn dedup_insert(
    text_masks: &mut ProcessedTextMasks<'_>,
    current_index: usize,
    changed: Option<String>,
    is_ascii: bool,
) -> usize {
    match changed {
        Some(processed) => {
            let plen = processed.len();
            if let Some(pos) = text_masks
                .iter()
                .position(|tv| tv.text.len() == plen && tv.text.as_ref() == processed.as_str())
            {
                return_string_to_pool(processed);
                pos
            } else {
                text_masks.push(TextVariant {
                    text: Cow::Owned(processed),
                    mask: 0u64,
                    is_ascii,
                });
                text_masks.len() - 1
            }
        }
        None => current_index,
    }
}

/// Walks the transformation trie, producing all text variants needed for matching.
///
/// This is the hot-path function called on every [`SimpleMatcher::is_match`](crate::SimpleMatcher::is_match) /
/// [`SimpleMatcher::process`](crate::SimpleMatcher::process) invocation. It performs one
/// forward pass over the flat tree built by [`build_process_type_tree`], relying on the
/// invariant that every parent node appears before its children (guaranteed by construction).
/// Common prefixes (for example the shared `Fanjian` step in both `Fanjian | Delete` and
/// `Fanjian | Normalize`) are computed once and their result indices are reused for child
/// nodes.
///
/// Each [`TextVariant`] in the returned [`ProcessedTextMasks`] carries the transformed
/// text, the bitmask of [`ProcessType`] indices that produced it, and an `is_ascii` flag.
/// When different trie paths converge to the same string, they share a single entry with
/// a merged mask.
///
/// # Const-generic `LAZY` parameter
///
/// - **`LAZY = false`** — Produces all variants without callbacks; `on_variant` is never
///   called. Use this when you need the complete set of variants (e.g. for
///   [`SimpleMatcher::process`](crate::SimpleMatcher::process)).
///
/// - **`LAZY = true`** — Calls `on_variant(text, index, mask, is_ascii)` as soon as each
///   new unique variant is produced. If `on_variant` returns `true`, the walk stops early.
///   A "delta phase" at the end replays any additional mask bits that were merged into an
///   already-seen text through deduplication. Use this for
///   [`SimpleMatcher::is_match`](crate::SimpleMatcher::is_match) to short-circuit as soon
///   as a rule is satisfied.
///
/// # Return value
///
/// Returns `(text_masks, stopped)` where `stopped` is `true` only when `LAZY = true` and
/// `on_variant` triggered early exit.
///
/// Inside `matcher_rs`, owned strings in the returned vector are recycled via
/// `return_processed_string_to_pool`.
/// External callers can simply drop the vector — no manual cleanup is needed.
///
/// # Safety
///
/// Accesses `TRANSFORM_STATE` through `UnsafeCell::get()`.
/// Safe because `#[thread_local]` guarantees single-threaded access and this function is
/// never called re-entrantly.
///
/// Contains a `transmute` from `ProcessedTextMasks<'static>` to `ProcessedTextMasks<'a>`
/// when recycling a pooled buffer. This is sound because the pooled buffer is always empty
/// (drained before being returned to the pool), so no `Cow` borrows with the wrong lifetime
/// exist, and `Vec` layout is lifetime-independent.
///
/// # Panics
///
/// Panics (via `.expect()`) if a non-root node has `step = None`, which indicates a
/// construction bug in [`build_process_type_tree`].
///
/// # Examples
///
/// ```rust
/// use std::collections::HashSet;
/// use matcher_rs::{ProcessType, build_process_type_tree, walk_process_tree};
///
/// let process_types = HashSet::from([
///     ProcessType::None,
///     ProcessType::Fanjian,
///     ProcessType::Delete,
///     ProcessType::Fanjian | ProcessType::Delete,
/// ]);
/// let tree = build_process_type_tree(&process_types);
///
/// // LAZY=false: produce all variants without early stopping.
/// let (variants, stopped) = walk_process_tree::<false, _>(
///     &tree, "妳！好", &mut |_, _, _, _| false,
/// );
/// assert!(!stopped);
///
/// let texts: std::collections::HashSet<_> = variants
///     .into_iter()
///     .map(|tv| tv.text.into_owned())
///     .collect();
///
/// assert_eq!(texts.len(), 4);
/// assert!(texts.contains("妳！好"));  // raw (None)
/// assert!(texts.contains("你！好"));  // Fanjian
/// assert!(texts.contains("妳好"));    // Delete
/// assert!(texts.contains("你好"));    // Fanjian | Delete
/// ```
#[inline(always)]
pub fn walk_process_tree<'a, const LAZY: bool, F>(
    process_type_tree: &[ProcessTypeBitNode],
    text: &'a str,
    on_variant: &mut F,
) -> (ProcessedTextMasks<'a>, bool)
where
    F: FnMut(&str, usize, u64, bool) -> bool,
{
    {
        // SAFETY: #[thread_local] guarantees single-threaded access.
        // walk_process_tree is never called re-entrantly.
        let ts = unsafe { &mut *TRANSFORM_STATE.get() };

        let pooled: Option<ProcessedTextMasks<'static>> = ts.masks_pool.pop();
        // Safety: pool holds empty Vecs with no live borrows; transmuting from
        // 'static to 'a is safe since 'static: 'a (covariant) and Vec is empty.
        let mut text_masks: ProcessedTextMasks<'a> =
            unsafe { std::mem::transmute(pooled.unwrap_or_default()) };
        text_masks.clear();
        let root_is_ascii = text.is_ascii();
        text_masks.push(TextVariant {
            text: Cow::Borrowed(text),
            mask: process_type_tree[0].pt_index_mask,
            is_ascii: root_is_ascii,
        });

        let mut scanned_masks: TinyVec<[u64; 8]> = TinyVec::new();
        if LAZY {
            scanned_masks.push(0u64);
            let root_mask = process_type_tree[0].pt_index_mask;
            if root_mask != 0 && on_variant(text, 0, root_mask, root_is_ascii) {
                return (text_masks, true);
            }
            scanned_masks[0] = root_mask;
        }

        if process_type_tree[0].children.is_empty() {
            return (text_masks, false);
        }

        ts.tree_node_indices.clear();
        ts.tree_node_indices.resize(process_type_tree.len(), 0);

        let mut stopped = false;

        'walk: for (current_node_index, current_node) in process_type_tree.iter().enumerate() {
            let current_index = ts.tree_node_indices[current_node_index];
            let parent_is_ascii = text_masks[current_index].is_ascii;

            for &child_node_index in &current_node.children {
                let child_node = &process_type_tree[child_node_index];
                let step = child_node
                    .step
                    .expect("non-root process tree nodes always cache a transform step");
                let current_text = text_masks[current_index].text.as_ref();
                let output = step.apply(current_text, parent_is_ascii);

                let old_len = if LAZY { text_masks.len() } else { 0 };
                let child_index = dedup_insert(
                    &mut text_masks,
                    current_index,
                    output.changed,
                    output.is_ascii,
                );

                if LAZY {
                    while scanned_masks.len() < text_masks.len() {
                        scanned_masks.push(0u64);
                    }
                }

                ts.tree_node_indices[child_node_index] = child_index;
                text_masks[child_index].mask |= child_node.pt_index_mask;

                if LAZY && child_index >= old_len {
                    // New unique text: call on_variant immediately.
                    let mask = text_masks[child_index].mask;
                    let is_ascii = text_masks[child_index].is_ascii;
                    if mask != 0
                        && on_variant(
                            text_masks[child_index].text.as_ref(),
                            child_index,
                            mask,
                            is_ascii,
                        )
                    {
                        stopped = true;
                        break 'walk;
                    }
                    scanned_masks[child_index] = mask;
                }
                // Dedup'd entry: mask may have grown; handled in delta phase below.
            }
        }

        if LAZY {
            if stopped {
                return (text_masks, true);
            }
            // Delta phase: re-scan entries whose mask grew after their initial callback.
            for i in 0..text_masks.len() {
                let delta = text_masks[i].mask & !scanned_masks[i];
                if delta != 0
                    && on_variant(
                        text_masks[i].text.as_ref(),
                        i,
                        delta,
                        text_masks[i].is_ascii,
                    )
                {
                    return (text_masks, true);
                }
            }
        }

        (text_masks, false)
    }
}
