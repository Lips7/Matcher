use std::borrow::Cow;

use id_set::IdSet;
use rapidfuzz::distance;
use serde::{Deserialize, Serialize};

use crate::{
    matcher::{MatchResultTrait, TextMatcherTrait},
    process::process_matcher::{
        build_process_type_tree, reduce_text_process_with_tree, ProcessType, ProcessTypeBitNode,
    },
};

/// Enumeration representing the types of similarity matching algorithms available.
///
/// Currently, this enum only supports the Levenshtein distance algorithm.
///
/// # Variants
///
/// * [SimMatchType::Levenshtein] - Represents the Levenshtein distance algorithm, a string metric for measuring the difference between two sequences.
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
/// # Fields
///
/// * `table_id` - A unique identifier for the table.
/// * `match_id` - A unique identifier for the matching process.
/// * `process_type` - The type of processing to be applied, represented by the [ProcessType] enum.
/// * `sim_match_type` - The type of similarity matching algorithm to be used, represented by the [SimMatchType] enum.
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
/// This struct is a concrete version of the [SimTable] struct, with ownership over
/// the word list.
///
/// # Fields
///
/// * `table_id` - A unique identifier for the table.
/// * `match_id` - A unique identifier for the matching process.
/// * `process_type` - The type of processing to be applied, represented by the [ProcessType] enum.
/// * `sim_match_type` - The type of similarity matching algorithm to be used, represented by the [SimMatchType] enum.
/// * `word_list` - A list of words over which the matching operation is performed. This is an owned vector of strings.
/// * `threshold` - A float value representing the similarity threshold for a match.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
/// [Cow] (Clone on Write) for efficient handling of borrowed or owned strings. This allows
/// flexibility in returning either a borrowed string or an owned string.
///
/// # Fields
///
/// * `match_id` - A unique identifier for the matching process.
/// * `table_id` - A unique identifier for the table.
/// * `word_id` - A unique identifier for the word within the table.
/// * `word` - The word that was matched, represented as a [Cow] to allow for both borrowed and owned strings.
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
    fn similarity(&self) -> f64 {
        self.similarity
    }
}

/// The [SimMatcher] struct is responsible for performing similarity matching operations
/// based on different processing types and similarity algorithms.
///
/// This struct maintains a process type tree and a list of pre-processed tables that contain
/// the necessary information for performing similarity matching on texts.
///
/// # Fields
///
/// * `process_type_tree` - A vector of `ProcessTypeBitNode`, representing the tree structure used for
///   text processing based on defined process types.
/// * `sim_processed_table_list` - A vector of `SimProcessedTable`, holding the tables with processed information
///   for performing similarity matching.
///
/// # Example
///
/// ```
/// use matcher_rs::{SimMatcher, SimTable, SimMatchType, ProcessType};
///
/// // Create a list of `SimTable` with the required properties
/// let sim_table_list = vec![SimTable {
///     table_id: 1,
///     match_id: 1,
///     process_type: ProcessType::None,
///     sim_match_type: SimMatchType::Levenshtein,
///     word_list: vec!["example", "test"],
///     threshold: 0.8,
/// }];
///
/// // Instantiate a `SimMatcher` with the list of `SimTable`
/// let matcher = SimMatcher::new(&sim_table_list);
///
/// // Use `matcher` methods for performing similarity matching operations
/// ```
///
/// The [SimMatcher] struct provides methods for checking if a text matches any of the processed tables
/// and for processing texts to obtain a list of similarity results.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SimMatcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    sim_processed_table_list: Vec<SimProcessedTable>,
}

impl SimMatcher {
    /// Creates a new instance of [SimMatcher] from a list of [SimTable].
    ///
    /// This function initializes a [SimMatcher] by processing each [SimTable] in the input list.
    /// It extracts the process types and constructs a tree structure used for processing texts.
    /// Additionally, it converts the word lists in each [SimTable] from borrowed strings to owned strings.
    ///
    /// # Parameters
    ///
    /// * `sim_table_list` - A slice of [SimTable] references to be processed and included in the new [SimMatcher] instance.
    ///
    /// # Returns
    ///
    /// Returns a new instance of [SimMatcher] containing:
    /// * `process_type_tree` - A vector of `ProcessTypeBitNode`, representing the tree structure used for text processing based on the process types extracted from the input [SimTable] list.
    /// * `sim_processed_table_list` - A vector of `SimProcessedTable`, each containing an owned vector of words and other properties derived from the input [SimTable] list.
    pub fn new(sim_table_list: &[SimTable]) -> SimMatcher {
        let mut process_type_set = IdSet::with_capacity(sim_table_list.len());
        let mut sim_processed_table_list = Vec::with_capacity(sim_table_list.len());

        for sim_table in sim_table_list {
            process_type_set.insert(sim_table.process_type.bits() as usize);
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

        let process_type_tree = build_process_type_tree(&process_type_set);

        SimMatcher {
            process_type_tree,
            sim_processed_table_list,
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
    /// # Parameters
    ///
    /// * `text` - A string slice representing the input text to be checked for similarity matches.
    ///
    /// # Returns
    ///
    /// Returns `true` if the processed text matches any entry in the processed tables; otherwise returns `false`.
    fn is_match(&'a self, text: &'a str) -> bool {
        if text.is_empty() {
            return false;
        }

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._is_match_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    /// Checks if any processed text variant matches an entry in the similarity tables.
    ///
    /// This helper function iterates through the processed text variants and their corresponding
    /// process type sets. For each variant, it checks against all entries in the similarity tables
    /// to see if there is a match based on the defined similarity match type (e.g., Levenshtein).
    ///
    /// # Parameters
    ///
    /// * `processed_text_process_type_set` - A reference to a list of tuples where each tuple consists of:
    ///   - A processed text variant represented as a [`Cow<str>`].
    ///   - An [IdSet] containing the process type identifiers associated with the processed text.
    ///
    /// # Returns
    ///
    /// Returns `true` if any of the processed text variants match an entry in the similarity tables
    /// according to the specified match type and similarity threshold; otherwise, returns `false`.
    fn _is_match_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, id_set::IdSet)],
    ) -> bool {
        for (processed_text, process_type_set) in processed_text_process_type_set {
            for sim_processed_table in &self.sim_processed_table_list {
                if !process_type_set.contains(sim_processed_table.process_type.bits() as usize) {
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

    /// Processes the provided text and returns a list of similarity results.
    ///
    /// This function takes the input text and generates a set of processed text variants based
    /// on the defined process types, as described in the `process_type_tree`. It then uses these
    /// variants to find matches in the similarity tables, accumulating results where a similarity
    /// match is found.
    ///
    /// # Parameters
    ///
    /// * `text` - A string slice representing the input text to be processed and checked for similarity matches.
    ///
    /// # Returns
    ///
    /// Returns a vector of [SimResult] instances, each containing information about a matched entry
    /// in the similarity tables, including the `match_id`, `table_id`, `word_id`, `word`, and the
    /// similarity score.
    fn process(&'a self, text: &'a str) -> Vec<SimResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._process_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    /// Processes the provided set of processed text variants and their corresponding process type sets,
    /// returning a list of similarity results.
    ///
    /// This function iterates through each processed text variant and its associated process type set,
    /// comparing them against entries in the similarity tables to identify matches based on the defined
    /// similarity match type (e.g., Levenshtein). For each match found, the function accumulates the result
    /// with relevant information such as `match_id`, `table_id`, `word_id`, `word`, and the similarity score.
    ///
    /// # Parameters
    ///
    /// * `processed_text_process_type_set` - A reference to a list of tuples where each tuple consists of:
    ///   - A processed text variant represented as a [`Cow<str>`].
    ///   - An [IdSet] containing the process type identifiers associated with the processed text.
    ///
    /// # Returns
    ///
    /// Returns a vector of [SimResult] instances, each containing information about a matched entry
    /// in the similarity tables, including:
    /// - `match_id`: The identifier for the match.
    /// - `table_id`: The identifier of the similarity table where the match was found.
    /// - `word_id`: The index of the word in the similarity table's word list.
    /// - `word`: The word from the similarity table's word list that matched the processed text.
    /// - `similarity`: The similarity score of the match.
    ///
    /// The function ensures that only unique matches are included in the result list by maintaining
    /// an [IdSet] to track already processed table ID and word index combinations.
    fn _process_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> Vec<SimResult<'a>> {
        let mut result_list = Vec::new();
        let mut table_id_index_set = IdSet::new();

        for (processed_text, process_type_set) in processed_text_process_type_set {
            for sim_processed_table in &self.sim_processed_table_list {
                if !process_type_set.contains(sim_processed_table.process_type.bits() as usize) {
                    continue;
                }
                match sim_processed_table.sim_match_type {
                    SimMatchType::Levenshtein => {
                        for (index, text) in sim_processed_table.word_list.iter().enumerate() {
                            let table_id_index =
                                ((sim_processed_table.table_id as usize) << 32) | index;

                            if table_id_index_set.insert(table_id_index) {
                                if let Some(similarity) =
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
        }

        result_list
    }
}
