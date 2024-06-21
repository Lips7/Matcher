use std::borrow::Cow;

use fancy_regex::Regex;
use rapidfuzz::distance;
use sonic_rs::{Deserialize, Serialize};

use crate::{MatchResultTrait, TextMatcherTrait};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
/// An enumeration representing different types of similarity matching algorithms.
///
/// The [SimMatchType] enum defines several types of algorithms that can be used
/// for similarity matching operations. Each variant corresponds to a specific
/// algorithm, providing flexibility in choosing the appropriate method based on
/// the use case.
///
/// # Variants
///
/// - [Levenshtein](SimMatchType::Levenshtein): Represents the Levenshtein distance algorithm, which calculates
///   the number of single-character edits (insertions, deletions, or substitutions)
///   required to change one word into another.
/// - [DamerauLevenshtein](SimMatchType::DamerauLevenshtein): Represents the Damerau-Levenshtein distance algorithm,
///   an extension of Levenshtein that also considers transpositions (swapping of
///   two adjacent characters) as a single edit.
/// - [Indel](SimMatchType::Indel): Represents the Insertion-Deletion distance algorithm, focusing on
///   insertions and deletions as the only operations.
/// - [Jaro](SimMatchType::Jaro): Represents the Jaro distance algorithm, measuring the similarity between
///   two strings based on the number and order of matching characters.
/// - [JaroWinkler](SimMatchType::JaroWinkler): Represents the Jaro-Winkler distance algorithm, a variant of Jaro
///   that gives more favorable ratings to strings that match from the beginning.
///
/// This enum can be serialized and deserialized using Serde, with the variant names
/// automatically converted to snake_case during this process.
pub enum SimMatchType {
    Levenshtein,
    DamerauLevenshtein,
    Indel,
    Jaro,
    JaroWinkler,
}

#[derive(Debug, Clone)]
/// A struct representing a similarity table used for matching operations.
///
/// The [SimTable] struct is used to define a table of words and associated identifiers that
/// will be used in similarity matching. Each table has an ID, a match identifier, a list of words,
/// and a threshold for scoring.
///
/// The lifetime `'a` ensures that the references to the word list remain valid for as long as
/// the `SimTable` instance exists.
///
/// # Fields
///
/// - `table_id` ([u64]): The unique identifier for the similarity table.
/// - `match_id` ([u64]): An ID that serves as an identifier for the match within the table.
/// - `sim_match_type` ([SimMatchType]): The type of similarity matching algorithm to be used
///   with this table.
/// - `word_list` ([&'a Vec<&'a str>]): A reference to a vector of string slices representing
///   the words in this similarity table. These words will be used in the matching process.
/// - `threshold` ([f64]): The threshold value for similarity scoring. This score typically
///   ranges from 0.0 to 1.0, with higher values indicating higher similarity.
///
/// # Example
///
/// ```
/// use matcher_rs::{SimTable, SimMatchType};
///
/// let words = vec!["example1", "example2"];
///
/// let table = SimTable {
///     table_id: 1,
///     match_id: 1,
///     sim_match_type: SimMatchType::Levenshtein,
///     word_list: &words,
///     threshold: 0.8,
/// };
/// ```
pub struct SimTable<'a> {
    pub table_id: u64,
    pub match_id: u64,
    pub sim_match_type: SimMatchType,
    pub word_list: &'a Vec<&'a str>,
    pub threshold: f64,
}

#[derive(Debug, Clone)]
/// A struct representing a processed similarity table.
///
/// The [SimProcessedTable] struct holds the preprocessed data for similarity matching operations.
/// After a [SimTable] has been processed, its data is converted and stored in this struct, which
/// includes all necessary information for performing match operations, such as the unique table ID,
/// match ID, type of similarity matching algorithm used, a list of words, and the threshold for
/// similarity scoring.
///
/// # Fields
///
/// - `table_id` ([u64]): The unique identifier for the similarity table.
/// - `match_id` ([u64]): An ID that serves as an identifier for the match within the table.
/// - `sim_match_type` ([SimMatchType]): The type of similarity matching algorithm used for this table.
/// - `word_list` ([`Vec<String>`]): A vector of owned strings representing the words in this similarity table.
///   These words have been preprocessed and are ready for the matching process.
/// - `threshold` ([f64]): The threshold value for similarity scoring. This score ranges from 0.0 to 1.0,
///   with higher values indicating higher similarity.
///
struct SimProcessedTable {
    table_id: u64,
    match_id: u64,
    sim_match_type: SimMatchType,
    word_list: Vec<String>,
    threshold: f64,
}

#[derive(Debug, Clone)]
/// A struct representing the result of a similarity match.
///
/// The [SimResult] struct captures the details of a word that was found to be similar
/// during the similarity matching process. It includes the matched word, the unique
/// identifier of the table where the word was found, the match identifier of that table,
/// and the similarity score computed for the match.
///
/// The lifetimes ensure that the references in the [SimResult] struct remain valid
/// for as long as the struct instance exists.
///
/// # Fields
///
/// - `word` ([Cow<'a, str>]): The word that was found to be similar. It is stored as a [Cow]
///   (clone-on-write) to allow for both owned and borrowed strings.
/// - `table_id` ([u64]): The unique identifier of the table where the word was found.
/// - `match_id` ([u64]): An ID that serves as an identifier for the match.
/// - `similarity` ([f64]): The similarity score computed for the match. This score typically
///   ranges from 0.0 to 1.0, with higher values indicating greater similarity.
///
/// # Example
///
/// ```
/// use matcher_rs::SimResult;
/// use std::borrow::Cow;
///
/// let match_result = SimResult {
///     word: Cow::Borrowed("example"),
///     table_id: 1,
///     match_id: 1,
///     similarity: 0.9,
/// };
/// ```
pub struct SimResult<'a> {
    pub word: Cow<'a, str>,
    pub table_id: u64,
    pub match_id: u64,
    pub similarity: f64,
}

impl MatchResultTrait<'_> for SimResult<'_> {
    fn table_id(&self) -> u64 {
        self.table_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

#[derive(Debug, Clone)]
/// A struct representing a similarity matcher.
///
/// The [SimMatcher] struct is responsible for managing and processing similarity matching
/// operations on provided textual data using predefined tables. It includes functionality
/// to preprocess text by removing special characters and to search for matches within
/// the preprocessed tables using normalized Levenshtein similarity.
///
/// # Fields
///
/// - `remove_special_pattern` ([Regex]): A compiled regular expression used for removing
///   special characters from the text before processing.
/// - `sim_processed_table_list` ([`Vec<SimProcessedTable>`]): A vector containing preprocessed
///   tables, where each table consists of a list of words and identifiers ready for
///   similarity matching.
///
/// # Example
///
/// ```
/// use matcher_rs::{SimMatcher, SimTable, SimMatchType};
///
/// let word_list = vec!["example1", "example2"];
///
/// let sim_tables = vec![
///     SimTable {
///         table_id: 1,
///         match_id: 1,
///         sim_match_type: SimMatchType::Levenshtein,
///         word_list: &word_list,
///         threshold: 0.8,
///     },
///     // Add more SimTable instances as desired
/// ];
///
/// let matcher = SimMatcher::new(&sim_tables);
/// ```
pub struct SimMatcher {
    remove_special_pattern: Regex,
    sim_processed_table_list: Vec<SimProcessedTable>,
}

impl SimMatcher {
    /// Creates a new instance of [SimMatcher] by preprocessing the provided list of [SimTable] instances.
    ///
    /// This function takes a reference to a list of [SimTable] instances provided by the user and
    /// preprocesses each table to create corresponding `SimProcessedTable` instances. The preprocessing
    /// involves compiling a regular expression for removing special characters and converting the
    /// words and match identifiers to owned [String] types.
    ///
    /// # Parameters
    ///
    /// - `sim_table_list` (&[SimTable]): A reference to a slice of [SimTable] instances to be preprocessed.
    ///
    /// # Returns
    ///
    /// - [SimMatcher]: A new instance of [SimMatcher] with preprocessed tables ready for similarity matching.
    ///
    /// # Example
    ///
    /// ```
    /// use matcher_rs::{SimMatcher, SimTable, SimMatchType};
    ///
    /// let word_list = vec!["example1", "example2"];
    ///
    /// let sim_tables = vec![
    ///     SimTable {
    ///         table_id: 1,
    ///         match_id: 1,
    ///         sim_match_type: SimMatchType::Levenshtein,
    ///         word_list: &word_list,
    ///         threshold: 0.8,
    ///     },
    ///     // Add more SimTable instances as desired
    /// ];
    ///
    /// let matcher = SimMatcher::new(&sim_tables);
    /// ```
    pub fn new(sim_table_list: &[SimTable]) -> SimMatcher {
        SimMatcher {
            remove_special_pattern: Regex::new(r"\W+").unwrap(),
            sim_processed_table_list: sim_table_list
                .iter()
                .map(|sim_table| SimProcessedTable {
                    table_id: sim_table.table_id,
                    match_id: sim_table.match_id,
                    sim_match_type: sim_table.sim_match_type,
                    word_list: sim_table
                        .word_list
                        .iter()
                        .map(|&word| word.to_owned())
                        .collect::<Vec<String>>(),
                    threshold: sim_table.threshold,
                })
                .collect(),
        }
    }
}

impl<'a> TextMatcherTrait<'a, SimResult<'a>> for SimMatcher {
    /// Checks if the given text has any similarity match within the preprocessed tables.
    ///
    /// This function processes the input text by removing special characters and then
    /// checks if the processed text has any similarity match within the preprocessed tables.
    /// Various similarity metrics are used based on the type specified in each table.
    /// The function returns `true` if there is any match that meets the threshold specified
    /// for similarity, otherwise `false`.
    ///
    /// # Parameters
    ///
    /// - `text` (&str): A reference to the text string to be processed and checked
    ///   against the preprocessed tables for similarity matches.
    ///
    /// # Returns
    ///
    /// - (bool): `true` if a similarity match is found that meets the specified threshold, otherwise `false`.
    ///
    /// # Example
    ///
    /// ```
    /// use matcher_rs::{SimMatcher, SimTable, TextMatcherTrait, SimMatchType};
    ///
    /// let word_list = vec!["example1", "example2"];
    ///
    /// let sim_tables = vec![
    ///     SimTable {
    ///         table_id: 1,
    ///         match_id: 1,
    ///         word_list: &word_list,
    ///         sim_match_type: SimMatchType::Levenshtein,
    ///         threshold: 0.8,
    ///     },
    ///     // Add more SimTable instances as desired
    /// ];
    ///
    /// let matcher = SimMatcher::new(&sim_tables);
    ///
    /// let is_matched = matcher.is_match("example3");
    ///
    /// if is_matched {
    ///     println!("The text has a similarity match in the preprocessed tables.");
    /// } else {
    ///     println!("No similarity match found.");
    /// }
    /// ```
    fn is_match(&self, text: &str) -> bool {
        let processed_text = self.remove_special_pattern.replace_all(text, "");

        self.sim_processed_table_list
            .iter()
            .any(|sim_table| match sim_table.sim_match_type {
                SimMatchType::Levenshtein => sim_table.word_list.iter().any(|text| {
                    distance::levenshtein::normalized_similarity_with_args(
                        text.chars(),
                        processed_text.chars(),
                        &distance::levenshtein::Args::default().score_cutoff(sim_table.threshold),
                    )
                    .is_some()
                }),
                SimMatchType::DamerauLevenshtein => sim_table.word_list.iter().any(|text| {
                    distance::damerau_levenshtein::normalized_similarity_with_args(
                        text.chars(),
                        processed_text.chars(),
                        &distance::damerau_levenshtein::Args::default()
                            .score_cutoff(sim_table.threshold),
                    )
                    .is_some()
                }),
                SimMatchType::Indel => sim_table.word_list.iter().any(|text| {
                    distance::indel::normalized_similarity_with_args(
                        text.chars(),
                        processed_text.chars(),
                        &distance::indel::Args::default().score_cutoff(sim_table.threshold),
                    )
                    .is_some()
                }),
                SimMatchType::Jaro => sim_table.word_list.iter().any(|text| {
                    distance::jaro::normalized_similarity_with_args(
                        text.chars(),
                        processed_text.chars(),
                        &distance::jaro::Args::default().score_cutoff(sim_table.threshold),
                    )
                    .is_some()
                }),
                SimMatchType::JaroWinkler => sim_table.word_list.iter().any(|text| {
                    distance::jaro_winkler::normalized_similarity_with_args(
                        text.chars(),
                        processed_text.chars(),
                        &distance::jaro_winkler::Args::default().score_cutoff(sim_table.threshold),
                    )
                    .is_some()
                }),
            })
    }

    /// Processes the input text and returns a list of similarity results based on the
    /// preprocessed tables and their respective similarity match types and thresholds.
    ///
    /// This function removes special characters from the input text, then iterates through
    /// each preprocessed similarity table to calculate the similarity scores between the
    /// processed input text and each word in the table's word list. The results are collected
    /// into a vector of [SimResult] instances for each word that meets the similarity threshold.
    ///
    /// # Parameters
    ///
    /// - `text` (&str): A reference to the text string to be processed and checked against
    ///   the preprocessed tables for similarity matches.
    ///
    /// # Returns
    ///
    /// - `Vec<SimResult>`: A vector containing [SimResult] instances for each word that meets
    ///   the similarity threshold specified in the corresponding similarity table.
    ///
    /// # Example
    ///
    /// ```
    /// use matcher_rs::{SimMatcher, SimTable, TextMatcherTrait, SimResult, SimMatchType};
    ///
    /// let word_list = vec!["example1", "example2"];
    ///
    /// let sim_tables = vec![
    ///     SimTable {
    ///         table_id: 1,
    ///         match_id: 1,
    ///         word_list: &word_list,
    ///         sim_match_type: SimMatchType::Levenshtein,
    ///         threshold: 0.8,
    ///     },
    ///     // Add more SimTable instances as desired
    /// ];
    ///
    /// let matcher = SimMatcher::new(&sim_tables);
    ///
    /// let results: Vec<SimResult> = matcher.process("example3");
    ///
    /// for result in results {
    ///     println!(
    ///         "Found match in table {}: word={}, similarity={}",
    ///         result.table_id, result.word, result.similarity
    ///     );
    /// }
    /// ```
    fn process(&'a self, text: &str) -> Vec<SimResult<'a>> {
        let processed_text = self.remove_special_pattern.replace_all(text, "");

        let mut result_list = Vec::new();

        for sim_table in &self.sim_processed_table_list {
            match sim_table.sim_match_type {
                SimMatchType::Levenshtein => {
                    result_list.extend(sim_table.word_list.iter().filter_map(|text| {
                        distance::levenshtein::normalized_similarity_with_args(
                            text.chars(),
                            processed_text.chars(),
                            &distance::levenshtein::Args::default()
                                .score_cutoff(sim_table.threshold),
                        )
                        .map(|similarity| SimResult {
                            word: Cow::Borrowed(text),
                            table_id: sim_table.table_id,
                            match_id: sim_table.match_id,
                            similarity,
                        })
                    }));
                }
                SimMatchType::DamerauLevenshtein => {
                    result_list.extend(sim_table.word_list.iter().filter_map(|text| {
                        distance::damerau_levenshtein::normalized_similarity_with_args(
                            text.chars(),
                            processed_text.chars(),
                            &distance::damerau_levenshtein::Args::default()
                                .score_cutoff(sim_table.threshold),
                        )
                        .map(|similarity| SimResult {
                            word: Cow::Borrowed(text),
                            table_id: sim_table.table_id,
                            match_id: sim_table.match_id,
                            similarity,
                        })
                    }));
                }
                SimMatchType::Indel => {
                    result_list.extend(sim_table.word_list.iter().filter_map(|text| {
                        distance::indel::normalized_similarity_with_args(
                            text.chars(),
                            processed_text.chars(),
                            &distance::indel::Args::default().score_cutoff(sim_table.threshold),
                        )
                        .map(|similarity| SimResult {
                            word: Cow::Borrowed(text),
                            table_id: sim_table.table_id,
                            match_id: sim_table.match_id,
                            similarity,
                        })
                    }));
                }
                SimMatchType::Jaro => {
                    result_list.extend(sim_table.word_list.iter().filter_map(|text| {
                        distance::jaro::normalized_similarity_with_args(
                            text.chars(),
                            processed_text.chars(),
                            &distance::jaro::Args::default().score_cutoff(sim_table.threshold),
                        )
                        .map(|similarity| SimResult {
                            word: Cow::Borrowed(text),
                            table_id: sim_table.table_id,
                            match_id: sim_table.match_id,
                            similarity,
                        })
                    }));
                }
                SimMatchType::JaroWinkler => {
                    result_list.extend(sim_table.word_list.iter().filter_map(|text| {
                        distance::jaro_winkler::normalized_similarity_with_args(
                            text.chars(),
                            processed_text.chars(),
                            &distance::jaro_winkler::Args::default()
                                .score_cutoff(sim_table.threshold),
                        )
                        .map(|similarity| SimResult {
                            word: Cow::Borrowed(text),
                            table_id: sim_table.table_id,
                            match_id: sim_table.match_id,
                            similarity,
                        })
                    }));
                }
            }
        }

        result_list
    }
}
