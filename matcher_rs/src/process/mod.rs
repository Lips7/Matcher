//! Text transformation pipeline for standardizing input before pattern matching.
//!
//! The public surface is intentionally small:
//! [`text_process`], [`reduce_text_process`], [`reduce_text_process_emit`],
//! [`build_process_type_tree`], and [`walk_process_tree`].
//! Internally the module is split into a step registry, a traversal graph, and
//! low-level transform engines.
pub(crate) mod api;
pub(crate) mod graph;
pub(crate) mod process_type;
pub(crate) mod registry;
pub(crate) mod step;
pub(crate) mod transform;
pub(crate) mod variant;

pub use api::{reduce_text_process, reduce_text_process_emit, text_process};
pub use graph::{ProcessTypeBitNode, build_process_type_tree, walk_process_tree};
pub use process_type::ProcessType;
pub(crate) use variant::return_processed_string_to_pool;
pub use variant::{ProcessedTextMasks, TextVariant};
