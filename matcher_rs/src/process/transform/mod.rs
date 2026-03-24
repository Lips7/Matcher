//! Low-level text transformation engines.
//!
//! Provides the building blocks used by [`super::process_matcher::ProcessMatcher`]:
//! pre-compiled data tables ([`constants`]), single-character lookup engines
//! ([`single_char_matcher`]), multi-character substitution
//! ([`multi_char_matcher`]), and SIMD-accelerated character-skip utilities ([`simd_utils`]).
pub(crate) mod constants;
pub(crate) mod multi_char_matcher;
pub(crate) mod simd_utils;
pub(crate) mod single_char_matcher;
