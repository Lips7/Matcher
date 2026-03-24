use std::borrow::Cow;
use std::collections::HashSet;

use tinyvec::TinyVec;

use crate::process::process_matcher::get_process_matcher;
use crate::process::process_type::ProcessType;
use crate::process::string_pool::{
    ProcessedTextMasks, TRANSFORM_STATE, TextVariant, return_string_to_pool,
};

/// A node in the flat-array transformation trie used by [`walk_process_tree`].
///
/// The trie is built once by [`build_process_type_tree`] and stored inside `SimpleMatcher`.
/// Each node represents a single transformation step (`process_type_bit`) reachable from
/// its parent. The `process_type_list` records which composite [`ProcessType`] values
/// "arrive" at this node (i.e. terminate here), so the traversal can tag output text
/// variants with the correct bitmask. `children` holds flat-array indices of the next steps.
/// `folded_mask` is the pre-computed OR of `1u64 << pt.bits()` for all entries in
/// `process_type_list`, avoiding the per-call fold in the hot traversal loop.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessTypeBitNode {
    /// The composite [`ProcessType`] values whose decomposed bit-path terminates at this node.
    /// A non-empty list means that one or more rules emit a text variant here.
    process_type_list: Vec<ProcessType>,
    /// The single-bit [`ProcessType`] step that this node represents (i.e., the edge label
    /// from the parent). For the root node this is [`ProcessType::None`].
    pub(crate) process_type_bit: ProcessType,
    /// Flat-array indices of child nodes (the next transformation steps reachable from here).
    pub(crate) children: Vec<usize>,
    /// Pre-computed OR of `1u64 << pt.bits()` for every `pt` in `process_type_list`.
    /// Avoids a per-traversal fold in the hot [`walk_process_tree`] loop.
    pub(crate) folded_mask: u64,
}

impl ProcessTypeBitNode {
    /// Re-encodes `folded_mask` using a sequential index table.
    ///
    /// The default encoding stores `1u64 << pt.bits()`, which can use bits up to 63 for
    /// composite [`ProcessType`] values. A sequential index keeps bit positions small (0..N
    /// where N is the number of unique composite types) so [`PatternEntry`] can store the
    /// index as a `u8` rather than a `u64`, halving the entry size.
    ///
    /// `pt_index_table[pt.bits()]` must contain the sequential index for every composite
    /// [`ProcessType`] that terminates at any node (i.e. every type in the original
    /// `process_type_set` plus [`ProcessType::None`]).
    pub(crate) fn recompute_mask_with_index(&mut self, pt_index_table: &[u8; 64]) {
        self.folded_mask = self.process_type_list.iter().fold(0u64, |acc, pt| {
            acc | (1u64 << pt_index_table[pt.bits() as usize])
        });
    }
}

/// Builds a flat-array trie from a set of composite [`ProcessType`] bitmasks.
///
/// The trie encodes every unique prefix path among the given composite types. A root node
/// with `process_type_bit = ProcessType::None` is always present at index 0. For each
/// composite type (e.g. `Fanjian | Delete`), the constructor walks its constituent bits in
/// order, reusing existing child nodes where the path overlaps with previously inserted types
/// and creating new child nodes when a path diverges.
///
/// The resulting flat `Vec<ProcessTypeBitNode>` is passed to
/// [`walk_process_tree`], which performs a single BFS traversal to compute all
/// needed text variants while sharing common intermediate results.
pub fn build_process_type_tree(process_type_set: &HashSet<ProcessType>) -> Vec<ProcessTypeBitNode> {
    let mut process_type_tree = Vec::new();
    let mut root = ProcessTypeBitNode {
        process_type_list: Vec::new(),
        process_type_bit: ProcessType::None,
        children: Vec::new(),
        folded_mask: 0,
    };
    if process_type_set.contains(&ProcessType::None) {
        root.process_type_list.push(ProcessType::None);
        root.folded_mask |= 1u64 << ProcessType::None.bits();
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
                process_type_tree[current_node_index].folded_mask |= 1u64 << process_type.bits();
            } else {
                let mut child = ProcessTypeBitNode {
                    process_type_list: Vec::new(),
                    process_type_bit,
                    children: Vec::new(),
                    folded_mask: 0,
                };
                child.process_type_list.push(process_type);
                child.folded_mask |= 1u64 << process_type.bits();
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
/// If `changed` is `Some(pt)` and `pt` already exists in `text_masks`, the string is
/// returned to the pool and the existing index is returned. Otherwise `pt` is appended.
/// If `changed` is `None`, `current_index` is returned unchanged.
///
/// `is_ascii` indicates whether `processed` contains only ASCII bytes; stored alongside
/// the text so callers can skip the charwise automaton without a redundant byte scan.
///
/// This keeps `walk_process_tree` in "unique string" space even when different trie paths
/// converge onto the same transformed text.
#[inline(always)]
fn dedup_insert(
    text_masks: &mut ProcessedTextMasks<'_>,
    current_index: usize,
    changed: Option<String>,
    is_ascii: bool,
) -> usize {
    match changed {
        Some(processed) => {
            if let Some(pos) = text_masks
                .iter()
                .position(|tv| tv.text.as_ref() == processed.as_str())
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
/// This is the hot-path function called on every [`crate::SimpleMatcher::is_match`] /
/// [`crate::SimpleMatcher::process`] invocation. It performs a single left-to-right BFS traversal
/// of `process_type_tree`, applying each transformation step exactly once per unique path. Common
/// prefixes (e.g. the shared `Fanjian` step for both `Fanjian | Delete` and `Fanjian | Normalize`)
/// are computed only once and their result indices are reused for child nodes.
///
/// Each [`TextVariant`] in the returned `Vec` carries the transformed text, the bitmask of
/// [`ProcessType`] indices that produced it, and an `is_ascii` flag. Callers can use
/// `is_ascii` to skip the charwise automaton without a redundant byte scan.
///
/// When `LAZY=true`, `on_variant(text, index, mask, is_ascii)` is called immediately after each
/// new unique variant is produced. If it returns `true`, the walk stops early. A delta phase at
/// the end re-invokes `on_variant` for any entry whose mask grew through dedup after its initial
/// callback. When `LAZY=false`, `on_variant` is never called and all lazy-only code is
/// dead-code-eliminated.
///
/// Returns `(text_masks, stopped)` where `stopped` is `true` only when `LAZY=true` and
/// `on_variant` triggered early exit. Inside `matcher_rs`, owned strings are usually returned
/// to a thread-local pool after use; external callers can simply let the returned vector drop.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashSet;
///
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
/// let (variants, stopped) = walk_process_tree::<false, _>(&tree, "妳！好", &mut |_, _, _, _| false);
/// assert!(!stopped);
///
/// let texts = variants
///     .into_iter()
///     .map(|tv| tv.text.into_owned())
///     .collect::<std::collections::HashSet<_>>();
///
/// assert_eq!(texts.len(), 4);
/// assert!(texts.contains("妳！好"));
/// assert!(texts.contains("你！好"));
/// assert!(texts.contains("妳好"));
/// assert!(texts.contains("你好"));
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
    TRANSFORM_STATE.with(|state| {
        let mut ts = state.borrow_mut();

        let pooled: Option<ProcessedTextMasks<'static>> = ts.masks_pool.pop();
        // Safety: pool holds empty Vecs with no live borrows; transmuting from
        // 'static to 'a is safe since 'static: 'a (covariant) and Vec is empty.
        let mut text_masks: ProcessedTextMasks<'a> =
            unsafe { std::mem::transmute(pooled.unwrap_or_default()) };
        text_masks.clear();
        let root_is_ascii = text.is_ascii();
        text_masks.push(TextVariant {
            text: Cow::Borrowed(text),
            mask: process_type_tree[0].folded_mask,
            is_ascii: root_is_ascii,
        });

        let mut scanned_masks: TinyVec<[u64; 8]> = TinyVec::new();
        if LAZY {
            scanned_masks.push(0u64);
            let root_mask = process_type_tree[0].folded_mask;
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
                let pm = get_process_matcher(child_node.process_type_bit);

                // Compute (changed_text, is_ascii_of_result) for this transformation step.
                // - PinYin/PinYinChar output is always ASCII (romanized).
                // - Fanjian maps CJK→CJK: if changed, result is non-ASCII.
                // - Delete can only remove chars: if parent is ASCII, result is still ASCII;
                //   if parent is non-ASCII, check the shorter result directly.
                // - Normalize: check the result string.
                // - None: no transformation, inherit parent.
                // When unchanged (None returned), child inherits parent's flag (O(1)).
                let (changed, child_is_ascii) = match child_node.process_type_bit {
                    ProcessType::None => (None, parent_is_ascii),
                    ProcessType::PinYin | ProcessType::PinYinChar => {
                        let current_text = text_masks[current_index].text.as_ref();
                        if let Some(processed) = pm.replace_all(current_text) {
                            (Some(processed), true)
                        } else {
                            (None, parent_is_ascii)
                        }
                    }
                    ProcessType::Fanjian => {
                        let current_text = text_masks[current_index].text.as_ref();
                        if let Some(processed) = pm.replace_all(current_text) {
                            (Some(processed), false)
                        } else {
                            (None, parent_is_ascii)
                        }
                    }
                    ProcessType::Delete => {
                        let current_text = text_masks[current_index].text.as_ref();
                        if let Some(processed) = pm.delete_all(current_text) {
                            // If parent was ASCII, result is still ASCII (only ASCII chars removed).
                            // If parent was non-ASCII, some non-ASCII may have been deleted —
                            // check the shorter result string.
                            let ia = parent_is_ascii || processed.is_ascii();
                            (Some(processed), ia)
                        } else {
                            (None, parent_is_ascii)
                        }
                    }
                    _ => {
                        let current_text = text_masks[current_index].text.as_ref();
                        if let Some(processed) = pm.replace_all(current_text) {
                            let ia = processed.is_ascii();
                            (Some(processed), ia)
                        } else {
                            (None, parent_is_ascii)
                        }
                    }
                };

                let old_len = if LAZY { text_masks.len() } else { 0 };
                let child_index =
                    dedup_insert(&mut text_masks, current_index, changed, child_is_ascii);

                if LAZY {
                    while scanned_masks.len() < text_masks.len() {
                        scanned_masks.push(0u64);
                    }
                }

                ts.tree_node_indices[child_node_index] = child_index;
                text_masks[child_index].mask |= child_node.folded_mask;

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
    })
}
