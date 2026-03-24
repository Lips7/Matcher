//! Text normalization pipeline for standardizing input before pattern matching.
//!
//! Provides the [`ProcessType`](process_matcher::ProcessType) bitflags that describe
//! which transformation steps to apply to a text, together with the functions that
//! execute those steps. Available transformations include noise-character deletion,
//! Unicode normalization, Traditional→Simplified Chinese conversion (`Fanjian`),
//! and Pinyin transliteration.
pub(crate) mod process_matcher;
pub(crate) mod process_tree;
pub(crate) mod process_type;
pub(crate) mod string_pool;
pub(crate) mod transform;
