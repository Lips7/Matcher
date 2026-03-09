/// Text processing pipelines and constants.
///
/// This module provides the [`process_matcher::ProcessType`] bitflags and the functions designed to
/// standardize text before it is matched using `matcher_rs` matchers. Processing
/// rules such as lowercasing, spacing removal, traditional-to-simplified Chinese
/// conversion, or pinyin translation are defined here.
mod constants;
pub mod process_matcher;
pub mod single_char_matcher;
