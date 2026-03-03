use std::{borrow::Cow, collections::HashSet};

use rapidfuzz::distance;
use rustc_hash::FxHashSet;
use serde::{Deserialize, Serialize};

use crate::{
    matcher::{MatchResultTrait, TextMatcherTrait},
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
/// * `word` - The word that was matched, represented as a `Cow`.
/// * `similarity` - A float value representing the similarity score of the match.
#[derive(Debug, Clone)]
pub struct SimResult<'a> {
    pub match_id: u32,
    pub table_id: u32,
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

/// The [`SimMatcher`] struct is responsible for performing similarity matching.
///
/// It uses edit-distance algorithms to find matches that are "close enough" to the target patterns,
/// allowing for typos or minor variations in the text.
///
/// # Detailed Explanation / Algorithm
/// 1. **Initialization**:
///    - Converts borrowed word lists into owned strings (`SimProcessedTable`).
///    - Compiles a `ProcessTypeBitNode` DAG to handle text normalization variants.
/// 2. **Matching**:
///    - Iterates through all pre-processed text variants.
///    - For each variant, it iterates through all configured similarity tables.
///    - Uses `rapidfuzz`'s Levenshtein distance implementation for the actual comparison.
///    - **Performance**: It uses `.chars()` iterators for UTF-8 safety and a `score_cutoff`
///      to allow `rapidfuzz` to short-circuit if the distance exceeds the threshold.
///
/// # Fields
/// * `process_type_tree` - Workflow tree for efficient text transforms.
/// * `sim_processed_table_list` - Configured similarity rules with owned word lists.
///
/// # Examples
/// ```rust
/// use matcher_rs::{SimMatcher, SimTable, SimMatchType, ProcessType, TextMatcherTrait};
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
/// assert!(matcher.is_match("aple")); // Matches due to similarity
/// ```
#[derive(Debug, Clone)]
pub struct SimMatcher {
    process_type_tree: Box<[ProcessTypeBitNode]>,
    sim_processed_table_list: Box<[SimProcessedTable]>,
}

impl SimMatcher {
    /// Creates a new [`SimMatcher`] from a list of [`SimTable`].
    ///
    /// # Detailed Explanation / Algorithm
    /// This method initializes the matcher by:
    /// 1. Collecting all unique `ProcessType` bits to build the transformation tree.
    /// 2. Deep-copying the word lists into owned `String` vectors for long-term storage.
    ///
    /// # Arguments
    /// * `sim_table_list` - Configuration data for all similarity rules.
    ///
    /// # Returns
    /// A fully initialized [`SimMatcher`].
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

    /// Processes the given text and returns a vector of matching results.
    ///
    /// This function applies the process type tree to the text and passes the processed text
    /// variants to the `process_preprocessed` helper function for matching against the similarity tables.
    ///
    /// # Arguments
    /// * `text` - A string slice representing the input text to be processed.
    ///
    /// # Returns
    /// A vector of [`SimResult`] containing the matched words and their identifiers.
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
    /// let results = matcher.process("helo");
    /// assert_eq!(results.len(), 1);
    /// assert_eq!(results[0].word, "hello");
    /// ```
    fn process(&'a self, text: &'a str) -> Vec<SimResult<'a>> {
        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.process_preprocessed(&processed_text_process_type_masks)
    }

    /// Checks if any pre-processed text variant is similar enough to any rule pattern.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Iterates through the pre-processed text variations.
    /// 2. For each variant, checks all tables allowed by the variant's `process_type_mask`.
    /// 3. Executes `rapidfuzz` Levenshtein similarity check. It uses `chars()` for UTF-8
    ///    character-level distance and sets a `score_cutoff` for early short-circuiting.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - Pre-processed text variants and bitmasks.
    ///
    /// # Returns
    /// `true` if a match meets the similarity threshold.
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

    /// Evaluates pre-processed text and returns all unique similarity match results.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Iterates through all text variations and similarity tables.
    /// 2. For each word in an allowed table, calculates normalized Levenshtein similarity.
    /// 3. Deduplicates results by `(table_id, index)` to ensure each rule only triggers once.
    /// 4. Collects and returns all results meeting the threshold.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - Pre-processed text variants and bitmasks.
    ///
    /// # Returns
    /// A vector of [`SimResult`] matches.
    fn process_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<SimResult<'a>> {
        let mut result_list = Vec::new();
        let mut table_id_index_set = FxHashSet::default();

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
