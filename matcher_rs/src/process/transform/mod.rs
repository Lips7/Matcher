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
//! - [`replace`] -- Text-replacement engines: [`replace::FanjianMatcher`]
//!   (Traditional-to-Simplified Chinese, page-table), [`replace::PinyinMatcher`]
//!   (CJK-to-Pinyin, page-table), and [`replace::NormalizeMatcher`]
//!   (multi-character Aho-Corasick replacement).
//! - [`delete`] -- A flat Unicode bitset engine ([`delete::DeleteMatcher`])
//!   that strips configured codepoints from text, with a fast ASCII LUT path.
//! - [`simd`] -- SIMD-accelerated byte-skip helpers that let the replace and
//!   delete engines jump over long runs of irrelevant ASCII bytes in a single
//!   instruction (AVX2 / NEON / portable `std::simd` fallback).
//!
//! All types in this module are `pub(crate)` -- they are internal implementation
//! details consumed by the higher-level [`super::step`] and
//! [`super::api`] modules.
pub(crate) mod constants;
pub(crate) mod delete;
pub(crate) mod replace;
pub(crate) mod simd;
pub(crate) mod utf8;
