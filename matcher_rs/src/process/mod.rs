//! Text normalization pipeline for standardizing input before pattern matching.
//!
//! Provides the [`ProcessType`](process_matcher::ProcessType) bitflags that describe
//! which transformation steps to apply to a text, together with the functions that
//! execute those steps. Available transformations include noise-character deletion,
//! Unicode normalization, Traditional→Simplified Chinese conversion (`Fanjian`),
//! and Pinyin transliteration.
mod constants;
pub mod multi_char_matcher;
pub mod process_matcher;
pub mod single_char_matcher;
