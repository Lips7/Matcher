//! Text normalization pipeline for standardizing input before pattern matching.
//!
//! Exposes the [`ProcessType`] bitflags together with the helpers that execute those
//! transformation steps. The public surface is small: use the one-shot processing
//! helpers for a single composite pipeline, or build a transform tree when multiple
//! pipelines should share work.
pub(crate) mod process_matcher;
pub(crate) mod process_tree;
pub(crate) mod process_type;
pub(crate) mod string_pool;
pub(crate) mod transform;

pub use process_tree::{ProcessTypeBitNode, build_process_type_tree, walk_process_tree};
pub use process_type::ProcessType;
pub(crate) use string_pool::return_processed_string_to_pool;
pub use string_pool::{ProcessedTextMasks, TextVariant};
