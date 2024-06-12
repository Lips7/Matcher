use std::borrow::Cow;

use fancy_regex::Regex;
use rapidfuzz::distance::levenshtein;

use super::{MatchResultTrait, TextMatcherTrait};

/// A struct representing a processed similarity table with a unique identifier,
/// a match identifier, and a list of processed words.
///
/// The `SimProcessedTable` struct stores the following information:
/// - `table_id`: A unique identifier for the table.
/// - `match_id`: A string identifier for matching purposes.
/// - `word_list`: A vector of strings representing the processed word list.
///
/// This struct is utilized within the `SimMatcher` to store processed versions
/// of the words for similarity matching.
///
/// # Fields
/// - `table_id` (u64): The unique identifier for the table.
/// - `match_id` (String): The match identifier.
/// - `word_list` (Vec<String>): The list of processed words.
pub struct SimTable<'a> {
    pub table_id: u64,
    pub match_id: &'a str,
    pub word_list: Vec<&'a str>,
}

/// A struct representing a processed similarity table used for efficient
/// similarity matching.
///
/// The `SimProcessedTable` struct stores the following information:
/// - `table_id`: A unique identifier for the processed table.
/// - `match_id`: A string identifier for matching purposes.
/// - `word_list`: A vector of processed words.
///
/// This struct is used internally within the `SimMatcher` to store and manage
/// the words after processing for special pattern removal and normalization.
///
/// # Fields
/// - `table_id` (u64): The unique identifier for the processed table.
/// - `match_id` (String): The match identifier.
/// - `word_list` (Vec<String>): The list of processed words.
struct SimProcessedTable {
    table_id: u64,
    match_id: String,
    word_list: Vec<String>,
}

#[derive(Debug)]
/// A struct representing the result of a similarity match.
///
/// The `SimResult` struct is used to store the details of a matched word along
/// with its associated table identifier, match identifier, and similarity score.
/// It is designed to be used within the `SimMatcher` for retrieving and handling
/// the results of the similarity matching process.
///
/// # Fields
///
/// - `word` (Cow<'a, str>): A `Cow` (Copy-On-Write) that holds either a borrowed
///   or owned string slice representing the matched word.
/// - `table_id` (u64): The unique identifier of the table where the word was found.
/// - `match_id` (&'a str): A reference to the match identifier string associated
///   with the table.
/// - `similarity` (f64): The similarity score of the match, typically ranging from
///   0.0 to 1.0, indicating how closely the `word` matches the processed text.
pub struct SimResult<'a> {
    pub word: Cow<'a, str>,
    pub table_id: u64,
    pub match_id: &'a str,
    pub similarity: f64,
}

impl MatchResultTrait<'_> for SimResult<'_> {
    /// Provides implementations for the methods defined in the `MatchResultTrait` for the `SimResult` struct.
    ///
    /// The `table_id` method returns the unique identifier of the table where the word was found.
    ///
    /// The `word` method returns a reference to the word from the processed list that was found to be similar.
    ///
    /// These methods allow the `SimResult` struct to fulfill the `MatchResultTrait` and provide the necessary
    /// functionality for accessing the table identifier and the matched word in a generic way.
    ///
    /// # Methods
    ///
    /// - `table_id(&self) -> u64`: Returns the unique identifier of the table.
    /// - `word(&self) -> &str`: Returns a reference to the word from the processed list.
    fn table_id(&self) -> u64 {
        self.table_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

/// A struct representing a similarity matcher.
///
/// The `SimMatcher` struct provides functionality for preprocessing text by
/// removing special characters and then performing similarity matching against
/// a list of preprocessed tables.
///
/// # Fields
///
/// - `remove_special_pattern` (Regex): A regular expression pattern for removing
///   special characters from the text.
/// - `sim_processed_table_list` (Vec<SimProcessedTable>): A vector of preprocessed
///   tables, each containing a unique identifier, match identifier, and list of
///   processed words.
///
/// The `SimMatcher` struct includes methods for creating a new instance from a list
/// of `SimTable`s, checking if a given text matches any processed word, and processing
/// a given text to return detailed match results.
///
/// # Example
///
/// ```
/// use matcher_rs::{SimMatcher, SimTable};
///
/// let word_list = vec!["example1", "example2"];
///
/// let sim_tables = vec![
///     SimTable {
///         table_id: 1,
///         match_id: "match1",
///         word_list: word_list,
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
    /// Creates a new `SimMatcher` instance from a list of `SimTable` references.
    ///
    /// This function takes a reference to a vector of `SimTable` instances and performs
    /// the necessary preprocessing to convert each `SimTable` into a `SimProcessedTable`.
    /// It initializes a regex pattern to remove special characters from the text and
    /// stores the list of processed tables for similarity matching.
    ///
    /// # Parameters
    ///
    /// - `sim_table_list` (&Vec<SimTable>): A reference to a vector of `SimTable` instances.
    ///   Each `SimTable` contains the original words and identifiers that will be processed
    ///   for similarity matching.
    ///
    /// # Returns
    ///
    /// - `SimMatcher`: A new instance of `SimMatcher` containing the preprocessed tables
    ///   and the regex pattern for special character removal.
    ///
    /// # Example
    ///
    /// ```
    /// use matcher_rs::{SimMatcher, SimTable};
    ///
    /// let word_list = vec!["example1", "example2"];
    ///
    /// let sim_tables = vec![
    ///     SimTable {
    ///         table_id: 1,
    ///         match_id: "match1",
    ///         word_list: word_list,
    ///     },
    ///     // Add more SimTable instances as desired
    /// ];
    ///
    /// let matcher = SimMatcher::new(&sim_tables);
    /// ```
    pub fn new(sim_table_list: &Vec<SimTable>) -> SimMatcher {
        SimMatcher {
            // Initialize the regex pattern for removing special characters using a predefined regular expression.
            remove_special_pattern: Regex::new(r"\W+").unwrap(),
            // Process the provided sim_table_list to convert each SimTable into a SimProcessedTable.
            sim_processed_table_list: sim_table_list
                .iter()
                .map(|sim_table| SimProcessedTable {
                    // Assign the unique identifier from SimTable to SimProcessedTable.
                    table_id: sim_table.table_id,
                    // Clone the match_id from the SimTable since SimProcessedTable requires an owned string.
                    match_id: sim_table.match_id.to_owned(),
                    // Transform the word_list from SimTable by converting each borrowed string into an owned string.
                    word_list: sim_table
                        .word_list
                        .iter()
                        .map(|&word| word.to_owned())
                        .collect::<Vec<String>>(),
                })
                .collect(),
        }
    }
}

impl<'a> TextMatcherTrait<'a, SimResult<'a>> for SimMatcher {
    /// Checks if a given text matches any word in the preprocessed tables.
    ///
    /// This method takes a reference to a text string and processes it by removing special characters.
    /// It then iterates over all the preprocessed tables and their word lists to check for any match
    /// using the normalized Levenshtein similarity score. If any word in the tables matches the processed
    /// text with a similarity score above the cutoff (specified in the Levenshtein arguments), it returns `true`.
    ///
    /// # Parameters
    ///
    /// - `text` (&str): A reference to the text string to be checked for matches.
    ///
    /// # Returns
    ///
    /// - `bool`: Returns `true` if any word in the preprocessed tables matches the processed text,
    ///   otherwise returns `false`.
    ///
    /// # Example
    ///
    /// ```
    /// use matcher_rs::{SimMatcher, SimTable, TextMatcherTrait};
    ///
    /// let word_list = vec!["example1", "example2"];
    ///
    /// let sim_tables = vec![
    ///     SimTable {
    ///         table_id: 1,
    ///         match_id: "match1",
    ///         word_list: word_list,
    ///     },
    ///     // Add more SimTable instances as desired
    /// ];
    ///
    /// let matcher = SimMatcher::new(&sim_tables);
    ///
    /// assert!(matcher.is_match("example3"));
    /// ```
    fn is_match(&self, text: &str) -> bool {
        // Process the provided text by removing special characters based on the regex pattern.
        let processed_text = self.remove_special_pattern.replace_all(text, "");

        // Iterate over all the preprocessed tables to check if any word matches the processed text.
        self.sim_processed_table_list.iter().any(|sim_table| {
            // For each table, iterate over its word list to find a matching word.
            sim_table.word_list.iter().any(|text| {
                // Calculate the normalized Levenshtein similarity score between the processed text and each word.
                // If the similarity score is above the cutoff (0.8), return true indicating a match was found.
                levenshtein::normalized_similarity_with_args(
                    text.chars(),
                    processed_text.chars(),
                    &levenshtein::Args::default().score_cutoff(0.8),
                )
                .is_some() // Check if a similarity score was computed (indicating a match).
            })
        })
    }

    /// Processes a given text string and returns a list of similarity match results.
    ///
    /// This function takes a reference to a text string, processes it by removing
    /// special characters, and then computes similarity scores for each word in the
    /// preprocessed tables using normalized Levenshtein similarity. It collects and
    /// returns the matching results with their respective similarity scores, table
    /// identifiers, and match identifiers.
    ///
    /// # Parameters
    ///
    /// - `text` (&str): A reference to the text string to be processed and checked
    ///   against the preprocessed tables for similarity matches.
    ///
    /// # Returns
    ///
    /// - `Vec<SimResult>`: A vector of `SimResult` instances containing details of
    ///   the matched words, including their similarity scores, table identifiers,
    ///   and match identifiers.
    ///
    /// # Example
    ///
    /// ```
    /// use matcher_rs::{SimMatcher, SimTable, TextMatcherTrait};
    ///
    /// let word_list = vec!["example1", "example2"];
    ///
    /// let sim_tables = vec![
    ///     SimTable {
    ///         table_id: 1,
    ///         match_id: "match1",
    ///         word_list: word_list,
    ///     },
    ///     // Add more SimTable instances as desired
    /// ];
    ///
    /// let matcher = SimMatcher::new(&sim_tables);
    ///
    /// let results = matcher.process("example3");
    /// for result in results {
    ///     println!("{:?}", result);
    /// }
    /// ```
    fn process(&'a self, text: &str) -> Vec<SimResult<'a>> {
        // Process the provided text by removing special characters based on the regex pattern.
        let processed_text = self.remove_special_pattern.replace_all(text, "");

        // Create a mutable vector to store the resulting similarity match results.
        let mut result_list = Vec::new();

        // Iterate over all the preprocessed tables to compute similarity matches.
        for sim_table in &self.sim_processed_table_list {
            // For each table, iterate over its word list and find matches using Levenshtein similarity.
            result_list.extend(sim_table.word_list.iter().filter_map(|text| {
                // Calculate the normalized Levenshtein similarity score between the processed text and each word.
                levenshtein::normalized_similarity_with_args(
                    text.chars(),
                    processed_text.chars(),
                    &levenshtein::Args::default().score_cutoff(0.8), // Use a similarity cutoff score of 0.8.
                )
                .map(|similarity| SimResult {
                    // If similarity score is found, create a SimResult instance with the matched word details.
                    word: Cow::Borrowed(text),
                    table_id: sim_table.table_id,
                    match_id: &sim_table.match_id,
                    similarity, // Assign the calculated similarity score.
                })
            }));
        }

        // Return the list of collected similarity match results.
        result_list
    }
}
