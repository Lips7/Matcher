//! Compiled transformation steps and their lazy-initialization registry.
//!
//! A [`TransformStep`] wraps one of the low-level transform engines (Fanjian, Delete,
//! Normalize, PinYin, PinYinChar) and provides a uniform [`apply`](TransformStep::apply)
//! interface. [`StepOutput`] carries the result: either `changed = None` (the text was
//! unaffected) or `changed = Some(new_string)` together with an updated `is_ascii` flag.
//!
//! The registry is a fixed-size array of [`OnceLock`] slots — one per bit position in
//! [`ProcessType`]. On first access the corresponding [`TransformStep`] is compiled
//! (from build-time binary artifacts in [`super::transform::constants`]) and cached for
//! the lifetime of the process. All [`crate::SimpleMatcher`] instances
//! share the same compiled steps, so the heavy initialization cost (Aho-Corasick
//! compilation, page-table construction) is paid at most once per step per process.

use std::sync::OnceLock;

use crate::process::process_type::ProcessType;
use crate::process::transform::constants::*;
use crate::process::transform::delete::DeleteMatcher;
use crate::process::transform::replace::{FanjianMatcher, NormalizeMatcher, PinyinMatcher};

/// Result of applying one compiled pipeline step to a text variant.
///
/// `changed` is [`None`] when the step is a no-op for the provided input (the text was
/// not modified at all). `is_ascii` always describes the *post-step* text, regardless of
/// whether the text changed. Callers use this to decide whether to scan with the
/// bytewise or charwise Aho-Corasick automaton.
pub(crate) struct StepOutput {
    /// The transformed string, or [`None`] if the step did not modify the input.
    pub(crate) changed: Option<String>,
    /// Whether the post-step text consists entirely of ASCII bytes.
    pub(crate) is_ascii: bool,
}

/// Constructors for [`StepOutput`].
impl StepOutput {
    /// Creates an unchanged result that preserves the caller-provided ASCII flag.
    ///
    /// Used when a step determines that no characters in the input are affected by its
    /// transformation table.
    #[inline(always)]
    pub(crate) fn unchanged(is_ascii: bool) -> Self {
        Self {
            changed: None,
            is_ascii,
        }
    }

    /// Creates a changed result with the produced `String` and its ASCII status.
    #[inline(always)]
    pub(crate) fn changed(changed: String, is_ascii: bool) -> Self {
        Self {
            changed: Some(changed),
            is_ascii,
        }
    }
}

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
    /// Pinyin conversion with inter-syllable spaces stripped.
    PinYinChar(PinyinMatcher),
}

/// Execution policy for one cached transform step.
impl TransformStep {
    /// Returns the inner `DeleteMatcher` if this step is a Delete transform.
    #[inline(always)]
    pub(crate) fn as_delete(&self) -> Option<&DeleteMatcher> {
        match self {
            Self::Delete(m) => Some(m),
            _ => None,
        }
    }

    /// Returns the inner `NormalizeMatcher` if this step is a Normalize transform.
    #[inline(always)]
    pub(crate) fn as_normalize(&self) -> Option<&NormalizeMatcher> {
        match self {
            Self::Normalize(m) => Some(m),
            _ => None,
        }
    }

    /// Returns whether this step is guaranteed to be a no-op on ASCII input.
    ///
    /// Used by `walk_and_scan` to detect the no-op case for leaf nodes: when `true`,
    /// the leaf can reuse its parent's text and variant index instead of streaming
    /// a new byte iterator through the AC engine.
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

    /// Applies this step to `text`, returning a [`StepOutput`] indicating what changed.
    ///
    /// `parent_is_ascii` is the ASCII flag inherited from the parent text variant.
    ///
    /// When `parent_is_ascii` is `true`, ASCII-in → ASCII-out is guaranteed for all
    /// transforms (proven by process map analysis), so the output `is_ascii` flag is
    /// forced to `true` without re-scanning the result.
    ///
    /// - **Fanjian / PinYin / PinYinChar on ASCII**: always unchanged (no-op).
    /// - **Delete / Normalize on ASCII**: may change text but output stays ASCII.
    /// - **Fanjian on non-ASCII**: CJK→CJK, `is_ascii = false`.
    /// - **Delete / Normalize / PinYin / PinYinChar on non-ASCII**: `is_ascii` determined
    ///   by the underlying engine via `result.is_ascii()`.
    #[inline(always)]
    pub(crate) fn apply(&self, text: &str, parent_is_ascii: bool) -> StepOutput {
        if parent_is_ascii {
            // ASCII-in → ASCII-out for all transforms; Fanjian/PinYin/PinYinChar are no-ops.
            return match self {
                Self::None | Self::Fanjian(_) | Self::PinYin(_) | Self::PinYinChar(_) => {
                    StepOutput::unchanged(true)
                }
                Self::Delete(matcher) => matcher.delete(text).map_or_else(
                    || StepOutput::unchanged(true),
                    |(changed, _)| StepOutput::changed(changed, true),
                ),
                Self::Normalize(matcher) => matcher.replace(text).map_or_else(
                    || StepOutput::unchanged(true),
                    |(changed, _)| StepOutput::changed(changed, true),
                ),
            };
        }

        // Non-ASCII parent: use engine-reported is_ascii.
        match self {
            Self::None => StepOutput::unchanged(false),
            Self::Fanjian(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(false),
                |changed| StepOutput::changed(changed, false), // CJK→CJK
            ),
            Self::Delete(matcher) => matcher.delete(text).map_or_else(
                || StepOutput::unchanged(false),
                |(changed, is_ascii)| StepOutput::changed(changed, is_ascii),
            ),
            Self::Normalize(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(false),
                |(changed, is_ascii)| StepOutput::changed(changed, is_ascii),
            ),
            Self::PinYin(matcher) | Self::PinYinChar(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(false),
                |(changed, is_ascii)| StepOutput::changed(changed, is_ascii),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Lazy registry
// ---------------------------------------------------------------------------

/// Lazily initialized cache keyed by the bit position of a single-bit [`ProcessType`].
///
/// The array has 8 slots — one for each possible bit in the `u8` bitflags. Slots are
/// initialized on first access via [`OnceLock::get_or_init`] and live for the duration
/// of the process. All [`crate::SimpleMatcher`] instances share these compiled steps.
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

/// Returns the cached compiled step for a single-bit [`ProcessType`].
///
/// Uses `process_type_bit.bits().trailing_zeros()` as the index into
/// [`TRANSFORM_STEP_CACHE`], which maps directly to the bit position of the
/// single-bit flag (e.g., `ProcessType::Fanjian` = bit 1 → index 1).
///
/// If the cache slot has not been initialized yet, `build_transform_step` is
/// called once via [`OnceLock::get_or_init`] and the result is stored for the
/// lifetime of the process. Subsequent calls for the same bit return immediately
/// with an atomic load.
///
/// The returned reference is `'static`: it lives as long as the process, so
/// [`ProcessTypeBitNode`](super::graph::ProcessTypeBitNode) can store it without
/// lifetime parameters.
///
/// # Panics
///
/// Debug-asserts that `process_type_bit` has exactly one bit set and that the
/// resulting index is within the cache bounds. In release mode, passing a
/// multi-bit or out-of-range value causes an out-of-bounds array access.
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
