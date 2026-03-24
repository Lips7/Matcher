//! Text normalization pipeline for standardizing input before pattern matching.
//!
//! Provides the [`ProcessType`] bitflags that describe
//! which transformation steps to apply to a text, together with the functions that
//! execute those steps. Available transformations include noise-character deletion,
//! Unicode normalization, Traditional→Simplified Chinese conversion (`Fanjian`),
//! and Pinyin transliteration.
pub(crate) mod process_matcher;
pub(crate) mod process_tree;
pub(crate) mod process_type;
pub(crate) mod string_pool;
pub(crate) mod transform;

pub use process_tree::{ProcessTypeBitNode, build_process_type_tree, walk_process_tree};
pub use process_type::ProcessType;
pub(crate) use string_pool::return_processed_string_to_pool;
pub use string_pool::{ProcessedTextMasks, TextVariant};
