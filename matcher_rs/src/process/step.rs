//! Compiled transformation steps and their lazy-initialization registry.
//!
//! A [`TransformStep`] wraps one of the low-level transform engines (Fanjian, Delete,
//! Normalize, PinYin, PinYinChar) and provides a uniform [`apply`](TransformStep::apply)
//! interface. [`StepOutput`] carries the result: either `changed = None` (the text was
//! unaffected) or `changed = Some(new_string)` together with an updated `output_density` value.
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
/// not modified at all). `output_density` always describes the *post-step* text's
/// multi-byte character density (`continuation_bytes / total_bytes`), regardless of
/// whether the text changed. `output_density == 0.0` is exactly equivalent to pure ASCII.
/// Callers use this to decide whether to scan with the bytewise or charwise automaton.
pub(crate) struct StepOutput {
    /// The transformed string, or [`None`] if the step did not modify the input.
    pub(crate) changed: Option<String>,
    /// Multi-byte density of the post-step text (`continuation_bytes / total_bytes`).
    /// `0.0` for pure ASCII; propagated from parent when the step is a no-op.
    pub(crate) output_density: f32,
}

/// Constructors for [`StepOutput`].
impl StepOutput {
    /// Creates an unchanged result, propagating the caller-provided density.
    ///
    /// Used when a step determines that no characters in the input are affected by its
    /// transformation table.
    #[inline(always)]
    pub(crate) fn unchanged(density: f32) -> Self {
        Self {
            changed: None,
            output_density: density,
        }
    }

    /// Creates a changed result with the produced `String` and its density.
    #[inline(always)]
    pub(crate) fn changed(changed: String, density: f32) -> Self {
        Self {
            changed: Some(changed),
            output_density: density,
        }
    }
}

/// How one transform behaves when its input text is already known to be ASCII.
///
/// Used by [`TransformStep::is_noop_on_ascii_input`] and
/// [`TransformStep::apply`] to short-circuit work when the parent density is
/// `0.0` (pure ASCII), avoiding unnecessary page-table or automaton probes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AsciiInputBehavior {
    /// The transform is guaranteed to leave ASCII input unchanged.
    ///
    /// Applies to Fanjian (Traditional→Simplified maps only CJK codepoints),
    /// PinYin, and PinYinChar (Pinyin tables also only contain CJK entries).
    /// When `parent_density == 0.0` and this variant is returned, [`TransformStep::apply`]
    /// returns [`StepOutput::unchanged`] immediately without consulting the
    /// transform tables.
    NoOp,
    /// The transform may change ASCII input, but the output remains ASCII.
    ///
    /// Applies to Delete (the delete bitset can contain ASCII codepoints such
    /// as punctuation) and Normalize (normalization rules include full-width
    /// ASCII and special numeric forms). When this variant is returned,
    /// [`TransformStep::apply`] still runs the transform but forces
    /// `output_density = 0.0` in the returned [`StepOutput`] because ASCII
    /// input can only produce ASCII output.
    MayChangeButStaysAscii,
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
    /// Returns the behavior this step guarantees for pure-ASCII input.
    #[inline(always)]
    pub(crate) fn ascii_input_behavior(&self) -> AsciiInputBehavior {
        match self {
            Self::None | Self::Fanjian(_) | Self::PinYin(_) | Self::PinYinChar(_) => {
                AsciiInputBehavior::NoOp
            }
            Self::Delete(_) | Self::Normalize(_) => AsciiInputBehavior::MayChangeButStaysAscii,
        }
    }

    /// Returns whether this step is guaranteed to be a no-op on ASCII input.
    ///
    /// Convenience wrapper around [`ascii_input_behavior`](Self::ascii_input_behavior).
    /// Used by `walk_and_scan` to detect the no-op case for leaf nodes: when `true`,
    /// the leaf can reuse its parent's text and variant index instead of streaming
    /// a new byte iterator through the AC engine.
    #[inline(always)]
    pub(crate) fn is_noop_on_ascii_input(&self) -> bool {
        matches!(self.ascii_input_behavior(), AsciiInputBehavior::NoOp)
    }

    /// Returns the `use_bytewise` flag appropriate for scanning this step's output.
    ///
    /// The `use_bytewise` flag tells `ScanPlan::for_each_match_value` whether to
    /// route the text through the bytewise Aho-Corasick engine or the charwise one.
    ///
    /// Rules:
    /// - If the **parent is ASCII**, the parent flag is already `true` (bytewise)
    ///   and this method is never called on the hot path (ASCII no-op detection
    ///   handles it upstream).
    /// - **PinYin / PinYinChar**: always return `true`. Pinyin romanization
    ///   produces pure ASCII output regardless of the parent's CJK content,
    ///   so bytewise scanning is always correct and faster.
    /// - **All other steps**: inherit `parent_use_bytewise`. These steps may
    ///   preserve or reduce the density but cannot increase it above the parent,
    ///   so the parent's engine choice remains appropriate.
    #[inline(always)]
    pub(crate) fn output_use_bytewise(&self, parent_use_bytewise: bool) -> bool {
        match self {
            // PinYin/PinYinChar always produce ASCII romanization for non-ASCII input.
            Self::PinYin(_) | Self::PinYinChar(_) => true,
            _ => parent_use_bytewise,
        }
    }

    /// Applies this step to `text`, returning a [`StepOutput`] indicating what changed.
    ///
    /// `parent_density` is the multi-byte density (`continuation_bytes / total_bytes`)
    /// of the parent text variant. `0.0` means pure ASCII.
    ///
    /// - **Fanjian / PinYin / PinYinChar** — short-circuit to unchanged on ASCII input.
    /// - **Delete** — may change ASCII input, but ASCII stays ASCII (density = 0.0).
    /// - **Normalize** — may change ASCII input, but ASCII stays ASCII (density = 0.0).
    /// - **Non-ASCII Fanjian** — CJK→CJK substitution; byte widths are equal, so
    ///   output density equals parent density.
    /// - **Non-ASCII Delete / Normalize / PinYin / PinYinChar** — density computed by
    ///   the underlying engine and returned in [`StepOutput::output_density`].
    #[inline(always)]
    pub(crate) fn apply(&self, text: &str, parent_density: f32) -> StepOutput {
        if parent_density == 0.0 {
            return match self.ascii_input_behavior() {
                AsciiInputBehavior::NoOp => StepOutput::unchanged(0.0),
                AsciiInputBehavior::MayChangeButStaysAscii => match self {
                    Self::Delete(matcher) => matcher.delete(text).map_or_else(
                        || StepOutput::unchanged(0.0),
                        |(changed, _)| StepOutput::changed(changed, 0.0),
                    ),
                    Self::Normalize(matcher) => matcher.replace(text).map_or_else(
                        || StepOutput::unchanged(0.0),
                        |(changed, _)| StepOutput::changed(changed, 0.0),
                    ),
                    _ => unreachable!("ASCII behavior and step variant must agree"),
                },
            };
        }

        match self {
            Self::None => StepOutput::unchanged(parent_density),
            Self::Fanjian(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(parent_density),
                |changed| StepOutput::changed(changed, parent_density),
            ),
            Self::Delete(matcher) => matcher.delete(text).map_or_else(
                || StepOutput::unchanged(parent_density),
                |(changed, density)| StepOutput::changed(changed, density),
            ),
            Self::Normalize(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(parent_density),
                |(changed, density)| StepOutput::changed(changed, density),
            ),
            Self::PinYin(matcher) | Self::PinYinChar(matcher) => matcher.replace(text).map_or_else(
                || StepOutput::unchanged(parent_density),
                |(changed, density)| StepOutput::changed(changed, density),
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
