//! Transformation trie construction.
//!
//! The trie is a flat array where each node represents one single-bit [`ProcessType`] step.
//! [`build_process_type_tree`] constructs it from a set of composite bitmasks, merging shared
//! prefixes so that intermediate results are computed once.
//!
//! [`SimpleMatcher`](crate::SimpleMatcher) stores the trie and walks it at match time via
//! [`walk_and_scan`](crate::simple_matcher::SimpleMatcher::walk_and_scan) in `search.rs`.
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

use std::collections::HashSet;

use tinyvec::TinyVec;

use crate::process::process_type::ProcessType;
use crate::process::registry::get_transform_step;
use crate::process::step::TransformStep;

/// A node in the flat-array transformation trie.
///
/// Built once by [`build_process_type_tree`] and stored in
/// [`SimpleMatcher`](crate::SimpleMatcher)'s `ProcessPlan`. Each node represents a single
/// transformation step reachable from its parent.
#[derive(Clone)]
pub(crate) struct ProcessTypeBitNode {
    process_type_list: TinyVec<[ProcessType; 4]>,
    pub(crate) process_type_bit: ProcessType,
    pub(crate) children: TinyVec<[usize; 4]>,
    pub(crate) step: Option<&'static TransformStep>,
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
pub(crate) fn build_process_type_tree(
    process_type_set: &HashSet<ProcessType>,
) -> Vec<ProcessTypeBitNode> {
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
