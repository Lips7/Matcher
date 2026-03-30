//! Lazy registry for compiled single-bit transformation steps.
//!
//! The registry is a fixed-size array of [`OnceLock`] slots — one per bit position in
//! [`ProcessType`]. On first access the corresponding [`TransformStep`] is compiled
//! (either from build-time artifacts or from source maps when `runtime_build` is enabled)
//! and cached for the lifetime of the process.
//!
//! All [`crate::SimpleMatcher`] instances share the same compiled steps, so the heavy
//! initialization cost (Aho-Corasick compilation, page-table construction) is paid at
//! most once per step per process.

#[cfg(feature = "runtime_build")]
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::process::process_type::ProcessType;
use crate::process::step::TransformStep;
use crate::process::transform::charwise::{FanjianMatcher, PinyinMatcher};
use crate::process::transform::constants::*;
use crate::process::transform::delete::DeleteMatcher;
use crate::process::transform::normalize::NormalizeMatcher;

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
            let mut map = HashMap::new();
            for line in FANJIAN.trim().lines() {
                let mut split = line.split('\t');
                let key = split.next().unwrap().chars().next().unwrap() as u32;
                let value = split.next().unwrap().chars().next().unwrap() as u32;
                if key != value {
                    map.insert(key, value);
                }
            }
            TransformStep::Fanjian(FanjianMatcher::from_map(map))
        }
        ProcessType::Delete => {
            TransformStep::Delete(DeleteMatcher::from_sources(TEXT_DELETE, WHITE_SPACE))
        }
        ProcessType::Normalize => {
            let mut dict = HashMap::new();
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
            let mut map = HashMap::new();
            for line in PINYIN.trim().lines() {
                let mut split = line.split('\t');
                let key = split.next().unwrap().chars().next().unwrap() as u32;
                map.insert(key, split.next().unwrap());
            }
            TransformStep::PinYin(PinyinMatcher::from_map(map, false))
        }
        ProcessType::PinYinChar => {
            let mut map = HashMap::new();
            for line in PINYIN.trim().lines() {
                let mut split = line.split('\t');
                let key = split.next().unwrap().chars().next().unwrap() as u32;
                map.insert(key, split.next().unwrap());
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
        ProcessType::Normalize => {
            #[cfg(feature = "dfa")]
            {
                TransformStep::Normalize(
                    NormalizeMatcher::new(NORMALIZE_PROCESS_LIST_STR.lines())
                        .with_replacements(NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect()),
                )
            }
            #[cfg(not(feature = "dfa"))]
            {
                TransformStep::Normalize(
                    NormalizeMatcher::deserialize(NORMALIZE_PROCESS_MATCHER_BYTES)
                        .with_replacements(NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect()),
                )
            }
        }
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
