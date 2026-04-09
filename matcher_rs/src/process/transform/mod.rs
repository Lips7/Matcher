//! Low-level text transformation engines.
//!
//! This module provides the building blocks used by the step registry
//! ([`super::step`]) and pipeline executor ([`super::api`]) to transform
//! input text before pattern matching. Each sub-module implements one class of
//! transformation:
//!
//! - [`constants`] -- Pre-compiled binary tables (page tables, bitsets)
//!   embedded at build time by `build.rs`.
//! - [`page_table`] -- Shared two-stage page-table infrastructure used by the
//!   replacement engines below.
//! - [`variant_norm`] -- CJK variant normalization (Traditional→Simplified,
//!   Kyūjitai→Shinjitai, half-width katakana→full-width).
//! - [`normalize`] -- Unicode NFKC normalization + casefolding.
//! - [`romanize`] -- CJK romanization (Pinyin, Romaji, Revised Romanization).
//! - [`delete`] -- A flat Unicode bitset engine ([`delete::DeleteMatcher`])
//!   that strips configured codepoints from text, with a fast ASCII LUT path.
//! - [`simd`] -- SIMD-accelerated byte-skip helpers that let the replace and
//!   delete engines jump over long runs of irrelevant ASCII bytes in a single
//!   instruction (AVX2 / NEON / portable `std::simd` fallback).
//!
//! All types in this module are `pub(crate)` -- they are internal
//! implementation details consumed by the higher-level [`super::step`] and
//! [`super::api`] modules.
pub(crate) mod constants;
pub(crate) mod delete;
pub(crate) mod filter;
pub(crate) mod normalize;
pub(crate) mod page_table;
pub(crate) mod romanize;
pub(crate) mod simd;
pub(crate) mod utf8;
pub(crate) mod variant_norm;
