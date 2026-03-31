//! Low-level text transformation engines.
//!
//! This module provides the building blocks used by the step registry
//! ([`super::step`]) and pipeline executor ([`super::api`]) to transform
//! input text before pattern matching. Each sub-module implements one class of
//! transformation:
//!
//! - [`constants`] -- Pre-compiled binary tables (page tables, bitsets,
//!   serialized automata) embedded at build time by `build.rs`, or raw source
//!   text maps when the `runtime_build` feature is active.
//! - [`charwise`] -- Two-stage page-table lookup engines for single-codepoint
//!   replacements: [`charwise::FanjianMatcher`] (Traditional-to-Simplified
//!   Chinese) and [`charwise::PinyinMatcher`] (CJK-to-Pinyin).
//! - [`delete`] -- A flat Unicode bitset engine ([`delete::DeleteMatcher`])
//!   that strips configured codepoints from text, with a fast ASCII LUT path.
//! - [`normalize`] -- Multi-character replacement via Aho-Corasick
//!   ([`normalize::NormalizeMatcher`]), handling full-width-to-half-width,
//!   variant forms, and number normalization.
//! - [`simd`] -- SIMD-accelerated byte-skip helpers that let the charwise and
//!   delete engines jump over long runs of irrelevant ASCII bytes in a single
//!   instruction (AVX2 / NEON / portable `std::simd` fallback).
//!
//! All types in this module are `pub(crate)` -- they are internal implementation
//! details consumed by the higher-level [`super::registry`] and
//! [`super::api`] modules.
pub(crate) mod charwise;
pub(crate) mod constants;
pub(crate) mod delete;
pub(crate) mod normalize;
pub(crate) mod simd;
pub(crate) mod utf8;
