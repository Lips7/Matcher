use std::borrow::Cow;
use std::collections::HashMap;

use aho_corasick_unsafe::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use id_set::IdSet;
use nohash_hasher::IntMap;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::matcher::{MatchResultTrait, TextMatcherTrait};
use crate::process::process_matcher::{
    build_process_type_tree, reduce_text_process_emit, reduce_text_process_with_tree, ProcessType,
    ProcessTypeBitNode,
};

/// A type alias for a nested integer map structure used for mapping process types to words.
///
/// [SimpleTable] is a nested map where the outer map uses [ProcessType] as keys,
/// and the values are inner maps that map `u32` keys to string slices.
///
/// # Type Parameters
///
/// - `'a`: The lifetime of the string slices.
///
/// # Example
///
/// ```rust
/// use nohash_hasher::IntMap;
///
/// use matcher_rs::{SimpleTable, ProcessType};
///
/// let mut table: SimpleTable = IntMap::default();
/// table.insert(ProcessType::None, IntMap::default());
/// table.get_mut(&ProcessType::None).unwrap().insert(1, "example");
/// ```
///
/// The above example creates a [SimpleTable], inserts an inner map for a process type,
/// and then adds a mapping from an integer key to a string slice within that inner map.
pub type SimpleTable<'a> = IntMap<ProcessType, IntMap<u32, &'a str>>;

pub type SimpleTableSerde<'a> = IntMap<ProcessType, IntMap<u32, Cow<'a, str>>>;

/// Represents the configuration for a word within the SimpleMatcher.
///
/// [WordConf] contains the word as a string, the split bits indicating logical operators ('&' for AND, '~' for NOT),
/// and the index separating the 'NOT' part from the rest in the split bits vector.
///
/// # Fields
///
/// - `word`: The original word as a String.
/// - `split_bit`: A vector of integers representing the logical splits of the word. Positive integers indicate
///   multiple occurrences of sub-strings tied to '&' operators, while negative integers correspond to '~' operators.
/// - `not_offset`: The index in `split_bit` that indicates the start of the 'NOT' split parts.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WordConf {
    word: String,
    split_bit: Vec<i32>,
    not_offset: usize,
}

/// Represents a simple result for matching words in the SimpleMatcher.
///
/// [SimpleResult] holds the matched word and its identifier, allowing for results to be easily accessed and utilized
/// within the matching process. The main purpose of this structure is to provide a concise and clear representation
/// of word matching outcomes.
///
/// # Fields
///
/// - `word_id`: A unique identifier for the matched word.
/// - `word`: The matched word itself, wrapped in a [Cow] (Clone-On-Write) to optimize for performance and memory usage.
///
/// # Type Parameters
///
/// - `'a`: The lifetime of the matched word. This allows [SimpleResult] to hold either owned [String]s or references
///   to existing `str` data, depending on the context.
///
/// # Example
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
///
/// The example above demonstrates creating a [SimpleResult] with a borrowed `str`. The same structure can also
/// hold an owned [String] if necessary to accommodate different use cases and data lifetimes.
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

/// Represents a simple matcher for processing words based on process types.
///
/// The [SimpleMatcher] structure is designed to perform efficient word matching, supporting logical operators
/// like AND and NOT, and allowing seamless integration with various process types. Word configurations are
/// stored and managed internally, providing a flexible and powerful matching system.
///
/// The structure supports optional serialization and deserialization if the "serde" feature is enabled.
///
/// # Fields
///
/// - `process_type_tree`: A vector containing the process type tree, represented as `ProcessTypeBitNode`s.
/// - `ac_matcher`: An instance of the [AhoCorasick] matcher for efficient multi-pattern searching.
/// - `ac_dedup_word_conf_list`: A nested vector holding the deduplicated word configurations mapped to their process types.
/// - `word_conf_map`: A map of word identifiers to `WordConf` structs, storing the original word configurations.
///
/// # Example
///
/// This example demonstrates creating a [SimpleMatcher] instance using the `new` method with a sample
/// `process_type_word_map`:
///
/// ```rust
/// use std::collections::HashMap;
/// use matcher_rs::{SimpleMatcher, ProcessType};
///
/// // Define a mock process_type_word_map for demonstration
/// let mut process_type_word_map: HashMap<ProcessType, HashMap<u32, &str>> = HashMap::new();
/// let mut inner_map: HashMap<u32, &str> = HashMap::new();
/// inner_map.insert(1, "example&word");
/// process_type_word_map.insert(ProcessType::None, inner_map);
///
/// // Creating a SimpleMatcher instance
/// let matcher = SimpleMatcher::new(&process_type_word_map);
///
/// println!("{:?}", matcher);
/// ```
///
/// The above example creates a [SimpleMatcher] with a nested map and prints the matcher instance.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SimpleMatcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    ac_matcher: AhoCorasick,
    ac_dedup_word_conf_list: Vec<Vec<(ProcessType, u32, usize)>>,
    word_conf_map: IntMap<u32, WordConf>,
}

impl SimpleMatcher {
    /// Creates a new instance of [SimpleMatcher] from a given process type to word map.
    ///
    /// This method initializes the [SimpleMatcher] by constructing the internal structures necessary for efficient word matching.
    ///
    /// # Parameters
    ///
    /// - `process_type_word_map`: A reference to a hash map that associates [ProcessType] with another hash map.
    ///    The inner hash map links word identifiers (`u32`) to strings representing words. The outer hash map allows
    ///    different process types to have their own specific set of words.
    ///
    /// # Type Parameters
    ///
    /// - `I`: An iterator type whose items can be converted to string slices.
    /// - `S1`: A hasher type for the inner [HashMap].
    /// - `S2`: A hasher type for the outer [HashMap].
    ///
    /// # Returns
    ///
    /// Returns an initialized [SimpleMatcher] with all its internal structures set up for use.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::collections::HashMap;
    /// use matcher_rs::{SimpleMatcher, ProcessType};
    ///
    /// // Define a mock process_type_word_map for demonstration
    /// let mut process_type_word_map: HashMap<ProcessType, HashMap<u32, &str>> = HashMap::new();
    /// let mut inner_map: HashMap<u32, &str> = HashMap::new();
    /// inner_map.insert(1, "example&word");
    /// process_type_word_map.insert(ProcessType::None, inner_map);
    ///
    /// // Creating a SimpleMatcher instance
    /// let matcher = SimpleMatcher::new(&process_type_word_map);
    ///
    /// println!("{:?}", matcher);
    /// ```
    ///
    /// The above example demonstrates how to create a [SimpleMatcher] by passing a constructed
    /// `process_type_word_map`.
    pub fn new<I, S1, S2>(
        process_type_word_map: &HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str>,
    {
        let word_size: usize = process_type_word_map.values().map(|m| m.len()).sum();

        let mut process_type_set = IdSet::with_capacity(process_type_word_map.len());
        let mut ac_dedup_word_conf_list = Vec::with_capacity(word_size);
        let mut word_conf_map = IntMap::with_capacity_and_hasher(word_size, Default::default());

        let mut ac_dedup_word_id = 0;
        let mut ac_dedup_word_list = Vec::with_capacity(word_size);
        let mut ac_dedup_word_id_map =
            FxHashMap::with_capacity_and_hasher(word_size, Default::default());

        for (&process_type, simple_word_map) in process_type_word_map {
            let word_process_type = process_type - ProcessType::Delete;
            process_type_set.insert(process_type.bits() as usize);

            for (&simple_word_id, simple_word) in simple_word_map {
                let mut ac_split_word_and_counter = FxHashMap::default();
                let mut ac_split_word_not_counter = FxHashMap::default();

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

                let not_offset = ac_split_word_and_counter.len();
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
                        not_offset,
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

        let process_type_tree = build_process_type_tree(&process_type_set);

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
    /// Determines if the given text matches any pattern.
    ///
    /// This function first checks if the provided text is empty. If it is, the function
    /// immediately returns `false`. Otherwise, it processes the text using a process type
    /// tree to reduce the text, then checks for matches with the processed text and
    /// associated process types.
    ///
    /// # Arguments
    ///
    /// * `text` - A reference to a string slice that holds the text to be matched.
    ///
    /// # Returns
    ///
    /// * `true` if the text matches any pattern, otherwise `false`.
    ///
    /// # Safety
    ///
    /// This function does not perform any inherently unsafe operations, but it calls an
    /// internal function `_is_match_with_processed_text_process_type_set` which contains
    /// unsafe blocks. The safety of this function thus relies on the correctness and safety
    /// of the called internal functions.
    fn is_match(&'a self, text: &'a str) -> bool {
        if text.is_empty() {
            return false;
        }

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._is_match_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

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
    /// * `processed_text_process_type_set` - A reference to a slice containing tuples of
    ///   processed text and corresponding ID sets. The processed text is a [Cow] (Copy-On-Write)
    ///   string slice, and the ID set is an `id_set::IdSet`.
    ///
    /// # Returns
    ///
    /// * `true` if any pattern matches the processed text, otherwise `false`.
    ///
    /// # Safety
    ///
    /// This function contains several unsafe blocks. It relies on unchecked operations like
    /// `unwrap_unchecked`, `get_unchecked`, and unchecked arithmetic operations to ensure
    /// performance. The unsafe guarantees are based on the internal invariants that the
    /// original code assumes are always true, such as the fact that certain lookups and
    /// operations will not fail.
    fn _is_match_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> bool {
        let mut word_id_split_bit_map = FxHashMap::with_capacity_and_hasher(8, Default::default());
        let mut not_word_id_set = IdSet::new();

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
                        || not_word_id_set.contains(word_id as usize)
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
                                .map(|&bit| vec![bit; processed_times])
                                .collect::<Vec<Vec<i32>>>()
                        });

                    // bit is i32, so it will not overflow almost 100%
                    unsafe {
                        let bit = split_bit_matrix
                            .get_unchecked_mut(offset)
                            .get_unchecked_mut(index);
                        *bit = bit.unchecked_add((offset < word_conf.not_offset) as i32 * -2 + 1);

                        if offset >= word_conf.not_offset && *bit > 0 {
                            not_word_id_set.insert(word_id as usize);
                            word_id_split_bit_map.remove(&word_id);
                        }
                    }
                }
            }
        }

        word_id_split_bit_map.values().any(|split_bit_matrix| {
            split_bit_matrix
                .iter()
                .all(|split_bit_vec| split_bit_vec.iter().any(|&split_bit| split_bit <= 0))
        })
    }

    /// Processes the given text and returns a vector of matching results.
    ///
    /// This function takes an input `text`, processes it using the
    /// `reduce_text_process_with_tree` method to obtain a processed text process type set,
    /// and then passes this set to the `_process_with_processed_text_process_type_set` method
    /// to get the matching results. If the input `text` is empty, it immediately returns
    /// an empty vector.
    ///
    /// # Arguments
    ///
    /// * `text` - A reference to a string slice that needs to be processed.
    ///
    /// # Returns
    ///
    /// * A [`Vec<SimpleResult>`] containing the matching results. Each [SimpleResult] holds
    ///   the word ID and the matched word. If no matches are found or the text is empty,
    ///   it returns an empty vector.
    fn process(&'a self, text: &'a str) -> Vec<SimpleResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._process_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    /// Processes the given processed text and type sets to produce matching results.
    ///
    /// This function examines the provided processed text along with their corresponding ID sets
    /// and computes results by finding overlapping patterns using an Aho-Corasick matcher. The function
    /// maintains internal sets and maps to track which word IDs are relevant based on the processing types.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_set` - A reference to a slice of tuples, where each tuple
    ///   contains a [Cow] string slice (the processed text) and an [IdSet] (a set of IDs related to the processed text).
    ///
    /// # Returns
    ///
    /// * A vector of [`SimpleResult`] containing the word ID and the matched word for each successful match found. If no matches are found, it returns an empty vector.
    ///
    /// # Safety
    ///
    /// This function uses unsafe blocks extensively to ensure high performance. Unsafe operations like
    /// `unwrap_unchecked`, `get_unchecked`, and unchecked arithmetic operations are used based on the assumption
    /// that certain internal invariants always hold true: specific lookups and operations will not fail.
    /// The caller must ensure that the assumptions about the internal data structures remain valid.
    ///
    /// The unsafe guarantees are built on the same invariants as `_is_match_with_processed_text_process_type_set`, specifically that:
    /// - The internal patterns and configurations are set up correctly.
    /// - Index accesses and unwraps assume that the underlying data always exists and is valid.
    ///
    /// # Panics
    ///
    /// If the internal invariants are violated, the function may cause undefined behavior or panic.
    ///
    /// For example, if `processed_text_process_type_set` has invalid data or the internal Aho-Corasick matcher
    /// encounters unexpected states, this could lead to issues.
    fn _process_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> Vec<SimpleResult<'a>> {
        let mut word_id_split_bit_map = FxHashMap::with_capacity_and_hasher(8, Default::default());
        let mut not_word_id_set = IdSet::new();

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
                        || not_word_id_set.contains(word_id as usize)
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
                                .map(|&bit| vec![bit; processed_times])
                                .collect::<Vec<Vec<i32>>>()
                        });

                    // split_bit is i32, so it will not overflow almost 100%
                    unsafe {
                        let split_bit = split_bit_matrix
                            .get_unchecked_mut(offset)
                            .get_unchecked_mut(index);
                        *split_bit = split_bit
                            .unchecked_add((offset < word_conf.not_offset) as i32 * -2 + 1);

                        if offset >= word_conf.not_offset && *split_bit > 0 {
                            not_word_id_set.insert(word_id as usize);
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
