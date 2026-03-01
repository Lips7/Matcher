use std::{borrow::Cow, collections::HashSet};

use rapidfuzz::distance;
use serde::{Deserialize, Serialize};

use crate::{
    matcher::{MatchResultTrait, TextMatcherInternal, TextMatcherTrait},
    process::process_matcher::{
        ProcessType, ProcessTypeBitNode, ProcessedTextMasks, build_process_type_tree,
        reduce_text_process_with_tree,
    },
};

/// Enumeration representing the types of similarity matching algorithms available.
///
/// Currently, this enum only supports the Levenshtein distance algorithm.
///
/// # Variants
/// * `Levenshtein` - Represents the Levenshtein distance algorithm, a string metric for measuring the difference between two sequences.
///
/// The enum variants are serialized and deserialized using the `snake_case` naming convention.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SimMatchType {
    Levenshtein,
}

/// Represents a table structure to be used in the similarity matching process.
///
/// This structure holds various properties required for similarity matching using different algorithms.
///
/// # Type Parameters
/// * `'a` - The lifetime of the borrowed strings in the word list.
///
/// # Fields
/// * `table_id` - A unique identifier for the specific matching table.
/// * `match_id` - A unique identifier for the match operation.
/// * `process_type` - The text processing rules to be applied, represented by the [`ProcessType`] bitflags enum.
/// * `sim_match_type` - The type of similarity matching algorithm to be used, represented by the [`SimMatchType`] enum.
/// * `word_list` - A list of words to be used in the matching process.
/// * `threshold` - A float value representing the similarity threshold for matching.
#[derive(Debug, Clone)]
pub struct SimTable<'a> {
    pub table_id: u32,
    pub match_id: u32,
    pub process_type: ProcessType,
    pub sim_match_type: SimMatchType,
    pub word_list: Vec<&'a str>,
    pub threshold: f64,
}

/// Represents a processed table used in the similarity matching process.
///
/// This struct is a concrete version of the [`SimTable`] struct, with ownership over
/// the word list.
///
/// # Fields
/// * `table_id` - A unique identifier for the specific matching table.
/// * `match_id` - A unique identifier for the match operation.
/// * `process_type` - The text processing rules to be applied, represented by the [`ProcessType`] bitflags enum.
/// * `sim_match_type` - The type of similarity matching algorithm to be used, represented by the [`SimMatchType`] enum.
/// * `word_list` - A list of words to be used in the matching process. This is an owned vector of strings.
/// * `threshold` - A float value representing the similarity threshold for a match.
#[derive(Debug, Clone)]
struct SimProcessedTable {
    table_id: u32,
    match_id: u32,
    process_type: ProcessType,
    sim_match_type: SimMatchType,
    word_list: Vec<String>,
    threshold: f64,
}

/// Represents the result of a similarity matching operation.
///
/// This struct holds information about the match including identifiers for the match and table,
/// the word that was matched, and the similarity score of the match. The word is represented as a
/// `Cow` (Clone on Write) for efficient handling of borrowed or owned strings. This allows
/// flexibility in returning either a borrowed string or an owned string.
///
/// # Type Parameters
/// * `'a` - The lifetime of the matched word content.
///
/// # Fields
/// * `match_id` - A unique identifier for the match operation.
/// * `table_id` - A unique identifier for the specific matching table.
/// * `word_id` - A unique identifier for the word within the table.
/// * `word` - The word that was matched, represented as a `Cow`.
/// * `similarity` - A float value representing the similarity score of the match.
#[derive(Debug, Clone)]
pub struct SimResult<'a> {
    pub match_id: u32,
    pub table_id: u32,
    pub word_id: u32,
    pub word: Cow<'a, str>,
    pub similarity: f64,
}

impl MatchResultTrait<'_> for SimResult<'_> {
    fn match_id(&self) -> u32 {
        self.match_id
    }
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn word_id(&self) -> u32 {
        0
    }
    fn word(&self) -> &str {
        &self.word
    }
    fn similarity(&self) -> Option<f64> {
        Some(self.similarity)
    }
}

/// The [`SimMatcher`] struct is responsible for performing similarity matching operations
/// based on different processing types and similarity algorithms.
///
/// This struct maintains a process type tree and a list of pre-processed tables that contain
/// the necessary information for performing similarity matching on texts. Under the hood, it
/// delegates comparisons to `rapidfuzz` functions.
///
/// # Algorithm
/// 1. Iterates through each `SimTable` and extracts its `Word` definitions and `ProcessType`.
/// 2. Converts borrowed strings recursively to owned variations enforcing memory decoupling (`SimProcessedTable`).
/// 3. Compiles a composite `ProcessTypeBitNode` to resolve string iterations correctly.
/// 4. Defers sequence operations directly to PyO3 wrappers interfacing with C-accelerated Levenshtein metric evaluators (`rapidfuzz`).
///
/// # Fields
/// * `process_type_tree` - The compiled workflow tree ensuring text transforms happen exactly once per distinct branch sequence.
/// * `sim_processed_table_list` - A list storing configured similarity rules alongside their text ownership blocks.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{SimMatcher, SimTable, SimMatchType, ProcessType, TextMatcherTrait};
///
/// let sim_table_list = vec![SimTable {
///     table_id: 1,
///     match_id: 1,
///     process_type: ProcessType::None,
///     sim_match_type: SimMatchType::Levenshtein,
///     word_list: vec!["example", "test"],
///     threshold: 0.6, // Low threshold to allow matching "exampel"
/// }];
///
/// let matcher = SimMatcher::new(&sim_table_list);
///
/// assert!(matcher.is_match("exampel"));
/// ```
#[derive(Debug, Clone)]
pub struct SimMatcher {
    process_type_tree: Box<[ProcessTypeBitNode]>,
    sim_processed_table_list: Box<[SimProcessedTable]>,
}

impl SimMatcher {
    /// Creates a new instance of [`SimMatcher`] from a list of [`SimTable`].
    ///
    /// This function initializes a [`SimMatcher`] by processing each [`SimTable`] in the input list.
    /// It extracts the process types and constructs a tree structure used for processing texts.
    /// Additionally, it converts the word lists in each [`SimTable`] from borrowed strings to owned strings
    /// stored inside `SimProcessedTable` instances to ensure they can be freely moved and referenced.
    ///
    /// # Arguments
    /// * `sim_table_list` - A slice of [`SimTable`] references to be processed.
    ///
    /// # Returns
    /// An initialized [`SimMatcher`].
    pub fn new(sim_table_list: &[SimTable]) -> SimMatcher {
        let mut process_type_set = HashSet::with_capacity(sim_table_list.len());
        let mut sim_processed_table_list = Vec::with_capacity(sim_table_list.len());

        for sim_table in sim_table_list {
            process_type_set.insert(sim_table.process_type.bits());
            sim_processed_table_list.push(SimProcessedTable {
                table_id: sim_table.table_id,
                match_id: sim_table.match_id,
                process_type: sim_table.process_type,
                sim_match_type: sim_table.sim_match_type,
                word_list: sim_table
                    .word_list
                    .iter()
                    .map(|&word| word.to_owned())
                    .collect::<Vec<String>>(),
                threshold: sim_table.threshold,
            })
        }

        let process_type_tree = build_process_type_tree(&process_type_set).into_boxed_slice();

        SimMatcher {
            process_type_tree,
            sim_processed_table_list: sim_processed_table_list.into_boxed_slice(),
        }
    }
}

impl<'a> TextMatcherTrait<'a, SimResult<'a>> for SimMatcher {
    /// Checks if the provided text matches any entry in the processed tables.
    ///
    /// This function processes the input text to generate a set of processed text variants
    /// based on the defined process types. It then delegates the actual matching logic to a
    /// helper function that checks if any of these processed text variants match the entries
    /// in the `sim_processed_table_list`.
    ///
    /// # Arguments
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    /// `true` if the processed text matches any entry in the processed tables; otherwise returns `false`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimTable, SimMatchType, ProcessType, SimMatcher, TextMatcherTrait};
    ///
    /// let sim_table = SimTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     process_type: ProcessType::None,
    ///     sim_match_type: SimMatchType::Levenshtein,
    ///     word_list: vec!["hello"],
    ///     threshold: 0.6,
    /// };
    ///
    /// let matcher = SimMatcher::new(&[sim_table]);
    ///
    /// assert!(matcher.is_match("helo")); // Matches due to high similarity
    /// assert!(!matcher.is_match("world")); // Does not match
    /// ```
    fn is_match(&'a self, text: &'a str) -> bool {
        if text.is_empty() {
            return false;
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.is_match_preprocessed(&processed_text_process_type_masks)
    }
    /// Returns a **lazy** iterator over [`SimResult`] matches for the given text.
    ///
    /// Text preprocessing (`reduce_text_process_with_tree`) is performed once upfront.
    /// Each similarity comparison is then driven lazily — one word at a time — as the
    /// caller advances the iterator. Early termination (e.g. `.next()`, `.take(n)`,
    /// `.find()`) avoids unnecessary similarity computations.
    ///
    /// Internally a 3-level index state machine (`processed_text` → `table` → `word`)
    /// tracks progress. A [`HashSet`] deduplicates `(table_id, word_index)` pairs across
    /// processed-text variants.
    ///
    /// # Arguments
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    /// An `impl Iterator<Item = SimResult<'a>>` — a lazy iterator of similarity results.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimTable, SimMatchType, ProcessType, SimMatcher, TextMatcherTrait};
    ///
    /// let sim_table = SimTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     process_type: ProcessType::None,
    ///     sim_match_type: SimMatchType::Levenshtein,
    ///     word_list: vec!["apple"],
    ///     threshold: 0.8,
    /// };
    ///
    /// let matcher = SimMatcher::new(&[sim_table]);
    ///
    /// let mut iter = matcher.process_iter("appple"); // Minor typo
    /// assert!(iter.next().is_some());
    /// assert!(iter.next().is_none());
    /// ```
    fn process_iter(&'a self, text: &'a str) -> impl Iterator<Item = SimResult<'a>> + 'a {
        gen move {
            if text.is_empty() {
                return;
            }

            let processed = reduce_text_process_with_tree(&self.process_type_tree, text);
            let mut table_id_index_set = HashSet::new();

            for (processed_text, process_type_mask) in processed {
                for sim_processed_table in self.sim_processed_table_list.iter() {
                    if (process_type_mask & (1u64 << sim_processed_table.process_type.bits())) == 0
                    {
                        continue;
                    }

                    match sim_processed_table.sim_match_type {
                        SimMatchType::Levenshtein => {
                            for (index, sim_text) in
                                sim_processed_table.word_list.iter().enumerate()
                            {
                                let table_id_index =
                                    ((sim_processed_table.table_id as usize) << 32) | index;

                                if table_id_index_set.insert(table_id_index)
                                    && let Some(similarity) =
                                        distance::levenshtein::normalized_similarity_with_args(
                                            sim_text.chars(),
                                            processed_text.chars(),
                                            &distance::levenshtein::Args::default()
                                                .score_cutoff(sim_processed_table.threshold),
                                        )
                                {
                                    yield SimResult {
                                        match_id: sim_processed_table.match_id,
                                        table_id: sim_processed_table.table_id,
                                        word_id: index as u32,
                                        word: Cow::Borrowed(sim_text),
                                        similarity,
                                    };
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

impl<'a> TextMatcherInternal<'a, SimResult<'a>> for SimMatcher {
    /// Checks if any processed text variant matches an entry in the similarity tables.
    ///
    /// # Algorithm
    /// This helper iterates through the processed text variants and their corresponding process type
    /// sets. For each variant, it checks against entries in the `sim_processed_table_list`.
    ///
    /// If the algorithm is `SimMatchType::Levenshtein`, it defers to `rapidfuzz`'s `normalized_similarity_with_args`.
    /// Crucially, we pass `.chars()` iterators instead of raw bytes (`&str`) to ensure UTF-8 multibyte
    /// characters are correctly evaluated as single entities during alignment edits. A `score_cutoff` is
    /// supplied equal to the `sim_processed_table.threshold` to heavily optimize and short-circuit the
    /// internal dynamic programming matrix evaluation if the distance is irrecoverably poor.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple contains a processed text variant (as [`Cow<'a, str>`]) and a `u64` bitmask of applicable process type IDs.
    ///
    /// # Returns
    /// Returns `true` if any of the processed text variants match an entry in the similarity tables
    /// according to the specified match type and similarity threshold; otherwise, returns `false`.
    fn is_match_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> bool {
        for (processed_text, process_type_mask) in processed_text_process_type_masks {
            for sim_processed_table in &self.sim_processed_table_list {
                if (process_type_mask & (1u64 << sim_processed_table.process_type.bits())) == 0 {
                    continue;
                }
                let is_match = match sim_processed_table.sim_match_type {
                    SimMatchType::Levenshtein => sim_processed_table.word_list.iter().any(|text| {
                        distance::levenshtein::normalized_similarity_with_args(
                            text.chars(),
                            processed_text.chars(),
                            &distance::levenshtein::Args::default()
                                .score_cutoff(sim_processed_table.threshold),
                        )
                        .is_some()
                    }),
                };

                if is_match {
                    return true;
                }
            }
        }

        false
    }

    /// Processes the provided set of processed text variants and their corresponding process type sets,
    /// returning a list of similarity results.
    ///
    /// # Algorithm
    /// This function iterates through each processed text variant, comparing it against
    /// entries in the similarity tables via `rapidfuzz::distance::levenshtein::normalized_similarity_with_args`.
    ///
    /// - **UTF-8 Safety**: We pass `chars()` to `rapidfuzz` rather than bytes.
    /// - **Early Exit Thresholds**: `score_cutoff(sim_processed_table.threshold)` is utilized entirely inside `rapidfuzz`
    ///   so that it bounds the required matrix computation dynamically.
    /// - **Deduplication**: As different process conditions (like upper vs lower) might trigger positive matches
    ///   for the same entry string, a `HashSet` tracks `(table_id << 32) | index` to guarantee the match tuple
    ///   is only yielded exactly once per unique matched pattern, regardless of the intermediate process text form.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple contains a processed text variant (as [`Cow<'a, str>`]) and a `u64` bitmask of applicable process type IDs.
    ///
    /// # Returns
    /// Returns a vector of [`SimResult`] instances, each containing information about a matched entry
    /// in the similarity tables, including:
    /// - `match_id`: A unique identifier for the match operation.
    /// - `table_id`: A unique identifier for the specific matching table.
    /// - `word_id`: A unique identifier for the word within the table.
    /// - `word`: The word from the similarity table's word list that matched the processed text.
    /// - `similarity`: The similarity score of the match.
    fn process_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<SimResult<'a>> {
        let mut result_list = Vec::new();
        let mut table_id_index_set = HashSet::new();

        for (processed_text, process_type_mask) in processed_text_process_type_masks {
            for sim_processed_table in &self.sim_processed_table_list {
                if (process_type_mask & (1u64 << sim_processed_table.process_type.bits())) == 0 {
                    continue;
                }
                match sim_processed_table.sim_match_type {
                    SimMatchType::Levenshtein => {
                        for (index, text) in sim_processed_table.word_list.iter().enumerate() {
                            let table_id_index =
                                ((sim_processed_table.table_id as usize) << 32) | index;

                            if table_id_index_set.insert(table_id_index)
                                && let Some(similarity) =
                                    distance::levenshtein::normalized_similarity_with_args(
                                        text.chars(),
                                        processed_text.chars(),
                                        &distance::levenshtein::Args::default()
                                            .score_cutoff(sim_processed_table.threshold),
                                    )
                            {
                                result_list.push(SimResult {
                                    match_id: sim_processed_table.match_id,
                                    table_id: sim_processed_table.table_id,
                                    word_id: index as u32,
                                    word: Cow::Borrowed(text),
                                    similarity,
                                });
                            }
                        }
                    }
                }
            }
        }

        result_list
    }
}
