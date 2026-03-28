//! Text transformation pipeline for standardizing input before pattern matching.
//!
//! This module converts raw input text through a configurable series of steps —
//! Traditional-to-Simplified Chinese conversion, codepoint deletion, normalization,
//! and Pinyin transliteration — so that [`crate::SimpleMatcher`] can match patterns
//! against both raw and transformed forms of the same text.
//!
//! # Public API
//!
//! The public surface is intentionally small:
//!
//! | Item | Purpose |
//! |------|---------|
//! | [`ProcessType`] | Bitflags selecting which transformation steps to apply. |
//! | [`text_process`] | Applies a composite pipeline and returns the final result. |
//! | [`reduce_text_process`] | Applies a pipeline and records each intermediate change. |
//! | [`reduce_text_process_emit`] | Like `reduce_text_process`, but merges replace-type steps in-place. |
//! | [`build_process_type_tree`] | Builds a flat-array trie that reuses shared prefixes across process types. |
//! | [`walk_process_tree`] | Walks the trie, producing all unique [`TextVariant`]s for one input. |
//! | [`TextVariant`] | One transformed text plus metadata (bitmask, ASCII flag). |
//! | [`ProcessedTextMasks`] | All [`TextVariant`]s produced for a single input. |
//!
//! # Internal structure
//!
//! Internally the module is split into:
//!
//! - [`step`] — [`TransformStep`](step::TransformStep) enum and [`StepOutput`](step::StepOutput).
//! - [`registry`] — Global `OnceLock` cache that lazily compiles each single-bit step once.
//! - [`graph`] — Trie construction and traversal (the "DAG" that reuses intermediate results).
//! - [`variant`] — [`TextVariant`], [`ProcessedTextMasks`], and thread-local buffer pools.
//! - [`api`] — Standalone helpers ([`text_process`], [`reduce_text_process`], etc.).
//! - [`transform`] — Low-level engines (charwise page-table, Aho-Corasick normalizer, SIMD delete).
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
