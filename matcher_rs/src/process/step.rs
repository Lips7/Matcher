//! Compiled transformation steps and their lazy-initialization registry.
//!
//! A [`TransformStep`] wraps one of the low-level transform engines (Fanjian, Delete,
//! Normalize, PinYin, PinYinChar) and provides a uniform [`apply`](TransformStep::apply)
//! interface. [`StepOutput`] carries the result: either `changed = None` (the text was
//! unaffected) or `changed = Some(new_string)` together with an updated `is_ascii` flag.
//!
//! The registry is a fixed-size array of [`OnceLock`] slots — one per bit position in
//! [`ProcessType`]. On first access the corresponding [`TransformStep`] is compiled
//! (either from build-time artifacts or from source maps when `runtime_build` is enabled)
//! and cached for the lifetime of the process. All [`crate::SimpleMatcher`] instances
//! share the same compiled steps, so the heavy initialization cost (Aho-Corasick
//! compilation, page-table construction) is paid at most once per step per process.

#[cfg(feature = "runtime_build")]
use ahash::AHashMap;
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
/// ASCII-only or charwise Aho-Corasick automaton.
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
    /// Applies this step to `text`, returning a [`StepOutput`] indicating what changed.
    ///
    /// `parent_is_ascii` is the ASCII flag inherited from the parent text variant.
    /// Steps such as [`Delete`](Self::Delete) use it to skip redundant byte scans
    /// when the input is already known to be ASCII. The returned `is_ascii` flag is
    /// always authoritative for the *output* text:
    ///
    /// - **Fanjian** — always sets `is_ascii = false` (output may contain CJK).
    /// - **Delete** — ORs the parent flag with its own scan (deletion can only remove
    ///   non-ASCII chars, so if parent was ASCII the output is too).
    /// - **Normalize** — tracked incrementally during the replacement loop.
    /// - **PinYin / PinYinChar** — tracked incrementally: Pinyin replacements are
    ///   always ASCII, but unmapped characters (emoji, Korean, etc.) pass through
    ///   unchanged, so the output may contain non-ASCII bytes.
    #[inline(always)]
    pub(crate) fn apply(&self, text: &str, parent_is_ascii: bool) -> StepOutput {
        match self {
            Self::None => StepOutput::unchanged(parent_is_ascii),
            Self::Fanjian(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(parent_is_ascii),
                |changed| StepOutput::changed(changed, false),
            ),
            Self::Delete(matcher) => matcher.delete(text).map_or_else(
                || StepOutput::unchanged(parent_is_ascii),
                |(changed, is_ascii)| StepOutput::changed(changed, parent_is_ascii || is_ascii),
            ),
            Self::Normalize(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(parent_is_ascii),
                |(changed, is_ascii)| StepOutput::changed(changed, is_ascii),
            ),
            Self::PinYin(matcher) | Self::PinYinChar(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(parent_is_ascii),
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
/// The bit position of `process_type_bit` (via `trailing_zeros()`) is used as the array
/// index into [`TRANSFORM_STEP_CACHE`]. If the slot has not been initialized yet, the
/// step is compiled from either build-time artifacts or source maps (depending on the
/// `runtime_build` feature flag).
///
/// # Panics
///
/// Debug-asserts that `process_type_bit` has exactly one bit set and that the resulting
/// index is within the cache bounds. In release mode, passing a multi-bit or out-of-range
/// value is undefined behavior (array out-of-bounds).
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

/// Builds one compiled step by parsing the raw source maps shipped in `process_map/`.
///
/// This implementation is used when the `runtime_build` feature is enabled, allowing
/// transformation tables to be loaded dynamically rather than from build-time artifacts.
///
/// # Panics
///
/// Panics (via `.unwrap()`) if any line in the source map files is malformed (missing
/// tab separator or empty key/value). This is acceptable because the source maps are
/// shipped with the crate and validated at development time.
#[cfg(feature = "runtime_build")]
fn build_transform_step(process_type_bit: ProcessType) -> TransformStep {
    match process_type_bit {
        ProcessType::None => TransformStep::None,
        ProcessType::Fanjian => {
            let mut map = AHashMap::new();
            for line in FANJIAN.trim().lines() {
                let mut split = line.split('\t');
                let key = split.next().unwrap();
                let value = split.next().unwrap();
                assert!(
                    key.chars().count() == 1,
                    "FANJIAN key must be exactly one character: {key:?}"
                );
                assert!(
                    value.chars().count() == 1,
                    "FANJIAN value must be exactly one character: {value:?}"
                );
                let key = key.chars().next().unwrap() as u32;
                let value = value.chars().next().unwrap() as u32;
                if key != value {
                    map.insert(key, value);
                }
            }
            TransformStep::Fanjian(FanjianMatcher::from_map(map))
        }
        ProcessType::Delete => TransformStep::Delete(DeleteMatcher::from_sources(TEXT_DELETE)),
        ProcessType::Normalize => {
            let mut dict = AHashMap::new();
            for process_map in [NORM, NUM_NORM] {
                dict.extend(process_map.trim().lines().map(|pair| {
                    let mut split = pair.split('\t');
                    (split.next().unwrap(), split.next().unwrap())
                }));
            }
            dict.retain(|&key, value| key != *value);
            TransformStep::Normalize(NormalizeMatcher::from_dict(dict))
        }
        ProcessType::PinYin => {
            let mut map = AHashMap::new();
            for line in PINYIN.trim().lines() {
                let mut split = line.split('\t');
                let key = split.next().unwrap();
                assert!(
                    key.chars().count() == 1,
                    "PINYIN key must be exactly one character: {key:?}"
                );
                let key = key.chars().next().unwrap() as u32;
                let value = split.next().unwrap();
                assert!(
                    !value.is_empty(),
                    "PINYIN value must not be empty for key U+{key:04X}"
                );
                map.insert(key, value);
            }
            TransformStep::PinYin(PinyinMatcher::from_map(map, false))
        }
        ProcessType::PinYinChar => {
            let mut map = AHashMap::new();
            for line in PINYIN.trim().lines() {
                let mut split = line.split('\t');
                let key = split.next().unwrap();
                assert!(
                    key.chars().count() == 1,
                    "PINYIN key must be exactly one character: {key:?}"
                );
                let key = key.chars().next().unwrap() as u32;
                let value = split.next().unwrap();
                assert!(
                    !value.is_empty(),
                    "PINYIN value must not be empty for key U+{key:04X}"
                );
                map.insert(key, value);
            }
            TransformStep::PinYinChar(PinyinMatcher::from_map(map, true))
        }
        _ => unreachable!("unsupported single-bit ProcessType"),
    }
}

/// Builds one compiled step from the build-time artifacts emitted by `build.rs`.
///
/// This is the default (non-`runtime_build`) path. The artifacts are `include_bytes!` /
/// `include_str!` constants defined in [`super::transform::constants`], so initialization
/// is a deserialization rather than a full compilation.
///
/// # Panics
///
/// Panics (via `unreachable!`) if `process_type_bit` is not a recognized single-bit value.
#[cfg(not(feature = "runtime_build"))]
fn build_transform_step(process_type_bit: ProcessType) -> TransformStep {
    match process_type_bit {
        ProcessType::None => TransformStep::None,
        ProcessType::Fanjian => {
            TransformStep::Fanjian(FanjianMatcher::new(FANJIAN_L1_BYTES, FANJIAN_L2_BYTES))
        }
        ProcessType::Delete => TransformStep::Delete(DeleteMatcher::new(DELETE_BITSET_BYTES)),
        ProcessType::Normalize => TransformStep::Normalize(
            NormalizeMatcher::new(NORMALIZE_PROCESS_LIST_STR.lines())
                .with_replacements(NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect()),
        ),
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
