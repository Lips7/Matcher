//! Compiled single-step transforms for the text-processing pipeline.
//!
//! Each [`TransformStep`] variant wraps a low-level matcher (VariantNorm, Delete,
//! Normalize, Romanize) and provides a uniform [`apply`](TransformStep::apply)
//! interface. Returns `Option<String>` — `None` when the input is unaffected.
//!
//! The registry is a fixed-size array of [`OnceLock`] slots — one per bit position in
//! [`ProcessType`]. Each slot is lazily initialized on first access.

use std::sync::OnceLock;

use crate::process::process_type::ProcessType;
use crate::process::transform::constants::*;
use crate::process::transform::delete::DeleteMatcher;
use crate::process::transform::replace::{NormalizeMatcher, RomanizeMatcher, VariantNormMatcher};

/// Compiled single-bit transformation step.
///
/// Each variant wraps the corresponding low-level matcher from [`super::transform`].
/// Instances are created by `build_transform_step` and cached in
/// `TRANSFORM_STEP_CACHE` for the lifetime of the process. The [`apply`](Self::apply)
/// method provides a uniform dispatch point.
#[derive(Clone)]
pub(crate) enum TransformStep {
    /// Raw-text path; returns the input unchanged.
    None,
    /// CJK variant normalization via page-table lookup.
    VariantNorm(VariantNormMatcher),
    /// Codepoint deletion using a bitset, with optional SIMD acceleration.
    Delete(DeleteMatcher),
    /// Multi-character normalization replacements via Aho-Corasick.
    Normalize(NormalizeMatcher),
    /// CJK romanization with inter-syllable spaces preserved.
    Romanize(RomanizeMatcher),
    /// CJK romanization with inter-syllable spaces stripped.
    RomanizeChar(RomanizeMatcher),
}

impl TransformStep {
    /// Returns whether this step is guaranteed to be a no-op on ASCII input.
    ///
    /// - **VariantNorm / Romanize / RomanizeChar**: no-op — all keys are non-ASCII codepoints.
    /// - **Delete / Normalize**: may change ASCII input (punctuation deletion, casefold).
    #[inline(always)]
    pub(crate) fn is_noop_on_ascii_input(&self) -> bool {
        matches!(
            self,
            Self::None | Self::VariantNorm(_) | Self::Romanize(_) | Self::RomanizeChar(_)
        )
    }

    /// Applies this step to `text`. Returns `Some((new_string, output_density))`
    /// if the text was modified, `None` if the step is a no-op for this input.
    ///
    /// `parent_density` is the non-ASCII byte density of `text` (0.0 = pure ASCII).
    /// The returned density is an estimate for engine dispatch:
    /// - **VariantNorm**: CJK→CJK, density unchanged → `parent_density`
    /// - **Delete / Normalize**: density approximately unchanged → `parent_density`
    /// - **Romanize / RomanizeChar**: CJK→ASCII, density drops → `0.0`
    ///
    /// When `parent_density == 0.0` the ASCII fast path fires:
    /// VariantNorm/Romanize/RomanizeChar are guaranteed no-ops on ASCII input,
    /// and Delete/Normalize produce ASCII output from ASCII input (proven by
    /// process map analysis).
    #[inline(always)]
    pub(crate) fn apply(&self, text: &str, parent_density: f32) -> Option<(String, f32)> {
        if parent_density == 0.0 {
            return match self {
                Self::None | Self::VariantNorm(_) | Self::Romanize(_) | Self::RomanizeChar(_) => {
                    None
                }
                Self::Delete(matcher) => matcher.delete(text).map(|s| (s, 0.0)),
                Self::Normalize(matcher) => matcher.replace(text).map(|s| (s, 0.0)),
            };
        }

        match self {
            Self::None => None,
            Self::VariantNorm(matcher) => matcher.replace(text).map(|s| (s, parent_density)),
            Self::Delete(matcher) => matcher.delete(text).map(|s| (s, parent_density)),
            Self::Normalize(matcher) => matcher.replace(text).map(|s| (s, parent_density)),
            Self::Romanize(matcher) | Self::RomanizeChar(matcher) => {
                matcher.replace(text).map(|s| (s, 0.0))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Lazy registry
// ---------------------------------------------------------------------------

static TRANSFORM_STEP_CACHE: [OnceLock<TransformStep>; 8] = [
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
];

pub(crate) fn get_transform_step(process_type_bit: ProcessType) -> &'static TransformStep {
    debug_assert!(
        process_type_bit.bits().is_power_of_two() || process_type_bit == ProcessType::None,
        "get_transform_step requires a single-bit ProcessType, got {:?}",
        process_type_bit
    );
    let index = process_type_bit.bits().trailing_zeros() as usize;
    debug_assert!(index < TRANSFORM_STEP_CACHE.len());

    TRANSFORM_STEP_CACHE[index].get_or_init(|| build_transform_step(process_type_bit))
}

fn build_transform_step(process_type_bit: ProcessType) -> TransformStep {
    match process_type_bit {
        ProcessType::None => TransformStep::None,
        ProcessType::VariantNorm => TransformStep::VariantNorm(VariantNormMatcher::new(
            VARIANT_NORM_L1_BYTES,
            VARIANT_NORM_L2_BYTES,
        )),
        ProcessType::Delete => TransformStep::Delete(DeleteMatcher::new(DELETE_BITSET_BYTES)),
        ProcessType::Normalize => TransformStep::Normalize(NormalizeMatcher::new(
            NORMALIZE_L1_BYTES,
            NORMALIZE_L2_BYTES,
            NORMALIZE_STR_BYTES,
        )),
        ProcessType::Romanize => TransformStep::Romanize(RomanizeMatcher::new(
            ROMANIZE_L1_BYTES,
            ROMANIZE_L2_BYTES,
            ROMANIZE_STR_BYTES,
            false,
        )),
        ProcessType::RomanizeChar => TransformStep::RomanizeChar(RomanizeMatcher::new(
            ROMANIZE_L1_BYTES,
            ROMANIZE_L2_BYTES,
            ROMANIZE_STR_BYTES,
            true,
        )),
        _ => unreachable!("unsupported single-bit ProcessType"),
    }
}
