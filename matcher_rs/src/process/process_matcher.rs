use std::borrow::Cow;
use std::fmt::Display;
use std::sync::Arc;

#[cfg(any(feature = "runtime_build", feature = "dfa"))]
use ahash::AHashMap;
use ahash::HashMapExt;
use aho_corasick_unsafe::AhoCorasick;
#[cfg(any(feature = "runtime_build", feature = "dfa"))]
use aho_corasick_unsafe::{AhoCorasickBuilder, AhoCorasickKind, MatchKind as AhoCorasickMatchKind};
use bitflags::bitflags;
#[cfg(not(feature = "runtime_build"))]
use daachorse::CharwiseDoubleArrayAhoCorasick;
#[cfg(feature = "runtime_build")]
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
};
use id_set::IdSet;
use lazy_static::lazy_static;
use nohash_hasher::{IntMap, IsEnabled};
use parking_lot::RwLock;
use serde::{Deserializer, Serializer};
use sonic_rs::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::process::constants::*;

bitflags! {
    #[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Default)]
    pub struct ProcessType: u8 {
        const None = 0b00000001;
        const Fanjian = 0b00000010;
        const Delete = 0b00000100;
        const Normalize = 0b00001000;
        const DeleteNormalize = 0b00001100;
        const FanjianDeleteNormalize = 0b00001110;
        const PinYin = 0b00010000;
        const PinYinChar = 0b00100000;
    }
}

impl Serialize for ProcessType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcessType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bits: u8 = u8::deserialize(deserializer)?;
        Ok(ProcessType::from_bits_retain(bits))
    }
}

impl Display for ProcessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let display_str_list = self
            .iter_names()
            .map(|(name, _)| name.to_lowercase())
            .collect::<Vec<_>>();
        write!(f, "{:?}", display_str_list.join("_"))
    }
}

impl IsEnabled for ProcessType {}

type ProcessMatcherCache = RwLock<IntMap<ProcessType, Arc<(Vec<&'static str>, ProcessMatcher)>>>;

lazy_static! {
    pub static ref PROCESS_MATCHER_CACHE: ProcessMatcherCache =
        RwLock::new(IntMap::with_capacity(8));
}

#[derive(Clone)]
pub enum ProcessMatcher {
    #[cfg(not(feature = "dfa"))]
    LeftMost(CharwiseDoubleArrayAhoCorasick<u32>),
    Chinese(CharwiseDoubleArrayAhoCorasick<u32>),
    Others(AhoCorasick),
}

impl ProcessMatcher {
    #[inline(always)]
    pub fn replace_all<'a>(
        &self,
        text: &'a str,
        process_replace_list: &[&str],
    ) -> (bool, Cow<'a, str>) {
        let mut result = String::with_capacity(text.len());
        let mut last_end = 0;
        match self {
            #[cfg(not(feature = "dfa"))]
            ProcessMatcher::LeftMost(ac) => {
                for mat in ac.leftmost_find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    // Guaranteed not failed
                    result.push_str(unsafe {
                        process_replace_list.get_unchecked(mat.value() as usize)
                    });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Chinese(ac) => {
                for mat in ac.find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    // Guaranteed not failed
                    result.push_str(unsafe {
                        process_replace_list.get_unchecked(mat.value() as usize)
                    });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Others(ac) => {
                for mat in ac.find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    // Guaranteed not failed
                    result.push_str(unsafe {
                        process_replace_list.get_unchecked(mat.pattern().as_usize())
                    });
                    last_end = mat.end();
                }
            }
        }

        if last_end > 0 {
            // Guaranteed not failed
            result.push_str(unsafe { text.get_unchecked(last_end..) });
            (true, Cow::Owned(result))
        } else {
            (false, Cow::Borrowed(text))
        }
    }

    #[inline(always)]
    pub fn delete_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
        let mut result = String::with_capacity(text.len());
        let mut last_end = 0;
        match self {
            #[cfg(not(feature = "dfa"))]
            ProcessMatcher::LeftMost(ac) => {
                for mat in ac.leftmost_find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Chinese(ac) => {
                for mat in ac.find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Others(ac) => {
                for mat in ac.find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    last_end = mat.end();
                }
            }
        }

        if last_end > 0 {
            // Guaranteed not failed
            result.push_str(unsafe { text.get_unchecked(last_end..) });
            (true, Cow::Owned(result))
        } else {
            (false, Cow::Borrowed(text))
        }
    }
}

pub fn get_process_matcher(
    process_type_bit: ProcessType,
) -> Arc<(Vec<&'static str>, ProcessMatcher)> {
    {
        let process_matcher_cache = PROCESS_MATCHER_CACHE.read();

        if let Some(cached_result) = process_matcher_cache.get(&process_type_bit) {
            return Arc::clone(cached_result);
        }
    }

    #[cfg(feature = "runtime_build")]
    {
        let mut process_dict = AHashMap::default();

        match process_type_bit {
            ProcessType::None => {}
            ProcessType::Fanjian => {
                process_dict.extend(FANJIAN.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }
            ProcessType::Delete => {
                process_dict.extend(TEXT_DELETE.trim().lines().map(|pair_str| (pair_str, "")));
                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }
            ProcessType::Normalize => {
                for process_map in [NORM, NUM_NORM] {
                    process_dict.extend(process_map.trim().lines().map(|pair_str| {
                        let mut pair_str_split = pair_str.split('\t');
                        (
                            pair_str_split.next().unwrap(),
                            pair_str_split.next().unwrap(),
                        )
                    }));
                }
            }
            ProcessType::PinYin => {
                process_dict.extend(PINYIN.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }
            ProcessType::PinYinChar => {
                process_dict.extend(PINYIN.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap().trim_matches(' '),
                    )
                }));
            }
            _ => {}
        }

        process_dict.retain(|&key, &mut value| key != value);

        let (process_replace_list, process_matcher) = match process_type_bit {
            ProcessType::Fanjian | ProcessType::PinYin | ProcessType::PinYinChar => (
                process_dict.iter().map(|(_, &val)| val).collect(),
                ProcessMatcher::Chinese(
                    CharwiseDoubleArrayAhoCorasickBuilder::new()
                        .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                        .build(
                            process_dict
                                .iter()
                                .map(|(&key, _)| key)
                                .collect::<Vec<&str>>(),
                        )
                        .unwrap(),
                ),
            ),
            #[cfg(not(feature = "dfa"))]
            ProcessType::Delete | ProcessType::Normalize => (
                process_dict.iter().map(|(_, &val)| val).collect(),
                ProcessMatcher::LeftMost(
                    CharwiseDoubleArrayAhoCorasickBuilder::new()
                        .match_kind(DoubleArrayAhoCorasickMatchKind::LeftmostLongest)
                        .build(
                            process_dict
                                .iter()
                                .map(|(&key, _)| key)
                                .collect::<Vec<&str>>(),
                        )
                        .unwrap(),
                ),
            ),
            _ => (
                process_dict.iter().map(|(_, &val)| val).collect(),
                ProcessMatcher::Others(
                    AhoCorasickBuilder::new()
                        .kind(Some(AhoCorasickKind::DFA))
                        .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                        .build(
                            process_dict
                                .iter()
                                .map(|(&key, _)| key)
                                .collect::<Vec<&str>>(),
                        )
                        .unwrap(),
                ),
            ),
        };
        let uncached_result = Arc::new((process_replace_list, process_matcher));
        let mut process_matcher_cache = PROCESS_MATCHER_CACHE.write();
        process_matcher_cache.insert(process_type_bit, Arc::clone(&uncached_result));
        return uncached_result;
    }

    #[cfg(not(feature = "runtime_build"))]
    {
        let (process_replace_list, process_matcher) = match process_type_bit {
            ProcessType::None => {
                let empty_patterns: Vec<&str> = Vec::new();
                (
                    Vec::new(),
                    ProcessMatcher::Others(AhoCorasick::new(&empty_patterns).unwrap()),
                )
            }
            ProcessType::Fanjian => (
                FANJIAN_PROCESS_REPLACE_LIST_STR.lines().collect(),
                // Guaranteed not failed
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                        FANJIAN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),
            ProcessType::Delete => {
                #[cfg(feature = "dfa")]
                {
                    let mut process_dict = AHashMap::default();
                    process_dict.extend(TEXT_DELETE.trim().lines().map(|pair_str| (pair_str, "")));
                    process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
                    process_dict.retain(|&key, &mut value| key != value);
                    let process_list = process_dict
                        .iter()
                        .map(|(&key, _)| key)
                        .collect::<Vec<&str>>();

                    (
                        Vec::new(),
                        ProcessMatcher::Others(
                            AhoCorasickBuilder::new()
                                .kind(Some(AhoCorasickKind::DFA))
                                .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                                .build(&process_list)
                                .unwrap(),
                        ),
                    )
                }
                #[cfg(not(feature = "dfa"))]
                {
                    (
                        Vec::new(),
                        ProcessMatcher::LeftMost(unsafe {
                            CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                                TEXT_DELETE_PROCESS_MATCHER_BYTES,
                            )
                            .0
                        }),
                    )
                }
            }
            ProcessType::Normalize => {
                #[cfg(feature = "dfa")]
                {
                    (
                        NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                        ProcessMatcher::Others(
                            AhoCorasickBuilder::new()
                                .kind(Some(AhoCorasickKind::DFA))
                                .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                                .build(NORMALIZE_PROCESS_LIST_STR.lines())
                                .unwrap(),
                        ),
                    )
                }
                #[cfg(not(feature = "dfa"))]
                {
                    (
                        NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                        ProcessMatcher::LeftMost(unsafe {
                            CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                                NORMALIZE_PROCESS_MATCHER_BYTES,
                            )
                            .0
                        }),
                    )
                }
            }
            ProcessType::PinYin => (
                PINYIN_PROCESS_REPLACE_LIST_STR.lines().collect(),
                // Guaranteed not failed
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                        PINYIN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),
            ProcessType::PinYinChar => (
                PINYINCHAR_PROCESS_REPLACE_LIST_STR.lines().collect(),
                // Guaranteed not failed
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                        PINYIN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),
            _ => unreachable!(),
        };

        let uncached_result = Arc::new((process_replace_list, process_matcher));
        let mut process_matcher_cache = PROCESS_MATCHER_CACHE.write();
        process_matcher_cache.insert(process_type_bit, Arc::clone(&uncached_result));
        uncached_result
    }
}

#[inline(always)]
pub fn text_process(
    process_type_bit: ProcessType,
    text: &str,
) -> Result<Cow<'_, str>, &'static str> {
    if process_type_bit.iter().count() > 1 {
        return Err("text_process function only accept one bit of process_type");
    }

    let cached_result = get_process_matcher(process_type_bit);
    let (process_replace_list, process_matcher) = cached_result.as_ref();
    let mut result = Cow::Borrowed(text);
    match (process_type_bit, process_matcher) {
        (ProcessType::None, _) => {}
        (ProcessType::Fanjian, pm) => match pm.replace_all(text, process_replace_list) {
            (true, Cow::Owned(pt)) => {
                result = Cow::Owned(pt);
            }
            (false, _) => {}
            (_, _) => unreachable!(),
        },
        (ProcessType::Delete, pm) => match pm.delete_all(text) {
            (true, Cow::Owned(pt)) => {
                result = Cow::Owned(pt);
            }
            (false, _) => {}
            (_, _) => unreachable!(),
        },
        (_, pm) => match pm.replace_all(text, process_replace_list) {
            (true, Cow::Owned(pt)) => {
                result = Cow::Owned(pt);
            }
            (false, _) => {}
            (_, _) => unreachable!(),
        },
    };
    Ok(result)
}

#[inline(always)]
pub fn reduce_text_process<'a>(
    process_type: ProcessType,
    text: &'a str,
) -> ArrayVec<[Cow<'a, str>; 8]> {
    let mut processed_text_list: ArrayVec<[Cow<'a, str>; 8]> = ArrayVec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let cached_result = get_process_matcher(process_type_bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();
        // Guaranteed not failed
        let tmp_processed_text = unsafe { processed_text_list.last_mut().unwrap_unchecked() };

        match (process_type_bit, process_matcher) {
            (ProcessType::None, _) => {}
            (ProcessType::Delete, pm) => match pm.delete_all(tmp_processed_text.as_ref()) {
                (true, Cow::Owned(pt)) => {
                    processed_text_list.push(Cow::Owned(pt));
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
            (_, pm) => match pm.replace_all(tmp_processed_text.as_ref(), process_replace_list) {
                (true, Cow::Owned(pt)) => {
                    processed_text_list.push(Cow::Owned(pt));
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
        }
    }

    processed_text_list
}

#[inline(always)]
pub fn reduce_text_process_emit<'a>(
    process_type: ProcessType,
    text: &'a str,
) -> ArrayVec<[Cow<'a, str>; 8]> {
    let mut processed_text_list: ArrayVec<[Cow<'a, str>; 8]> = ArrayVec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let cached_result = get_process_matcher(process_type_bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();
        // Guaranteed not failed
        let tmp_processed_text = unsafe { processed_text_list.last_mut().unwrap_unchecked() };

        match (process_type_bit, process_matcher) {
            (ProcessType::None, _) => {}
            (ProcessType::Delete, pm) => match pm.delete_all(tmp_processed_text.as_ref()) {
                (true, Cow::Owned(pt)) => {
                    processed_text_list.push(Cow::Owned(pt));
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
            (_, pm) => match pm.replace_all(tmp_processed_text.as_ref(), process_replace_list) {
                (true, Cow::Owned(pt)) => {
                    *tmp_processed_text = Cow::Owned(pt);
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
        }
    }

    processed_text_list
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProcessTypeBitNode {
    process_type_list: ArrayVec<[ProcessType; 8]>,
    process_type_bit: ProcessType,
    is_processed: bool,
    processed_text_index: usize,
    children: ArrayVec<[usize; 8]>,
}

pub fn build_process_type_tree(process_type_list: &[ProcessType]) -> Vec<ProcessTypeBitNode> {
    let mut process_type_tree = Vec::new();
    let root = ProcessTypeBitNode {
        process_type_list: ArrayVec::new(),
        process_type_bit: ProcessType::None,
        is_processed: true,
        processed_text_index: 0,
        children: ArrayVec::new(),
    };
    process_type_tree.push(root);
    for &process_type in process_type_list.iter() {
        let mut current_node_index = 0;
        for process_type_bit in process_type.iter() {
            let current_node = process_type_tree[current_node_index];
            if current_node.process_type_bit == process_type_bit {
                continue;
            }

            let mut is_found = false;
            for child_node_index in current_node.children {
                if process_type_bit == process_type_tree[child_node_index].process_type_bit {
                    current_node_index = child_node_index;
                    is_found = true;
                    break;
                }
            }

            if !is_found {
                let mut child = ProcessTypeBitNode {
                    process_type_list: ArrayVec::new(),
                    process_type_bit,
                    is_processed: false,
                    processed_text_index: 0,
                    children: ArrayVec::new(),
                };
                child.process_type_list.push(process_type);
                process_type_tree.push(child);
                let new_node_index = process_type_tree.len() - 1;
                process_type_tree[current_node_index]
                    .children
                    .push(new_node_index);
                current_node_index = new_node_index;
            } else {
                process_type_tree[current_node_index]
                    .process_type_list
                    .push(process_type);
            }
        }
    }
    process_type_tree
}

#[inline(always)]
pub fn reduce_text_process_with_tree<'a>(
    process_type_tree: &[ProcessTypeBitNode],
    text: &'a str,
) -> ArrayVec<[(Cow<'a, str>, IdSet); 16]> {
    let mut process_type_tree_copied: Vec<ProcessTypeBitNode> = process_type_tree.to_vec();

    let mut processed_text_process_type_set: ArrayVec<[(Cow<'a, str>, IdSet); 16]> =
        ArrayVec::new();
    processed_text_process_type_set.push((
        Cow::Borrowed(text),
        IdSet::from_iter([ProcessType::None.bits() as usize]),
    ));

    for (current_node_index, current_node) in process_type_tree.iter().enumerate() {
        let (left_tree, right_tree) = unsafe {
            process_type_tree_copied.split_at_mut_unchecked(current_node_index.unchecked_add(1))
        };

        let current_copied_node = unsafe { left_tree.get_unchecked(current_node_index) };
        let mut current_index = current_copied_node.processed_text_index;
        let current_text_ptr =
            unsafe { processed_text_process_type_set.get_unchecked(current_index) }
                .0
                .as_ref() as *const str;

        for child_node_index in current_node.children {
            let child_node = unsafe {
                right_tree.get_unchecked_mut(
                    child_node_index
                        .unchecked_sub(current_node_index)
                        .unchecked_sub(1),
                )
            };

            if child_node.is_processed {
                current_index = current_copied_node.processed_text_index;
            } else {
                let cached_result = get_process_matcher(child_node.process_type_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match child_node.process_type_bit {
                    ProcessType::None => {}
                    ProcessType::Delete => {
                        match process_matcher.delete_all(unsafe { &*current_text_ptr }) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_process_type_set.push((
                                    Cow::Owned(pt),
                                    IdSet::from_iter(
                                        child_node
                                            .process_type_list
                                            .iter()
                                            .map(|smt| smt.bits() as usize),
                                    ),
                                ));
                                current_index = unsafe {
                                    processed_text_process_type_set.len().unchecked_sub(1)
                                };
                            }
                            (false, _) => {
                                current_index = current_copied_node.processed_text_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                    _ => match process_matcher
                        .replace_all(unsafe { &*current_text_ptr }, process_replace_list)
                    {
                        (true, Cow::Owned(pt)) => {
                            processed_text_process_type_set.push((Cow::Owned(pt), IdSet::new()));
                            current_index =
                                unsafe { processed_text_process_type_set.len().unchecked_sub(1) };
                        }
                        (false, _) => {
                            current_index = current_copied_node.processed_text_index;
                        }
                        (_, _) => unreachable!(),
                    },
                }
                child_node.is_processed = true;
            }

            child_node.processed_text_index = current_index;
            let processed_text_process_type_tuple =
                unsafe { processed_text_process_type_set.get_unchecked_mut(current_index) };
            processed_text_process_type_tuple.1.extend(
                child_node
                    .process_type_list
                    .iter()
                    .map(|smt| smt.bits() as usize),
            );
        }
    }

    processed_text_process_type_set
}

#[inline(always)]
pub fn reduce_text_process_with_list<'a>(
    process_type_list: &[ProcessType],
    text: &'a str,
) -> ArrayVec<[(Cow<'a, str>, IdSet); 16]> {
    let mut process_type_tree = Vec::with_capacity(8);
    let mut root = ProcessTypeBitNode {
        process_type_list: ArrayVec::new(),
        process_type_bit: ProcessType::None,
        is_processed: true,
        processed_text_index: 0,
        children: ArrayVec::new(),
    };
    root.process_type_list.push(ProcessType::None);
    process_type_tree.push(root);

    let mut processed_text_process_type_set: ArrayVec<[(Cow<'a, str>, IdSet); 16]> =
        ArrayVec::new();
    processed_text_process_type_set.push((
        Cow::Borrowed(text),
        IdSet::from_iter([ProcessType::None.bits() as usize]),
    ));

    for &process_type in process_type_list.iter() {
        let mut current_text = text;
        let mut current_index = 0;
        let mut current_node_index = 0;

        for process_type_bit in process_type.iter() {
            let current_node = unsafe { process_type_tree.get_unchecked(current_node_index) };
            if current_node.process_type_bit == process_type_bit {
                continue;
            }

            let mut is_found = false;
            for child_node_index in current_node.children {
                if process_type_bit
                    == unsafe { process_type_tree.get_unchecked(child_node_index) }.process_type_bit
                {
                    current_node_index = child_node_index;
                    is_found = true;
                    break;
                }
            }
            let current_node = unsafe { process_type_tree.get_unchecked_mut(current_node_index) };

            if !is_found {
                let cached_result = get_process_matcher(process_type_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match process_type_bit {
                    ProcessType::None => {}
                    ProcessType::Delete => match process_matcher.delete_all(current_text) {
                        (true, Cow::Owned(pt)) => {
                            processed_text_process_type_set.push((Cow::Owned(pt), IdSet::new()));
                            current_index = processed_text_process_type_set.len() - 1;

                            let processed_text_process_type_tuple = unsafe {
                                processed_text_process_type_set
                                    .get_unchecked_mut(current_node.processed_text_index)
                            };
                            processed_text_process_type_tuple
                                .1
                                .insert(process_type.bits() as usize);
                        }
                        (false, _) => {
                            current_index = current_node.processed_text_index;
                        }
                        (_, _) => unreachable!(),
                    },
                    _ => match process_matcher.replace_all(current_text, process_replace_list) {
                        (true, Cow::Owned(pt)) => {
                            processed_text_process_type_set.push((Cow::Owned(pt), IdSet::new()));
                            current_index = processed_text_process_type_set.len() - 1;
                        }
                        (false, _) => {
                            current_index = current_node.processed_text_index;
                        }
                        (_, _) => unreachable!(),
                    },
                }

                let mut child = ProcessTypeBitNode {
                    process_type_list: ArrayVec::new(),
                    process_type_bit,
                    is_processed: true,
                    processed_text_index: current_index,
                    children: ArrayVec::new(),
                };
                child.process_type_list.push(process_type);
                process_type_tree.push(child);

                let new_node_index = process_type_tree.len() - 1;
                let current_node =
                    unsafe { process_type_tree.get_unchecked_mut(current_node_index) };
                current_node.children.push(new_node_index);
                current_node_index = new_node_index;
            } else {
                current_index = current_node.processed_text_index;
                current_node.process_type_list.push(process_type);
            }

            let processed_text_process_type_tuple =
                unsafe { processed_text_process_type_set.get_unchecked_mut(current_index) };
            processed_text_process_type_tuple
                .1
                .insert(process_type.bits() as usize);
            current_text = unsafe { processed_text_process_type_set.get_unchecked(current_index) }
                .0
                .as_ref();
        }
    }

    processed_text_process_type_set
}
