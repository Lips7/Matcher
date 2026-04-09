//! Transformation trie construction.
//!
//! The trie is a flat array where each node represents one single-bit
//! [`ProcessType`] step. [`build_process_type_tree`] constructs it from a set
//! of composite bitmasks, merging shared prefixes so that intermediate results
//! are computed once.
//!
//! [`SimpleMatcher`](crate::SimpleMatcher) stores the trie and walks it at
//! match time via
//! [`walk_and_scan`](crate::simple_matcher::SimpleMatcher::walk_and_scan) in
//! `search.rs`.
//!
//! # Example trie
//!
//! Given the set `{VariantNorm, VariantNorm|Delete,
//! VariantNorm|Delete|Normalize}`, the trie is:
//!
//! ```text
//!   [0] root (None)
//!    └─[1] VariantNorm          ← terminates: {VariantNorm}
//!       └─[2] Delete        ← terminates: {VariantNorm|Delete}
//!          └─[3] Normalize  ← terminates: {VariantNorm|Delete|Normalize}
//! ```

use std::collections::HashSet;

use crate::process::{
    process_type::ProcessType,
    step::{TransformStep, get_transform_step},
};

/// A node in the flat-array transformation trie.
///
/// Built once by [`build_process_type_tree`] and stored in
/// [`SimpleMatcher`](crate::SimpleMatcher). Each node represents a single
/// transformation step reachable from its parent.
#[derive(Clone)]
pub(crate) struct ProcessTypeBitNode {
    /// The single-bit [`ProcessType`] transformation step this node represents.
    ///
    /// The root node always has `ProcessType::None`; all other nodes have
    /// exactly one bit set (e.g., `VariantNorm`, `Delete`).
    pub(crate) process_type_bit: ProcessType,
    /// Indices of child nodes in the flat trie array.
    ///
    /// Children represent the next transformation step that follows this one.
    /// Empty for leaf nodes.
    pub(crate) children: Vec<usize>,
    /// Cached reference to the compiled [`TransformStep`] for this node's bit.
    ///
    /// [`None`] only for the root node (which represents the raw input text and
    /// needs no transformation). All other nodes hold a `&'static` reference
    /// obtained from the global `TRANSFORM_STEP_CACHE` in
    /// [`crate::process::step`].
    pub(crate) step: Option<&'static TransformStep>,
    /// Bitmask of compact process-type indices that produce a scannable variant
    /// at this node.
    ///
    /// Bit `i` is set when the composite [`ProcessType`] whose compact index is
    /// `i` terminates at (or passes through) this node. A non-zero mask means
    /// this node's text variant should be scanned by the AC automaton.
    /// Encoded using sequential indices from `pt_index_table` during
    /// construction.
    pub(crate) pt_index_mask: u64,
}

impl ProcessTypeBitNode {
    /// Returns the estimated heap memory in bytes used by the `children` vec.
    pub(crate) fn heap_bytes(&self) -> usize {
        self.children.capacity() * size_of::<usize>()
    }
}

/// Builds a flat-array trie from a set of composite [`ProcessType`] bitmasks.
///
/// The trie encodes every unique prefix path among the given composite types. A
/// root node with `process_type_bit = ProcessType::None` is always present at
/// index 0. For each composite type (e.g. `VariantNorm | Delete`), the
/// constructor walks its constituent bits in ascending order (determined by
/// [`ProcessType::iter`]), reusing existing child nodes where the path overlaps
/// with previously inserted types and creating new child nodes when a path
/// diverges.
///
/// `pt_index_table` maps raw `ProcessType::bits()` to compact sequential
/// indices (0..N). The `pt_index_mask` on each node is computed directly using
/// these indices, so no post-construction reindexing is needed.
///
/// The resulting `Vec<ProcessTypeBitNode>` is indexed by node position in the
/// trie. The root (index 0) represents the raw input; each subsequent node
/// represents one transformation step. Node indices are used by `walk_and_scan`
/// to traverse the trie at match time.
///
/// ```text
/// // Given process_type_set = {VariantNorm|Delete, VariantNorm|Delete|Normalize}:
/// let tree = build_process_type_tree(&set, &pt_index_table);
/// // tree[0] = root (None), children: [1]
/// // tree[1] = VariantNorm,     children: [2]
/// // tree[2] = Delete,      children: [3], terminates
/// // tree[3] = Normalize,   children: [],  terminates
/// ```
pub(crate) fn build_process_type_tree(
    process_type_set: &HashSet<ProcessType>,
    pt_index_table: &[u8; 128],
) -> Vec<ProcessTypeBitNode> {
    let max_nodes: usize = 1 + process_type_set
        .iter()
        .map(|pt| pt.bits().count_ones() as usize)
        .sum::<usize>();
    let mut process_type_tree = Vec::with_capacity(max_nodes);
    let mut root = ProcessTypeBitNode {
        process_type_bit: ProcessType::None,
        children: Vec::new(),
        step: None,
        pt_index_mask: 0,
    };
    if process_type_set.contains(&ProcessType::None) {
        root.pt_index_mask |= 1u64 << pt_index_table[ProcessType::None.bits() as usize];
    }
    process_type_tree.push(root);
    for &process_type in process_type_set.iter() {
        let pt_mask_bit = 1u64 << pt_index_table[process_type.bits() as usize];
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
                process_type_tree[current_node_index].pt_index_mask |= pt_mask_bit;
            } else {
                let child = ProcessTypeBitNode {
                    process_type_bit,
                    children: Vec::new(),
                    step: Some(get_transform_step(process_type_bit)),
                    pt_index_mask: pt_mask_bit,
                };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessType;

    fn identity_index_table() -> [u8; 128] {
        let mut table = [u8::MAX; 128];
        for i in 0..128u8 {
            table[i as usize] = i;
        }
        table
    }

    #[test]
    fn test_tree_single_none() {
        let set: HashSet<ProcessType> = [ProcessType::None].into_iter().collect();
        let tree = build_process_type_tree(&set, &identity_index_table());

        assert_eq!(tree.len(), 1); // root only
        assert!(tree[0].children.is_empty());
        assert_ne!(
            tree[0].pt_index_mask, 0,
            "root should have non-zero mask for None"
        );
        assert_eq!(tree[0].process_type_bit, ProcessType::None);
    }

    #[test]
    fn test_tree_prefix_sharing() {
        let set: HashSet<ProcessType> = [
            ProcessType::VariantNorm,
            ProcessType::VariantNorm | ProcessType::Delete,
        ]
        .into_iter()
        .collect();
        let tree = build_process_type_tree(&set, &identity_index_table());

        // Root + VariantNorm + Delete = 3 nodes
        assert_eq!(tree.len(), 3);
        // Root has one child (VariantNorm)
        assert_eq!(tree[0].children.len(), 1);
        let fj_idx = tree[0].children[0];
        assert_eq!(tree[fj_idx].process_type_bit, ProcessType::VariantNorm);
        // VariantNorm node has one child (Delete)
        assert_eq!(tree[fj_idx].children.len(), 1);
        let del_idx = tree[fj_idx].children[0];
        assert_eq!(tree[del_idx].process_type_bit, ProcessType::Delete);
        assert!(tree[del_idx].children.is_empty());
    }
}
