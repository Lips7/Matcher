use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
#[cfg(not(feature = "vectorscan"))]
use aho_corasick::AhoCorasickKind;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::Serialize;
use tinyvec::TinyVec;

use crate::matcher::{MatchResultTrait, TextMatcherTrait};
use crate::process::process_matcher::{
    ProcessType, ProcessTypeBitNode, ProcessedTextMasks, build_process_type_tree,
    reduce_text_process_emit, reduce_text_process_with_tree,
};
#[cfg(feature = "vectorscan")]
use crate::vectorscan_matcher::VectorscanMatcher;

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

/// A type alias for a nested map structure used for serialization and deserialization.
///
/// This serves exactly the same role as [`SimpleTable`] but internally owns its
/// text references using a copy-on-write `Cow<'a, str>` string format.
///
/// # Type Parameters
/// * `'a` - The lifetime of the string slices.
pub type SimpleTableSerde<'a> = HashMap<ProcessType, HashMap<u32, Cow<'a, str>>>;

/// Represents the configuration for a word within the SimpleMatcher.
///
/// [`WordConf`] contains the word as a string, the split bits indicating logical operators ('&' for AND, '~' for NOT),
/// and the index separating the 'NOT' part from the rest in the split bits vector.
///
/// # Fields
/// * `word_id` - A unique identifier for the word within the table.
/// * `word` - The original word as a String.
/// * `split_bit` - A vector of integers representing the logical splits of the word. Positive integers indicate multiple occurrences of sub-strings tied to '&' operators, while negative integers correspond to '~' operators.
/// * `not_offset` - The index in `split_bit` that indicates the start of the 'NOT' split parts.
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
/// * `word_id` - A unique identifier for the word within the table.
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

/// Represents a single entry in the deduplicated word configuration list.
///
/// [`WordConfEntry`] provides a mapping between a matched pattern and its original
/// word configuration, specifying the process type and the specific sub-pattern offset.
///
/// # Fields
/// * `process_type` - The [`ProcessType`] associated with this word configuration.
/// * `word_conf_idx` - The index of the [`WordConf`] within the `word_conf_list`.
/// * `offset` - The position within the `split_bit` vector of the [`WordConf`].
#[derive(Debug, Clone)]
struct WordConfEntry {
    process_type: ProcessType,
    word_conf_idx: usize,
    offset: usize,
}

#[derive(Debug, Clone)]
enum AcMatcher {
    #[cfg_attr(feature = "vectorscan", allow(dead_code))]
    AhoCorasick(AhoCorasick),
    #[cfg(feature = "vectorscan")]
    Vectorscan(Arc<VectorscanMatcher>),
}

/// Represents a simple matcher for processing words using Aho-Corasick or Vectorscan.
///
/// The [`SimpleMatcher`] is optimized for exact matching of multiple patterns simultaneously.
/// It supports complex logical operators within a single pattern entry:
/// - **AND (`&`)**: All sub-patterns separated by `&` must match for the rule to trigger.
/// - **NOT (`~`)**: If any sub-pattern preceded by `~` matches, the rule is disqualified.
///
/// # Detailed Explanation / Algorithm
/// 1. **Initialization**:
///    - Parses logical operators in each pattern, splitting them into AND and NOT sub-patterns.
///    - Assigns a `split_bit` vector tracking the state of each logical segment.
///    - Deduplicates all unique sub-patterns across all `ProcessType` variants.
///    - Compiles an optimized Aho-Corasick automaton (DFA or NFA) or Vectorscan database from the unique sub-patterns.
/// 2. **Matching (Two-Pass Logic)**:
///    - **Pass 1**: Scans the text using the AC/Vectorscan automaton. For each hit, it updates a state matrix
///      representing which logical segments for which rules have been satisfied in which text variant.
///    - **Pass 2**: Evaluates the state matrix. A rule matches if all its AND segments were satisfied
///      in at least one text variant AND none of its NOT segments were found.
///
/// # Fields
/// * `process_type_tree` - Workflow tree for efficient text transforms.
/// * `ac_matcher` - Compiled Aho-Corasick or Vectorscan automaton.
/// * `ac_dedup_word_conf_list` - References from automaton hits back to original rules.
/// * `word_conf_list` - Unified metadata for each parsed split pattern block.
///
/// # Examples
/// ```rust
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType, TextMatcherTrait};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 1, "apple&pie")
///     .add_word(ProcessType::None, 2, "banana~peel")
///     .build();
///
/// assert!(matcher.is_match("I like apple and pie"));
/// assert!(!matcher.is_match("I like banana peel"));
/// ```
#[derive(Debug)]
pub struct SimpleMatcher {
    process_type_tree: Box<[ProcessTypeBitNode]>,
    ac_matcher: AssertUnwindSafe<AcMatcher>,
    ac_dedup_word_conf_list: Box<[Box<[WordConfEntry]>]>,
    word_conf_list: Box<[WordConf]>,
}

impl Clone for SimpleMatcher {
    fn clone(&self) -> Self {
        Self {
            process_type_tree: self.process_type_tree.clone(),
            ac_matcher: AssertUnwindSafe((*self.ac_matcher).clone()),
            ac_dedup_word_conf_list: self.ac_dedup_word_conf_list.clone(),
            word_conf_list: self.word_conf_list.clone(),
        }
    }
}

impl SimpleMatcher {
    /// Creates a new [`SimpleMatcher`] from a mapping of process types to words.
    ///
    /// It is recommended to use [`SimpleMatcherBuilder`] instead.
    ///
    /// # Detailed Explanation / Algorithm
    /// This method is computationally intensive. It iterates through all patterns,
    /// performs manual parsing of `&` and `~` (ignoring escaped versions if implemented),
    /// generates all required normalized variants for each sub-pattern, and finally
    /// builds the Aho-Corasick automaton.
    ///
    /// # Type Parameters
    /// * `I` - Iterator yielding string slices.
    /// * `S1`, `S2` - Hashers for the input maps.
    ///
    /// # Arguments
    /// * `process_type_word_map` - Maps [`ProcessType`] to identifiers and their patterns.
    ///
    /// # Returns
    /// An initialized and compiled [`SimpleMatcher`].
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
                if simple_word.as_ref().is_empty() {
                    continue;
                }
                let mut ac_split_word_and_counter = HashMap::new();
                let mut ac_split_word_not_counter = HashMap::new();

                let mut start = 0;
                let mut is_and = false;
                let mut is_not = false;

                for (index, char) in simple_word.as_ref().match_indices(['&', '~']) {
                    if (is_and || start == 0) && start != index {
                        let word = &simple_word.as_ref()[start..index];
                        if !word.is_empty() {
                            ac_split_word_and_counter
                                .entry(word)
                                .and_modify(|cnt| *cnt += 1)
                                .or_insert(1);
                        }
                    }
                    if is_not && start != index {
                        let word = &simple_word.as_ref()[start..index];
                        if !word.is_empty() {
                            ac_split_word_not_counter
                                .entry(word)
                                .and_modify(|cnt| *cnt -= 1)
                                .or_insert(0);
                        }
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
                    let word = &simple_word.as_ref()[start..];
                    if !word.is_empty() {
                        ac_split_word_and_counter
                            .entry(word)
                            .and_modify(|cnt| *cnt += 1)
                            .or_insert(1);
                    }
                }
                if is_not && start != simple_word.as_ref().len() {
                    let word = &simple_word.as_ref()[start..];
                    if !word.is_empty() {
                        ac_split_word_not_counter
                            .entry(word)
                            .and_modify(|cnt| *cnt -= 1)
                            .or_insert(0);
                    }
                }

                if ac_split_word_and_counter.is_empty() && ac_split_word_not_counter.is_empty() {
                    continue;
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
                            ac_dedup_word_conf_list.push(vec![WordConfEntry {
                                process_type,
                                word_conf_idx,
                                offset,
                            }]);
                            ac_dedup_word_list.push(ac_word);
                            ac_dedup_word_id += 1;
                            continue;
                        };
                        ac_dedup_word_conf_list[ac_dedup_word_id as usize].push(WordConfEntry {
                            process_type,
                            word_conf_idx,
                            offset,
                        });
                    }
                }
            }
        }

        let process_type_tree = build_process_type_tree(&process_type_set).into_boxed_slice();

        #[cfg(feature = "vectorscan")]
        let ac_matcher = if ac_dedup_word_list.is_empty() {
            AcMatcher::AhoCorasick(AhoCorasickBuilder::new().build([""]).unwrap())
        } else {
            AcMatcher::Vectorscan(Arc::new(
                VectorscanMatcher::new(
                    &ac_dedup_word_list
                        .iter()
                        .map(|ac_word| ac_word.as_ref())
                        .collect::<Vec<_>>(),
                ),
            ))
        };

        #[cfg(not(feature = "vectorscan"))]
        let ac_matcher = {
            #[cfg(feature = "dfa")]
            let aho_corasick_kind = AhoCorasickKind::DFA;
            #[cfg(not(feature = "dfa"))]
            let aho_corasick_kind = AhoCorasickKind::ContiguousNFA;

            AcMatcher::AhoCorasick(
                AhoCorasickBuilder::new()
                    .kind(Some(aho_corasick_kind))
                    .build(ac_dedup_word_list.iter().map(|ac_word| ac_word.as_ref()))
                    .unwrap(),
            )
        };

        let ac_dedup_word_conf_list = ac_dedup_word_conf_list
            .into_iter()
            .map(|v| v.into_boxed_slice())
            .collect::<Box<[_]>>();

        SimpleMatcher {
            process_type_tree,
            ac_matcher: AssertUnwindSafe(ac_matcher),
            ac_dedup_word_conf_list,
            word_conf_list: word_conf_list.into_boxed_slice(),
        }
    }

    /// Pass 1: Scans text variants and records sub-pattern hits in a state matrix.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Iterates over each text variant and its bitmask.
    /// 2. Performs overlapping search using Aho-Corasick or Vectorscan.
    /// 3. For each hit:
    ///    - Checks if the hit's `ProcessType` is allowed for the current variant.
    ///    - Increments or decrements the state in a `flat_matrix` (`split_bit_store`).
    ///    - **NOT Check**: If a `~` sub-pattern is hit, the rule is immediately disqualified
    ///      (`not_word_id_set`).
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - Pre-processed text variants and bitmasks.
    ///
    /// # Returns
    /// A list of rule identifiers and their corresponding state matrices (`flat_split_bit_matrix`).
    fn _word_match_with_processed_text_process_type_masks<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<(usize, TinyVec<[i32; 16]>)> {
        if self.ac_dedup_word_conf_list.is_empty() {
            return Vec::new();
        }
        let mut split_bit_store: FxHashMap<usize, TinyVec<[i32; 16]>> =
            FxHashMap::with_capacity_and_hasher(16, Default::default());
        let mut not_word_id_set: FxHashSet<usize> = FxHashSet::default();

        let processed_times = processed_text_process_type_masks.len();

        for (index, (processed_text, process_type_mask)) in
            processed_text_process_type_masks.iter().enumerate()
        {
            match &*self.ac_matcher {
                AcMatcher::AhoCorasick(ac_matcher) => {
                    for ac_dedup_result in ac_matcher.find_overlapping_iter(processed_text.as_ref())
                    {
                        let pattern_idx = ac_dedup_result.pattern().as_usize();
                        self.process_match(
                            pattern_idx,
                            index,
                            *process_type_mask,
                            processed_times,
                            &mut split_bit_store,
                            &mut not_word_id_set,
                        );
                    }
                }
                #[cfg(feature = "vectorscan")]
                AcMatcher::Vectorscan(vs_matcher) => {
                    for pattern_idx in vs_matcher.find_overlapping_iter(processed_text.as_ref()) {
                        self.process_match(
                            pattern_idx,
                            index,
                            *process_type_mask,
                            processed_times,
                            &mut split_bit_store,
                            &mut not_word_id_set,
                        );
                    }
                }
            }
        }

        split_bit_store.into_iter().collect()
    }

    #[inline]
    fn process_match(
        &self,
        pattern_idx: usize,
        text_index: usize,
        process_type_mask: u64,
        processed_times: usize,
        split_bit_store: &mut FxHashMap<usize, TinyVec<[i32; 16]>>,
        not_word_id_set: &mut FxHashSet<usize>,
    ) {
        for &WordConfEntry {
            process_type: match_process_type,
            word_conf_idx,
            offset,
        } in &self.ac_dedup_word_conf_list[pattern_idx]
        {
            if process_type_mask & (1u64 << match_process_type.bits()) == 0
                || not_word_id_set.contains(&word_conf_idx)
            {
                continue;
            }

            let word_conf = &self.word_conf_list[word_conf_idx];

            let flat_matrix = split_bit_store.entry(word_conf_idx).or_insert_with(|| {
                let num_splits = word_conf.split_bit.len();
                let mut flat = TinyVec::new();
                flat.resize(num_splits * processed_times, 0i32);
                for (s, &bit) in word_conf.split_bit.iter().enumerate() {
                    let row_start = s * processed_times;
                    flat[row_start..row_start + processed_times].fill(bit);
                }
                flat
            });

            let bit = &mut flat_matrix[offset * processed_times + text_index];
            *bit += (offset < word_conf.not_offset) as i32 * -2 + 1;

            if offset >= word_conf.not_offset && *bit > 0 {
                not_word_id_set.insert(word_conf_idx);
                split_bit_store.remove(&word_conf_idx);
            }
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
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    /// `true` if the text matches any pattern, otherwise `false`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType, TextMatcherTrait};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "hello")
    ///     .add_word(ProcessType::None, 2, "world")
    ///     .build();
    ///
    /// assert!(matcher.is_match("hello there"));
    /// assert!(matcher.is_match("beautiful world"));
    /// assert!(!matcher.is_match("hi planet!"));
    /// ```
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
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    /// A [`Vec<SimpleResult>`] containing the matching results.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType, TextMatcherTrait};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "apple")
    ///     .add_word(ProcessType::None, 2, "banana")
    ///     .build();
    ///
    /// let results = matcher.process("I have an apple and a banana");
    /// assert_eq!(results.len(), 2);
    /// ```
    fn process(&'a self, text: &'a str) -> Vec<SimpleResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.process_preprocessed(&processed_text_process_type_masks)
    }

    /// Pass 2: Evaluates the state matrix to determine if any rule is fully satisfied.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Executes Pass 1 to get candidate matrix states.
    /// 2. For each rule candidate, checks if every logical segment (row in the matrix)
    ///    has been satisfied (`bit <= 0`) in at least one text variant (column in the matrix).
    /// 3. Returns `true` on the first rule that meets these criteria.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - Pre-processed text variants and bitmasks.
    ///
    /// # Returns
    /// `true` if any rule matches.
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

    /// Pass 2: Evaluates the state matrix and returns all satisfied rules.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Executes Pass 1 to get candidate matrix states.
    /// 2. Filters rules where all logical segments were satisfied.
    /// 3. Projects satisfied rules into [`SimpleResult`] objects.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - Pre-processed text variants and bitmasks.
    ///
    /// # Returns
    /// A vector of [`SimpleResult`] matches.
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
