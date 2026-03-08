use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

#[cfg(not(feature = "vectorscan"))]
use aho_corasick::AhoCorasickKind;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use serde::Serialize;
use tinyvec::TinyVec;

use crate::process::process_matcher::{
    ProcessType, ProcessTypeBitNode, ProcessedTextMasks, build_process_type_tree,
    reduce_text_process_emit, reduce_text_process_with_tree, return_processed_string_to_pool,
};
#[cfg(feature = "vectorscan")]
use crate::vectorscan::VectorscanScanner;

/// Internal state used for tracking matches in `SimpleMatcher`.
///
/// This structure implements a sparse-set like optimization to avoid full clearing
/// of state between matching calls. It uses generation IDs to determine if an
/// entry is valid for the current query.
///
/// # Fields
/// * `matrix` - A flat vector of state matrices for each word configuration. Each entry is indexed by `word_conf_idx`.
/// * `satisfied_mask` - A vector of bitmasks indicating which splits have been satisfied for each word configuration.
/// * `matrix_generation` - Stores the generation ID for each entry in `matrix`.
/// * `not_flags_generation` - Stores the generation ID indicating if a word is blocked by a "NOT" (`~`) pattern.
/// * `touched_indices` - A list of indices in `word_conf_list` that were modified during the current search.
/// * `generation` - The current search's unique generation ID.
struct SimpleMatchState {
    matrix: Vec<TinyVec<[i32; 16]>>,
    satisfied_mask: Vec<u64>,
    matrix_generation: Vec<u32>,
    not_flags_generation: Vec<u32>,
    touched_indices: Vec<usize>,
    generation: u32,
}

impl SimpleMatchState {
    /// Creates a new, empty `SimpleMatchState`.
    ///
    /// # Returns
    /// A new instance of `SimpleMatchState` with default values.
    fn new() -> Self {
        Self {
            matrix: Vec::new(),
            satisfied_mask: Vec::new(),
            matrix_generation: Vec::new(),
            not_flags_generation: Vec::new(),
            touched_indices: Vec::new(),
            generation: 0,
        }
    }

    /// Prepares the state for a new search operation.
    ///
    /// This increments the generation ID and ensures the internal buffers are
    /// large enough for the given number of word configurations.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Increments `self.generation`. If it overflows `u32::MAX`, resets all generation tracking arrays to 0.
    /// 2. Resizes `matrix`, `satisfied_mask`, `matrix_generation`, and `not_flags_generation` to `size` if they are smaller.
    /// 3. Clears `touched_indices`.
    ///
    /// # Arguments
    /// * `size` - The number of word configurations to accommodate.
    fn prepare(&mut self, size: usize) {
        if self.generation == u32::MAX {
            self.matrix_generation.fill(0);
            self.not_flags_generation.fill(0);
            self.generation = 1;
        } else {
            self.generation += 1;
        }

        if self.matrix.len() < size {
            self.matrix.resize(size, TinyVec::new());
            self.satisfied_mask.resize(size, 0);
            self.matrix_generation.resize(size, 0);
            self.not_flags_generation.resize(size, 0);
        }

        self.touched_indices.clear();
    }
}

thread_local! {
    /// Thread-local cache for `SimpleMatchState` to avoid repeated allocations.
    static SIMPLE_MATCH_STATE: RefCell<SimpleMatchState> = RefCell::new(SimpleMatchState::new());
}

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
/// * `expected_mask` - A bitmask representing the required AND splits that must be satisfied.
/// * `use_matrix` - A boolean indicating whether the state matrix is required for this word.
#[derive(Debug, Clone)]
struct WordConf {
    word_id: u32,
    word: String,
    split_bit: Vec<i32>,
    not_offset: usize,
    expected_mask: u64,
    use_matrix: bool,
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

/// Wrapper for the underlying string matching engine.
#[derive(Debug, Clone)]
enum AcMatcher {
    /// Standard Aho-Corasick implementation.
    #[cfg_attr(feature = "vectorscan", allow(dead_code))]
    AhoCorasick(AhoCorasick),
    /// Intel Vectorscan (Hyperscan) implementation for SIMD-accelerated matching.
    #[cfg(feature = "vectorscan")]
    Vectorscan(VectorscanScanner),
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
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 1, "apple&pie")
///     .add_word(ProcessType::None, 2, "banana~peel")
///     .build();
///
/// assert!(matcher.is_match("I like apple and pie"));
/// assert!(!matcher.is_match("I like banana peel"));
/// ```
#[derive(Debug, Clone)]
pub struct SimpleMatcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    ac_matcher: AcMatcher,
    ac_dedup_word_conf_list: Vec<Vec<WordConfEntry>>,
    word_conf_list: Vec<WordConf>,
}

impl SimpleMatcher {
    /// Creates a new [`SimpleMatcher`] from a mapping of process types to words.
    ///
    /// It is recommended to use [`crate::SimpleMatcherBuilder`] instead.
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
    pub fn new<'a, I, S1, S2>(
        process_type_word_map: &'a HashMap<ProcessType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str> + 'a,
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
                let mut current_is_not = false;

                let mut add_sub_word = |word: &'a str, is_not: bool| {
                    if word.is_empty() {
                        return;
                    }
                    if is_not {
                        let entry = ac_split_word_not_counter.entry(word).or_insert(1);
                        *entry -= 1;
                    } else {
                        let entry = ac_split_word_and_counter.entry(word).or_insert(0);
                        *entry += 1;
                    }
                };

                for (index, char) in simple_word.as_ref().match_indices(['&', '~']) {
                    add_sub_word(&simple_word.as_ref()[start..index], current_is_not);
                    current_is_not = char == "~";
                    start = index + 1;
                }
                add_sub_word(&simple_word.as_ref()[start..], current_is_not);

                if ac_split_word_and_counter.is_empty() && ac_split_word_not_counter.is_empty() {
                    continue;
                }

                let not_offset = ac_split_word_and_counter.len();
                let split_bit = ac_split_word_and_counter
                    .values()
                    .copied()
                    .chain(ac_split_word_not_counter.values().copied())
                    .collect::<Vec<i32>>();

                let expected_mask = if not_offset > 0 && not_offset <= 64 {
                    u64::MAX >> (64 - not_offset)
                } else {
                    0
                };

                let use_matrix = not_offset > 64
                    || split_bit.len() > 64
                    || split_bit[..not_offset].iter().any(|&v| v != 1)
                    || split_bit[not_offset..].iter().any(|&v| v != 0);

                let word_conf_idx = if let Some(&existing_idx) = word_id_to_idx.get(&simple_word_id)
                {
                    word_conf_list[existing_idx] = WordConf {
                        word_id: simple_word_id,
                        word: simple_word.as_ref().to_owned(),
                        split_bit,
                        not_offset,
                        expected_mask,
                        use_matrix,
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
                        expected_mask,
                        use_matrix,
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

        let process_type_tree = build_process_type_tree(&process_type_set);

        let patterns = ac_dedup_word_list
            .iter()
            .map(|ac_word| ac_word.as_ref())
            .collect::<Vec<_>>();

        #[cfg(feature = "vectorscan")]
        let ac_matcher = if patterns.is_empty() {
            AcMatcher::AhoCorasick(AhoCorasickBuilder::new().build(&patterns).unwrap())
        } else {
            let flags = vec![0u32; patterns.len()];
            AcMatcher::Vectorscan(
                VectorscanScanner::new_literal(&patterns, &flags)
                    .expect("failed to compile vectorscan literal database"),
            )
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
                    .build(&patterns)
                    .unwrap(),
            )
        };

        SimpleMatcher {
            process_type_tree,
            ac_matcher,
            ac_dedup_word_conf_list,
            word_conf_list,
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
    /// * `state` - The internal `SimpleMatchState` used to track hits and disqualifications.
    fn _word_match_with_processed_text_process_type_masks<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        state: &mut SimpleMatchState,
    ) {
        if self.ac_dedup_word_conf_list.is_empty() {
            return;
        }

        let processed_times = processed_text_process_type_masks.len();

        for (index, (processed_text, process_type_mask)) in
            processed_text_process_type_masks.iter().enumerate()
        {
            match &self.ac_matcher {
                AcMatcher::AhoCorasick(ac_matcher) => {
                    for ac_dedup_result in ac_matcher.find_overlapping_iter(processed_text.as_ref())
                    {
                        let pattern_idx = ac_dedup_result.pattern().as_usize();
                        self.process_match(
                            pattern_idx,
                            index,
                            *process_type_mask,
                            processed_times,
                            state,
                        );
                    }
                }
                #[cfg(feature = "vectorscan")]
                AcMatcher::Vectorscan(scanner) => {
                    let _ = scanner.scan(processed_text.as_ref().as_bytes(), |pattern_idx| {
                        self.process_match(
                            pattern_idx,
                            index,
                            *process_type_mask,
                            processed_times,
                            state,
                        );
                    });
                }
            }
        }
    }

    /// Records a sub-pattern match and updates the logical state of affected rules.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Retrieves all rules (`WordConfEntry`) associated with the matched `pattern_idx`.
    /// 2. Skips rules that don't match the current `process_type_mask` or are already disqualified.
    /// 3. If a rule is touched for the first time in this search (checked via `matrix_generation`):
    ///    - Marks it as touched and adds it to `touched_indices`.
    ///    - Initializes its row in the state matrix with the default `split_bit` values.
    /// 4. Updates the specific bit in the rule's state matrix corresponding to the `text_index`.
    /// 5. If the match belongs to a "NOT" (`~`) logical segment and the bit becomes satisfied (`> 0`),
    ///    the rule is marked as disqualified in `not_flags_generation`.
    ///
    /// # Arguments
    /// * `pattern_idx` - The index of the matched sub-pattern in `ac_dedup_word_conf_list`.
    /// * `text_index` - The index of the current text variant being scanned.
    /// * `process_type_mask` - Bitmask of active process types for the current variant.
    /// * `processed_times` - Total number of text variants in the search.
    /// * `state` - The internal `SimpleMatchState` to update.
    #[inline]
    fn process_match(
        &self,
        pattern_idx: usize,
        text_index: usize,
        process_type_mask: u64,
        processed_times: usize,
        state: &mut SimpleMatchState,
    ) {
        let generation = state.generation;
        for &WordConfEntry {
            process_type: match_process_type,
            word_conf_idx,
            offset,
        } in &self.ac_dedup_word_conf_list[pattern_idx]
        {
            if process_type_mask & (1u64 << match_process_type.bits()) == 0
                || state.not_flags_generation[word_conf_idx] == generation
            {
                continue;
            }

            let word_conf = &self.word_conf_list[word_conf_idx];

            if state.matrix_generation[word_conf_idx] != generation {
                state.matrix_generation[word_conf_idx] = generation;
                state.touched_indices.push(word_conf_idx);
                state.satisfied_mask[word_conf_idx] = 0;

                if word_conf.use_matrix {
                    let num_splits = word_conf.split_bit.len();
                    let flat_matrix = &mut state.matrix[word_conf_idx];
                    flat_matrix.clear();
                    flat_matrix.resize(num_splits * processed_times, 0i32);
                    for (s, &bit) in word_conf.split_bit.iter().enumerate() {
                        let row_start = s * processed_times;
                        flat_matrix[row_start..row_start + processed_times].fill(bit);
                    }
                }
            }

            if word_conf.use_matrix {
                let flat_matrix = &mut state.matrix[word_conf_idx];
                let bit = &mut flat_matrix[offset * processed_times + text_index];
                *bit += (offset < word_conf.not_offset) as i32 * -2 + 1;

                if offset < word_conf.not_offset {
                    if *bit <= 0 {
                        state.satisfied_mask[word_conf_idx] |= 1u64 << offset;
                    }
                } else if *bit > 0 {
                    state.not_flags_generation[word_conf_idx] = generation;
                }
            } else if offset < word_conf.not_offset {
                state.satisfied_mask[word_conf_idx] |= 1u64 << offset;
            } else {
                state.not_flags_generation[word_conf_idx] = generation;
            }
        }
    }

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
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
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
    pub fn is_match<'a>(&'a self, text: &'a str) -> bool {
        if text.is_empty() {
            return false;
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        let result = self.is_match_preprocessed(&processed_text_process_type_masks);

        return_processed_string_to_pool(processed_text_process_type_masks);

        result
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
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "apple")
    ///     .add_word(ProcessType::None, 2, "banana")
    ///     .build();
    ///
    /// let results = matcher.process("I have an apple and a banana");
    /// assert_eq!(results.len(), 2);
    /// ```
    pub fn process<'a>(&'a self, text: &'a str) -> Vec<SimpleResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        let result = self.process_preprocessed(&processed_text_process_type_masks);

        return_processed_string_to_pool(processed_text_process_type_masks);

        result
    }

    /// Pass 2: Evaluates the state matrix to determine if any rule is fully satisfied.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Executes Pass 1 to populate `SimpleMatchState` with candidate matrix states.
    /// 2. Iterates over `touched_indices` in the state.
    /// 3. Skips rules that were disqualified by "NOT" patterns.
    /// 4. For each rule candidate, checks if every logical segment (row in the matrix)
    ///    has been satisfied (`bit <= 0`) in at least one text variant (column in the matrix).
    /// 5. Returns `true` on the first rule that meets these criteria.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - Pre-processed text variants and bitmasks.
    ///
    /// # Returns
    /// `true` if any rule matches.
    fn is_match_preprocessed<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> bool {
        SIMPLE_MATCH_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.prepare(self.word_conf_list.len());

            self._word_match_with_processed_text_process_type_masks(
                processed_text_process_type_masks,
                &mut state,
            );

            let generation = state.generation;
            let processed_times = processed_text_process_type_masks.len();

            state.touched_indices.iter().any(|&word_conf_idx| {
                if state.not_flags_generation[word_conf_idx] == generation {
                    return false;
                }
                let word_conf = &self.word_conf_list[word_conf_idx];
                let expected_mask = word_conf.expected_mask;
                if expected_mask > 0 {
                    return state.satisfied_mask[word_conf_idx] == expected_mask;
                }

                let num_splits = word_conf.split_bit.len();
                let flat_matrix = &state.matrix[word_conf_idx];
                (0..num_splits).all(|s| {
                    flat_matrix[s * processed_times..(s + 1) * processed_times]
                        .iter()
                        .any(|&bit| bit <= 0)
                })
            })
        })
    }

    /// Pass 2: Evaluates the state matrix and returns all satisfied rules.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Executes Pass 1 to populate `SimpleMatchState` with candidate matrix states.
    /// 2. Filters rules where all logical segments were satisfied and no "NOT" segments were triggered.
    /// 3. Projects satisfied rules into [`SimpleResult`] objects.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - Pre-processed text variants and bitmasks.
    ///
    /// # Returns
    /// A vector of [`SimpleResult`] matches.
    fn process_preprocessed<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<SimpleResult<'a>> {
        SIMPLE_MATCH_STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.prepare(self.word_conf_list.len());

            self._word_match_with_processed_text_process_type_masks(
                processed_text_process_type_masks,
                &mut state,
            );

            let generation = state.generation;
            let processed_times = processed_text_process_type_masks.len();

            state
                .touched_indices
                .iter()
                .filter_map(|&word_conf_idx| {
                    if state.not_flags_generation[word_conf_idx] == generation {
                        return None;
                    }
                    let word_conf = &self.word_conf_list[word_conf_idx];
                    let expected_mask = word_conf.expected_mask;
                    let is_satisfied = if expected_mask > 0 {
                        state.satisfied_mask[word_conf_idx] == expected_mask
                    } else {
                        let num_splits = word_conf.split_bit.len();
                        let flat_matrix = &state.matrix[word_conf_idx];
                        (0..num_splits).all(|s| {
                            flat_matrix[s * processed_times..(s + 1) * processed_times]
                                .iter()
                                .any(|&bit| bit <= 0)
                        })
                    };

                    is_satisfied.then_some(SimpleResult {
                        word_id: word_conf.word_id,
                        word: Cow::Borrowed(&word_conf.word),
                    })
                })
                .collect()
        })
    }
}
