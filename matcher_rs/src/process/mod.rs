//! Text transformation pipeline for standardizing input before pattern
//! matching.
//!
//! This module converts raw input text through a configurable series of steps —
//! Traditional-to-Simplified Chinese conversion, codepoint deletion,
//! normalization, and CJK romanization — so that [`crate::SimpleMatcher`] can
//! match patterns against both raw and transformed forms of the same text.
//!
//! # Public API
//!
//! | Item | Purpose |
//! |------|---------|
//! | [`ProcessType`] | Bitflags selecting which transformation steps to apply. |
//! | [`text_process`] | Applies a composite pipeline and returns the final result. |
//! | [`reduce_text_process`] | Applies a pipeline and records each intermediate change. |
//! | [`reduce_text_process_emit`] | Like `reduce_text_process`, but merges replace-type steps in-place. |
//!
//! # Internal structure
//!
//! - [`step`] — [`TransformStep`](step::TransformStep) enum and the global
//!   `OnceLock` cache that lazily compiles each single-bit step once.
//! - [`graph`] — Trie construction (reuses shared prefixes across process
//!   types).
//! - [`api`] — Standalone helpers ([`text_process`], [`reduce_text_process`],
//!   etc.).
//! - [`transform`] — Low-level engines (charwise page-table, Aho-Corasick
//!   normalizer, SIMD delete).
pub(crate) mod api;
pub(crate) mod graph;
pub(crate) mod process_type;
pub(crate) mod step;
pub(crate) mod transform;

pub use api::{reduce_text_process, reduce_text_process_emit, text_process};
pub use process_type::ProcessType;
