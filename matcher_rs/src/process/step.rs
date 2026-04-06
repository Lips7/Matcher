//! Compiled single-step transforms for the text-processing pipeline.
//!
//! Each [`TransformStep`] variant wraps a low-level matcher (Fanjian, Delete,
//! Normalize, PinYin) and provides a uniform [`apply`](TransformStep::apply)
//! interface. Returns `Option<String>` — `None` when the input is unaffected.
//!
//! The registry is a fixed-size array of [`OnceLock`] slots — one per bit position in
//! [`ProcessType`]. Each slot is lazily initialized on first access.

use std::sync::OnceLock;

use crate::process::process_type::ProcessType;
use crate::process::transform::constants::*;
use crate::process::transform::delete::DeleteMatcher;
use crate::process::transform::replace::{FanjianMatcher, NormalizeMatcher, PinyinMatcher};

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
    /// Traditional-to-Simplified Chinese conversion via page-table lookup.
    Fanjian(FanjianMatcher),
    /// Codepoint deletion using a bitset, with optional SIMD acceleration.
    Delete(DeleteMatcher),
    /// Multi-character normalization replacements via Aho-Corasick.
    Normalize(NormalizeMatcher),
    /// Pinyin conversion with inter-syllable spaces preserved.
    PinYin(PinyinMatcher),
    /// Pinyin conversion that keeps only the initial of each syllable.
    PinYinChar(PinyinMatcher),
}

impl TransformStep {
    /// Returns whether this step is guaranteed to be a no-op on ASCII input.
    ///
    /// - **Fanjian / PinYin / PinYinChar**: no-op — all keys are non-ASCII codepoints.
    /// - **Delete / Normalize**: may change ASCII input (punctuation deletion, casefold).
    #[inline(always)]
    pub(crate) fn is_noop_on_ascii_input(&self) -> bool {
        matches!(
            self,
            Self::None | Self::Fanjian(_) | Self::PinYin(_) | Self::PinYinChar(_)
        )
    }

    /// Conservative estimate of the non-ASCII byte density after this transform.
    ///
    /// Returns `parent_density` — a safe bound since no transform increases
    /// non-ASCII density. Used by `walk_and_scan` to propagate engine-dispatch
    /// density through the transform tree without re-scanning each variant.
    #[inline(always)]
    pub(crate) fn output_density(&self, parent_density: f32) -> f32 {
        parent_density
    }

    /// Applies this step to `text`. Returns `Some(new_string)` if the text was
    /// modified, `None` if the step is a no-op for this input.
    ///
    /// `parent_is_ascii` enables the ASCII fast path: Fanjian/PinYin/PinYinChar
    /// are guaranteed no-ops on ASCII input, and Delete/Normalize produce ASCII
    /// output from ASCII input (proven by process map analysis).
    #[inline(always)]
    pub(crate) fn apply(&self, text: &str, parent_is_ascii: bool) -> Option<String> {
        if parent_is_ascii {
            return match self {
                Self::None | Self::Fanjian(_) | Self::PinYin(_) | Self::PinYinChar(_) => None,
                Self::Delete(matcher) => matcher.delete(text).map(|(s, _)| s),
                Self::Normalize(matcher) => matcher.replace(text).map(|(s, _)| s),
            };
        }

        match self {
            Self::None => None,
            Self::Fanjian(matcher) => matcher.replace(text),
            Self::Delete(matcher) => matcher.delete(text).map(|(s, _)| s),
            Self::Normalize(matcher) => matcher.replace(text).map(|(s, _)| s),
            Self::PinYin(matcher) | Self::PinYinChar(matcher) => {
                matcher.replace(text).map(|(s, _)| s)
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
        ProcessType::Fanjian => {
            TransformStep::Fanjian(FanjianMatcher::new(FANJIAN_L1_BYTES, FANJIAN_L2_BYTES))
        }
        ProcessType::Delete => TransformStep::Delete(DeleteMatcher::new(DELETE_BITSET_BYTES)),
        ProcessType::Normalize => TransformStep::Normalize(NormalizeMatcher::new(
            NORMALIZE_L1_BYTES,
            NORMALIZE_L2_BYTES,
            NORMALIZE_STR_BYTES,
        )),
        ProcessType::PinYin => TransformStep::PinYin(PinyinMatcher::new(
            PINYIN_L1_BYTES,
            PINYIN_L2_BYTES,
            PINYIN_STR_BYTES,
            false,
        )),
        ProcessType::PinYinChar => TransformStep::PinYinChar(PinyinMatcher::new(
            PINYIN_L1_BYTES,
            PINYIN_L2_BYTES,
            PINYIN_STR_BYTES,
            true,
        )),
        _ => unreachable!("unsupported single-bit ProcessType"),
    }
}
