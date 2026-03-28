//! Lazy registry for compiled single-bit transformation steps.

#[cfg(feature = "runtime_build")]
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::process::process_type::ProcessType;
use crate::process::step::TransformStep;
use crate::process::transform::charwise::{FanjianMatcher, PinyinMatcher};
use crate::process::transform::constants::*;
use crate::process::transform::delete::DeleteMatcher;
use crate::process::transform::normalize::NormalizeMatcher;

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
pub(crate) fn get_transform_step(process_type_bit: ProcessType) -> &'static TransformStep {
    let index = process_type_bit.bits().trailing_zeros() as usize;
    debug_assert!(
        index < TRANSFORM_STEP_CACHE.len(),
        "ProcessType bit index out of bounds"
    );

    TRANSFORM_STEP_CACHE[index].get_or_init(|| build_transform_step(process_type_bit))
}

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
