use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;

use crate::matcher::{MatchResultTrait, TextMatcherInternal, TextMatcherTrait};
use crate::process::process_matcher::{
    ProcessType, ProcessTypeBitNode, ProcessedTextMasks, build_process_type_tree,
    reduce_text_process_emit, reduce_text_process_with_tree,
};

/// A type alias for a nested integer map structure used for mapping process types to words.
///
/// [`SimpleTable`] is a nested map where the outer map uses [`ProcessType`] as keys,
/// and the values are inner maps that map [`u32`] keys to string slices.
///
/// # Type Parameters
/// * `'a` - The lifetime of the string slices.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use matcher_rs::{SimpleTable, ProcessType};
///
/// let mut table: SimpleTable = HashMap::new();
/// table.insert(ProcessType::None, HashMap::new());
/// ```
pub type SimpleTable<'a> = HashMap<ProcessType, HashMap<u32, &'a str>>;

pub type SimpleTableSerde<'a> = HashMap<ProcessType, HashMap<u32, Cow<'a, str>>>;

/// Represents the configuration for a word within the SimpleMatcher.
///
/// [`WordConf`] contains the word as a string, the split bits indicating logical operators ('&' for AND, '~' for NOT),
/// and the index separating the 'NOT' part from the rest in the split bits vector.
///
/// # Fields
///
/// - `word_id`: The ID of the word.
/// - `word`: The original word as a String.
/// - `split_bit`: A vector of integers representing the logical splits of the word. Positive integers indicate
///   multiple occurrences of sub-strings tied to '&' operators, while negative integers correspond to '~' operators.
/// - `not_offset`: The index in `split_bit` that indicates the start of the 'NOT' split parts.
#[derive(Debug, Clone)]
struct WordConf {
    word_id: u32,
    word: String,
    split_bit: Vec<i32>,
    not_offset: usize,
}

/// Represents a simple result for matching words in the `SimpleMatcher`.
///
/// [`SimpleResult`] holds the matched word and its identifier, allowing for results to be easily accessed and utilized
/// within the matching process. The main purpose of this structure is to provide a concise and clear representation
/// of word matching outcomes.
///
/// # Type Parameters
/// * `'a` - The lifetime of the matched word. This allows [`SimpleResult`] to hold either owned `String`s or references
///   to existing `str` data, depending on the context.
///
/// # Fields
/// * `word_id` - A unique identifier for the matched word.
/// * `word` - The matched word itself, wrapped in a [`Cow`] (Clone-On-Write).
///
/// # Examples
///
/// ```rust
/// use std::borrow::Cow;
/// use matcher_rs::SimpleResult;
///
/// let result = SimpleResult {
///     word_id: 1,
///     word: Cow::Borrowed("example"),
/// };
/// assert_eq!(result.word_id, 1);
/// assert_eq!(result.word, "example");
/// ```
#[derive(Serialize, Debug)]
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
    fn similarity(&self) -> Option<f64> {
        None
    }
}

/// A single entry in the deduplicated word configuration list.
///
/// Fields: `(process_type, word_conf_idx, offset)`.
type WordConfEntry = (ProcessType, usize, usize);

/// Represents a simple matcher for processing words based on process types.
///
/// The [`SimpleMatcher`] structure is designed to perform efficient word matching, supporting logical operators
/// like AND and NOT, and allowing seamless integration with various process types. Word configurations are
/// stored and managed internally, providing a flexible and powerful matching system.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use matcher_rs::{SimpleMatcher, SimpleMatcherBuilder, ProcessType};
///
/// // Recommended: Using SimpleMatcherBuilder
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 1, "example&word")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct SimpleMatcher {
    process_type_tree: Box<[ProcessTypeBitNode]>,
    ac_matcher: AhoCorasick,
    ac_dedup_word_conf_list: Box<[Box<[WordConfEntry]>]>,
    word_conf_list: Box<[WordConf]>,
}

impl SimpleMatcher {
    /// Creates a new instance of [`SimpleMatcher`] from a given process type to word map.
    ///
    /// This method initializes the [`SimpleMatcher`] by constructing the internal structures necessary for efficient word matching.
    ///
    /// Note: It is highly recommended to use [`SimpleMatcherBuilder`](crate::SimpleMatcherBuilder)
    /// to construct a [`SimpleMatcher`] without dealing with nested HashMaps manually.
    ///
    /// # Arguments
    /// * `process_type_word_map` - A mapped Hash map structure linking [`ProcessType`] to maps of [`u32`] to word identifiers.
    ///
    /// # Type Parameters
    /// * `I` - An iterator type whose items can be converted to string slices.
    /// * `S1` - A hasher type for the inner [`HashMap`].
    /// * `S2` - A hasher type for the outer [`HashMap`].
    ///
    /// # Returns
    /// An initialized [`SimpleMatcher`] with all its internal structures set up for use.
    pub fn new<I, S1, S2>(
        process_type_word_map: &HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str>,
    {
        let word_size: usize = process_type_word_map.values().map(|m| m.len()).sum();

        let mut process_type_set = HashSet::with_capacity(process_type_word_map.len());
        let mut ac_dedup_word_conf_list = Vec::with_capacity(word_size);
        let mut word_conf_list: Vec<WordConf> = Vec::with_capacity(word_size);
        let mut word_id_to_idx: HashMap<u32, usize> = HashMap::with_capacity(word_size);

        let mut ac_dedup_word_id = 0;
        let mut ac_dedup_word_list = Vec::with_capacity(word_size);
        let mut ac_dedup_word_id_map = HashMap::with_capacity(word_size);

        for (&process_type, simple_word_map) in process_type_word_map {
            let word_process_type = process_type - ProcessType::Delete;
            process_type_set.insert(process_type.bits());

            for (&simple_word_id, simple_word) in simple_word_map {
                let mut ac_split_word_and_counter = HashMap::new();
                let mut ac_split_word_not_counter = HashMap::new();

                let mut start = 0;
                let mut is_and = false;
                let mut is_not = false;

                for (index, char) in simple_word.as_ref().match_indices(['&', '~']) {
                    if (is_and || start == 0) && start != index {
                        ac_split_word_and_counter
                            .entry(&simple_word.as_ref()[start..index])
                            .and_modify(|cnt| *cnt += 1)
                            .or_insert(1);
                    }
                    if is_not && start != index {
                        ac_split_word_not_counter
                            .entry(&simple_word.as_ref()[start..index])
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
                        .entry(&simple_word.as_ref()[start..])
                        .and_modify(|cnt| *cnt += 1)
                        .or_insert(1);
                }
                if is_not && start != simple_word.as_ref().len() {
                    ac_split_word_not_counter
                        .entry(&simple_word.as_ref()[start..])
                        .and_modify(|cnt| *cnt -= 1)
                        .or_insert(0);
                }

                let not_offset = ac_split_word_and_counter.len();
                let split_bit = ac_split_word_and_counter
                    .values()
                    .copied()
                    .chain(ac_split_word_not_counter.values().copied())
                    .collect::<Vec<i32>>();

                let word_conf_idx = if let Some(&existing_idx) = word_id_to_idx.get(&simple_word_id)
                {
                    word_conf_list[existing_idx] = WordConf {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                        split_bit,
                        not_offset,
                    };
                    existing_idx
                } else {
                    let idx = word_conf_list.len();
                    word_id_to_idx.insert(simple_word_id, idx);
                    word_conf_list.push(WordConf {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                        split_bit,
                        not_offset,
                    });
                    idx
                };

                for (offset, &split_word) in ac_split_word_and_counter
                    .keys()
                    .chain(ac_split_word_not_counter.keys())
                    .enumerate()
                {
                    for ac_word in reduce_text_process_emit(word_process_type, split_word) {
                        let Some(&ac_dedup_word_id) = ac_dedup_word_id_map.get(ac_word.as_ref())
                        else {
                            ac_dedup_word_id_map.insert(ac_word.clone(), ac_dedup_word_id);
                            ac_dedup_word_conf_list.push(vec![(
                                process_type,
                                word_conf_idx,
                                offset,
                            )]);
                            ac_dedup_word_list.push(ac_word);
                            ac_dedup_word_id += 1;
                            continue;
                        };
                        ac_dedup_word_conf_list[ac_dedup_word_id as usize].push((
                            process_type,
                            word_conf_idx,
                            offset,
                        ));
                    }
                }
            }
        }

        let process_type_tree = build_process_type_tree(&process_type_set).into_boxed_slice();

        #[cfg(feature = "dfa")]
        let aho_corasick_kind = AhoCorasickKind::DFA;
        #[cfg(not(feature = "dfa"))]
        let aho_corasick_kind = AhoCorasickKind::ContiguousNFA;

        let ac_matcher = AhoCorasickBuilder::new()
            .kind(Some(aho_corasick_kind))
            .build(ac_dedup_word_list.iter().map(|ac_word| ac_word.as_ref()))
            .unwrap();

        let ac_dedup_word_conf_list = ac_dedup_word_conf_list
            .into_iter()
            .map(|v| v.into_boxed_slice())
            .collect::<Box<[_]>>();

        SimpleMatcher {
            process_type_tree,
            ac_matcher,
            ac_dedup_word_conf_list,
            word_conf_list: word_conf_list.into_boxed_slice(),
        }
    }

    /// Core matching logic for `SimpleMatcher`, processing multiple text variants and process types.
    ///
    /// This function scans the provided processed text variants using the internal Aho-Corasick automaton.
    /// It keeps track of sub-pattern matches (AND logic `&`) and handles exclusions (NOT logic `~`).
    /// The returned data structure maps each `word_id` to a nested vector tracking which split-bits
    /// matched across the different text variants.
    ///
    /// # Algorithm
    ///
    /// 1. Iterate over each tuple of `(processed_text, process_type_mask)`.
    /// 2. Use `find_overlapping_iter` with the internal Aho-Corasick automaton to locate *all*
    ///    sub-pattern matches within the `processed_text`.
    /// 3. For each sub-pattern match, check if its [`ProcessType`] aligns with the current text variant's `process_type_mask`.
    /// 4. Maintain a 2D split-bit matrix for each `word_id` to record which tokens condition the text satisfies.
    ///    - **AND Tokens (`&`)**: Decrements their state towards `< 0`. The token count dictates how negative it goes.
    ///      Every time the exact sub-pattern occurs, it brings the count closer.
    ///    - **NOT Tokens (`~`)**: Checks if they exist (offset >= `not_offset`). If a NOT token appears,
    ///      the `word_id` is disqualified and immediately discarded from further checks using `not_word_id_set`.
    /// 5. Return the map of matched patterns which is later used in *Pass 2* to evaluate conditions.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple
    ///   contains a processed text piece (as [`Cow<str>`]) and a
    ///   u64 bitmask of process type IDs (`u64`).
    ///
    /// # Returns
    ///
    /// * `Vec<(usize, Vec<i32>)>` - A list of `(word_conf_idx, flat_split_bit_matrix)` pairs
    ///   for matched patterns, used in pass 2 to evaluate complex AND/NOT logic conditions.
    ///   The flat matrix has layout `[num_splits × processed_times]` with stride = `processed_times`.
    fn _word_match_with_processed_text_process_type_masks<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<(usize, Vec<i32>)> {
        let mut split_bit_store: FxHashMap<usize, Vec<i32>> =
            FxHashMap::with_capacity_and_hasher(16, Default::default());
        let mut not_word_id_set: FxHashSet<usize> = FxHashSet::default();

        let processed_times = processed_text_process_type_masks.len();

        for (index, (processed_text, process_type_mask)) in
            processed_text_process_type_masks.iter().enumerate()
        {
            let ac_iter = self
                .ac_matcher
                .find_overlapping_iter(processed_text.as_ref());
            for ac_dedup_result in ac_iter {
                let pattern_idx = ac_dedup_result.pattern().as_usize();
                for &(match_process_type, word_conf_idx, offset) in
                    &self.ac_dedup_word_conf_list[pattern_idx]
                {
                    if process_type_mask & (1u64 << match_process_type.bits()) == 0
                        || not_word_id_set.contains(&word_conf_idx)
                    {
                        continue;
                    }

                    let word_conf = &self.word_conf_list[word_conf_idx];

                    let flat_matrix = split_bit_store.entry(word_conf_idx).or_insert_with(|| {
                        let num_splits = word_conf.split_bit.len();
                        let mut flat = vec![0i32; num_splits * processed_times];
                        for (s, &bit) in word_conf.split_bit.iter().enumerate() {
                            let row_start = s * processed_times;
                            for t in 0..processed_times {
                                flat[row_start + t] = bit;
                            }
                        }
                        flat
                    });

                    let bit = &mut flat_matrix[offset * processed_times + index];
                    *bit += (offset < word_conf.not_offset) as i32 * -2 + 1;

                    if offset >= word_conf.not_offset && *bit > 0 {
                        not_word_id_set.insert(word_conf_idx);
                        split_bit_store.remove(&word_conf_idx);
                    }
                }
            }
        }

        split_bit_store.into_iter().collect()
    }
}

impl<'a> TextMatcherTrait<'a, SimpleResult<'a>> for SimpleMatcher {
    /// Determines if the given text matches any pattern.
    ///
    /// This function first checks if the provided text is empty. If it is, the function
    /// immediately returns `false`. Otherwise, it processes the text using a process type
    /// tree to reduce the text, then checks for matches with the processed text and
    /// associated process types.
    ///
    /// # Arguments
    /// * `text` - A string slice that holds the text to be matched.
    ///
    /// # Returns
    /// `true` if the text matches any pattern, otherwise `false`.
    fn is_match(&'a self, text: &'a str) -> bool {
        if text.is_empty() {
            return false;
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.is_match_preprocessed(&processed_text_process_type_masks)
    }
    /// Processes the given text and returns a vector of matching results.
    ///
    /// This function applies the process type tree to the text and passes the processed text
    /// to the matching implementation.
    ///
    /// # Arguments
    /// * `text` - A string slice that needs to be processed.
    ///
    /// # Returns
    /// A [`Vec<SimpleResult>`] containing the matching results.
    fn process(&'a self, text: &'a str) -> Vec<SimpleResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.process_preprocessed(&processed_text_process_type_masks)
    }

    /// Processes the given text and returns an iterator over [`SimpleResult`] matches.
    ///
    /// # Details
    /// The Aho-Corasick automaton with AND/NOT logical operators requires a **two-pass** algorithm:
    ///
    /// - **Pass 1** (scan): Traverse the entire input and accumulate the `word_id_split_bit_map`
    ///   (counting which sub-patterns were seen) and the `not_word_id_set` (patterns that triggered
    ///   a NOT-exclusion). A NOT-exclusion token can appear *after* a positive match token anywhere
    ///   in the text, so no result can be emitted until the full scan is complete.
    ///
    /// - **Pass 2** (emit): Walk `word_id_split_bit_map` and yield entries whose split-bit
    ///   matrices satisfy the AND conditions.
    ///
    /// Returns a consuming iterator over the underlying `Vec`.
    fn process_iter(&'a self, text: &'a str) -> impl Iterator<Item = SimpleResult<'a>> + 'a {
        self.process(text).into_iter()
    }
}

impl<'a> TextMatcherInternal<'a, SimpleResult<'a>> for SimpleMatcher {
    /// Checks if any pattern matches the processed text.
    ///
    /// This function processes the text with the given process type set and checks for
    /// matches. It maintains bitmaps to keep track of word IDs that are matched and
    /// potentially excluded (i.e., words that should not be in the matched set). The function
    /// iterates over the processed text, updates the split bitmaps and sets, and finally determines
    /// if any word ID set contains a match.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple
    ///   contains a processed text piece (as [`Cow<str>`]) and a
    ///   u64 bitmask of process type IDs (`u64`).
    ///
    /// # Returns
    ///
    /// * `true` if any pattern matches the processed text, otherwise `false`.
    fn is_match_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> bool {
        let matched = self
            ._word_match_with_processed_text_process_type_masks(processed_text_process_type_masks);
        let processed_times = processed_text_process_type_masks.len();

        matched.iter().any(|(word_conf_idx, flat_matrix)| {
            let num_splits = self.word_conf_list[*word_conf_idx].split_bit.len();
            (0..num_splits).all(|s| {
                flat_matrix[s * processed_times..(s + 1) * processed_times]
                    .iter()
                    .any(|&bit| bit <= 0)
            })
        })
    }

    /// Processes the given processed text and type sets to produce matching results.
    ///
    /// This function examines the provided processed text along with their corresponding ID sets
    /// and computes results by finding overlapping patterns using an Aho-Corasick matcher. The function
    /// maintains internal sets and maps to track which word IDs are relevant based on the processing types.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple
    ///   contains a processed text piece (as [`Cow<str>`]) and a
    ///   u64 bitmask of process type IDs (`u64`).
    ///
    /// # Returns
    ///
    /// * A vector of [`SimpleResult`] containing the word ID and the matched word for each successful match found. If no matches are found, it returns an empty vector.
    ///
    /// # Panics
    ///
    /// If the internal invariants are violated, the function may cause undefined behavior or panic.
    ///
    /// For example, if `processed_text_process_type_masks` has invalid data or the internal Aho-Corasick matcher
    /// encounters unexpected states, this could lead to issues.
    fn process_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<SimpleResult<'a>> {
        let matched = self
            ._word_match_with_processed_text_process_type_masks(processed_text_process_type_masks);
        let processed_times = processed_text_process_type_masks.len();

        matched
            .into_iter()
            .filter_map(|(word_conf_idx, flat_matrix)| {
                let word_conf = &self.word_conf_list[word_conf_idx];
                let num_splits = word_conf.split_bit.len();
                (0..num_splits)
                    .all(|s| {
                        flat_matrix[s * processed_times..(s + 1) * processed_times]
                            .iter()
                            .any(|&bit| bit <= 0)
                    })
                    .then_some(SimpleResult {
                        word_id: word_conf.word_id,
                        word: Cow::Borrowed(&word_conf.word),
                    })
            })
            .collect()
    }
}
