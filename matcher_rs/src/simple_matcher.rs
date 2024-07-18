use std::iter;
use std::{borrow::Cow, collections::HashMap};

use ahash::AHashMap;
use aho_corasick_unsafe::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use nohash_hasher::{IntMap, IntSet};
use sonic_rs::{Deserialize, Serialize};

use crate::matcher::{MatchResultTrait, TextMatcherTrait};
use crate::process::process_matcher::{
    build_process_type_tree, reduce_text_process_emit, reduce_text_process_with_tree, ProcessType,
    ProcessTypeBitNode,
};

pub type SimpleTable<'a> = IntMap<ProcessType, IntMap<u32, &'a str>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WordConf {
    word: String,
    split_bit: Vec<i32>,
    not_index: usize,
}

#[derive(Debug, Serialize)]
pub struct SimpleResult<'a> {
    pub word_id: u32,
    pub word: Cow<'a, str>,
}

impl MatchResultTrait<'_> for SimpleResult<'_> {
    fn match_id(&self) -> u32 {
        0
    }
    fn table_id(&self) -> u32 {
        0
    }
    fn word_id(&self) -> u32 {
        self.word_id
    }
    fn word(&self) -> &str {
        &self.word
    }
    fn similarity(&self) -> f64 {
        1.0
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SimpleMatcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    ac_matcher: AhoCorasick,
    ac_dedup_word_conf_list: Vec<Vec<(ProcessType, u32, usize)>>,
    word_conf_map: IntMap<u32, WordConf>,
}

impl SimpleMatcher {
    pub fn new<I, S1, S2>(
        process_type_word_map: &HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str>,
    {
        let mut process_type_list = Vec::new();
        let mut ac_dedup_word_conf_list = Vec::new();
        let mut word_conf_map = IntMap::default();

        let mut ac_dedup_word_id = 0;
        let mut ac_dedup_word_list = Vec::new();
        let mut ac_dedup_word_id_map = AHashMap::new();

        for (&process_type, simple_word_map) in process_type_word_map {
            let word_process_type = process_type - ProcessType::Delete;
            process_type_list.push(process_type);

            for (&simple_word_id, simple_word) in simple_word_map {
                let mut ac_split_word_and_counter = AHashMap::default();
                let mut ac_split_word_not_counter = AHashMap::default();

                let mut start = 0;
                let mut is_and = false;
                let mut is_not = false;

                for (index, char) in simple_word.as_ref().match_indices(['&', '~']) {
                    if (is_and || start == 0) && start != index {
                        ac_split_word_and_counter
                            // Guaranteed not failed
                            .entry(unsafe { simple_word.as_ref().get_unchecked(start..index) })
                            .and_modify(|cnt| *cnt += 1)
                            .or_insert(1);
                    }
                    if is_not && start != index {
                        ac_split_word_not_counter
                            // Guaranteed not failed
                            .entry(unsafe { simple_word.as_ref().get_unchecked(start..index) })
                            .and_modify(|cnt| *cnt -= 1)
                            .or_insert(0);
                    }
                    match char {
                        "&" => {
                            is_and = true;
                            is_not = false;
                            start = index + 1;
                        }
                        "~" => {
                            is_and = false;
                            is_not = true;
                            start = index + 1
                        }
                        _ => {}
                    }
                }
                if (is_and || start == 0) && start != simple_word.as_ref().len() {
                    ac_split_word_and_counter
                        // Guaranteed not failed
                        .entry(unsafe { simple_word.as_ref().get_unchecked(start..) })
                        .and_modify(|cnt| *cnt += 1)
                        .or_insert(1);
                }
                if is_not && start != simple_word.as_ref().len() {
                    ac_split_word_not_counter
                        // Guaranteed not failed
                        .entry(unsafe { simple_word.as_ref().get_unchecked(start..) })
                        .and_modify(|cnt| *cnt -= 1)
                        .or_insert(0);
                }

                let not_index = ac_split_word_and_counter.len();
                let split_bit = ac_split_word_and_counter
                    .values()
                    .copied()
                    .chain(ac_split_word_not_counter.values().copied())
                    .collect::<Vec<i32>>();

                word_conf_map.insert(
                    simple_word_id,
                    WordConf {
                        word: simple_word.as_ref().to_owned(),
                        split_bit,
                        not_index,
                    },
                );

                for (offset, &split_word) in ac_split_word_and_counter
                    .keys()
                    .chain(ac_split_word_not_counter.keys())
                    .enumerate()
                {
                    for ac_word in reduce_text_process_emit(word_process_type, split_word) {
                        if let Some(ac_dedup_word_id) = ac_dedup_word_id_map.get(ac_word.as_ref()) {
                            // Guaranteed not failed
                            let word_conf_list: &mut Vec<(ProcessType, u32, usize)> = unsafe {
                                ac_dedup_word_conf_list
                                    .get_unchecked_mut(*ac_dedup_word_id as usize)
                            };
                            word_conf_list.push((process_type, simple_word_id, offset));
                        } else {
                            ac_dedup_word_id_map.insert(ac_word.clone(), ac_dedup_word_id);
                            ac_dedup_word_conf_list.push(vec![(
                                process_type,
                                simple_word_id,
                                offset,
                            )]);
                            ac_dedup_word_list.push(ac_word);
                            ac_dedup_word_id += 1;
                        }
                    }
                }
            }
        }

        let process_type_tree = build_process_type_tree(&process_type_list);

        #[cfg(feature = "dfa")]
        let aho_corasick_kind = AhoCorasickKind::DFA;
        #[cfg(not(feature = "dfa"))]
        let aho_corasick_kind = AhoCorasickKind::ContiguousNFA;

        #[cfg(feature = "serde")]
        let prefilter = false;
        #[cfg(not(feature = "serde"))]
        let prefilter = true;

        let ac_matcher = AhoCorasickBuilder::new()
            .kind(Some(aho_corasick_kind))
            .ascii_case_insensitive(true)
            .prefilter(prefilter)
            .build(ac_dedup_word_list.iter().map(|ac_word| ac_word.as_ref()))
            .unwrap();

        SimpleMatcher {
            process_type_tree,
            ac_matcher,
            ac_dedup_word_conf_list,
            word_conf_map,
        }
    }
}

impl<'a> TextMatcherTrait<'a, SimpleResult<'a>> for SimpleMatcher {
    fn is_match(&'a self, text: &'a str) -> bool {
        if text.is_empty() {
            return false;
        }

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._is_match_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    fn _is_match_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, id_set::IdSet)],
    ) -> bool {
        let mut word_id_split_bit_map = IntMap::default();
        let mut word_id_set = IntSet::default();
        let mut not_word_id_set = IntSet::default();

        let processed_times = processed_text_process_type_set.len();

        for (index, (processed_text, process_type_set)) in
            processed_text_process_type_set.iter().enumerate()
        {
            // Guaranteed not failed
            for ac_dedup_result in unsafe {
                self.ac_matcher
                    .try_find_overlapping_iter(processed_text.as_ref())
                    .unwrap_unchecked()
            } {
                // Guaranteed not failed
                for &(match_process_type, word_id, offset) in unsafe {
                    self.ac_dedup_word_conf_list
                        .get_unchecked(ac_dedup_result.pattern().as_usize())
                } {
                    if !process_type_set.contains(match_process_type.bits() as usize)
                        || not_word_id_set.contains(&word_id)
                    {
                        continue;
                    }

                    // Guaranteed not failed
                    let word_conf = unsafe { self.word_conf_map.get(&word_id).unwrap_unchecked() };

                    let split_bit_matrix =
                        word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                            word_conf
                                .split_bit
                                .iter()
                                .map(|&bit| iter::repeat(bit).take(processed_times).collect())
                                .collect::<Vec<Vec<i32>>>()
                        });

                    // bit is i32, so it will not overflow almost 100%
                    unsafe {
                        let bit = split_bit_matrix
                            .get_unchecked_mut(offset)
                            .get_unchecked_mut(index);
                        *bit = bit.unchecked_add((offset < word_conf.not_index) as i32 * -2 + 1);

                        if offset >= word_conf.not_index && *bit > 0 {
                            not_word_id_set.insert(word_id);
                            word_id_set.remove(&word_id);
                            continue;
                        }

                        if split_bit_matrix
                            .iter()
                            .all(|split_bit_vec| split_bit_vec.iter().any(|&bit| bit <= 0))
                        {
                            word_id_set.insert(word_id);
                        }
                    }
                }
            }
            if !word_id_set.is_empty() {
                return true;
            }
        }

        false
    }

    fn process(&'a self, text: &'a str) -> Vec<SimpleResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._process_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    fn _process_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, id_set::IdSet)],
    ) -> Vec<SimpleResult<'a>> {
        let mut word_id_split_bit_map = IntMap::default();
        let mut not_word_id_set = IntSet::default();

        let processed_times = processed_text_process_type_set.len();

        for (index, (processed_text, process_type_set)) in
            processed_text_process_type_set.iter().enumerate()
        {
            // Guaranteed not failed
            for ac_dedup_result in unsafe {
                self.ac_matcher
                    .try_find_overlapping_iter(processed_text.as_ref())
                    .unwrap_unchecked()
            } {
                // Guaranteed not failed
                for &(match_process_type, word_id, offset) in unsafe {
                    self.ac_dedup_word_conf_list
                        .get_unchecked(ac_dedup_result.pattern().as_usize())
                } {
                    if !process_type_set.contains(match_process_type.bits() as usize)
                        || not_word_id_set.contains(&word_id)
                    {
                        continue;
                    }

                    // Guaranteed not failed
                    let word_conf = unsafe { self.word_conf_map.get(&word_id).unwrap_unchecked() };

                    let split_bit_matrix =
                        word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                            word_conf
                                .split_bit
                                .iter()
                                .map(|&bit| iter::repeat(bit).take(processed_times).collect())
                                .collect::<Vec<Vec<i32>>>()
                        });

                    // split_bit is i32, so it will not overflow almost 100%
                    unsafe {
                        let split_bit = split_bit_matrix
                            .get_unchecked_mut(offset)
                            .get_unchecked_mut(index);
                        *split_bit =
                            split_bit.unchecked_add((offset < word_conf.not_index) as i32 * -2 + 1);

                        if offset >= word_conf.not_index && *split_bit > 0 {
                            not_word_id_set.insert(word_id);
                            word_id_split_bit_map.remove(&word_id);
                        }
                    }
                }
            }
        }

        word_id_split_bit_map
            .into_iter()
            .filter_map(|(word_id, split_bit_matrix)| {
                split_bit_matrix
                    .into_iter()
                    .all(|split_bit_vec| split_bit_vec.into_iter().any(|split_bit| split_bit <= 0))
                    .then_some(SimpleResult {
                        word_id,
                        word: Cow::Borrowed(
                            // Guaranteed not failed
                            &unsafe { self.word_conf_map.get(&word_id).unwrap_unchecked() }.word,
                        ),
                    })
            })
            .collect()
    }
}
