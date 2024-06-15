use std::borrow::Cow;

use fancy_regex::Regex;
use rapidfuzz::distance::levenshtein;

use super::{MatchResultTrait, TextMatcherTrait};

#[derive(Debug, Clone)]
/// A struct representing a table for similarity matching.
///
/// The `SimTable` struct defines a similarity table with a unique identifier, match
/// identifier, and a list of words. This struct is primarily used to store the
/// original words and identifiers that will be processed for similarity matching
/// purposes.
///
/// # Fields
///
/// - `table_id` (u64): The unique identifier for the similarity table.
/// - `match_id` (u64): An ID that serves as an identifier for the match.
/// - `word_list` (Vec<&'a str>): A vector of string slices representing the words
///   to be included in the similarity table.
///
/// The lifetimes `'a` ensure that the references in the `SimTable` struct are valid
/// for as long as the struct instance exists.
///
/// # Example
///
/// ```
/// use matcher_rs::SimTable;
///
/// let word_list = vec!["example1", "example2"];
///
/// let sim_table = SimTable {
///     table_id: 1,
///     match_id: 1,
///     word_list: word_list,
/// };
/// ```
pub struct SimTable<'a> {
    pub table_id: u64,
    pub match_id: u64,
    pub word_list: Vec<&'a str>,
}

#[derive(Debug, Clone)]
/// A struct representing a preprocessed table for similarity matching.
///
/// The `SimProcessedTable` struct is used internally within the `SimMatcher` to store
/// preprocessed versions of the tables originally defined by the user through the `SimTable` struct.
///
/// # Fields
///
/// - `table_id` (u64): The unique identifier for the similarity table.
/// - `match_id` (u64): An ID that serves as an identifier for the match.
/// - `word_list` (`Vec<String>`): A vector of owned strings representing the words
///   that have been preprocessed for similarity matching.
struct SimProcessedTable {
    table_id: u64,
    match_id: u64,
    word_list: Vec<String>,
}

#[derive(Debug, Clone)]
/// A struct representing the result of a similarity match.
///
/// The `SimResult` struct captures the details of a word that was found to be similar
/// during the similarity matching process. It includes the matched word, the unique
/// identifier of the table where the word was found, the match identifier of that table,
/// and the similarity score computed for the match.
///
/// The lifetimes ensure that the references in the `SimResult` struct remain valid
/// for as long as the struct instance exists.
///
/// # Fields
///
/// - `word` (Cow<'a, str>): The word that was found to be similar. It is stored as a `Cow`
///   (clone-on-write) to allow for both owned and borrowed strings.
/// - `table_id` (u64): The unique identifier of the table where the word was found.
/// - `match_id` (u64): An ID that serves as an identifier for the match.
/// - `similarity` (f64): The similarity score computed for the match. This score typically
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
/// The `SimMatcher` struct is responsible for managing and processing similarity matching
/// operations on provided textual data using predefined tables. It includes functionality
/// to preprocess text by removing special characters and to search for matches within
/// the preprocessed tables using normalized Levenshtein similarity.
///
/// # Fields
///
/// - `remove_special_pattern` (Regex): A compiled regular expression used for removing
///   special characters from the text before processing.
/// - `sim_processed_table_list` (`Vec<SimProcessedTable>`): A vector containing preprocessed
///   tables, where each table consists of a list of words and identifiers ready for
///   similarity matching.
///
/// # Example
///
/// ```
/// use fancy_regex::Regex;
/// use matcher_rs::{SimMatcher, SimTable};
///
/// let word_list = vec!["example1", "example2"];
///
/// let sim_tables = vec![
///     SimTable {
///         table_id: 1,
///         match_id: 1,
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
    /// Creates a new instance of `SimMatcher` by preprocessing the provided list of `SimTable` instances.
    ///
    /// This function takes a reference to a list of `SimTable` instances provided by the user and
    /// preprocesses each table to create corresponding `SimProcessedTable` instances. The preprocessing
    /// involves compiling a regular expression for removing special characters and converting the
    /// words and match identifiers to owned `String` types.
    ///
    /// # Parameters
    ///
    /// - `sim_table_list` (&[SimTable]): A reference to a slice of `SimTable` instances to be preprocessed.
    ///
    /// # Returns
    ///
    /// - `SimMatcher`: A new instance of `SimMatcher` with preprocessed tables ready for similarity matching.
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
    ///         match_id: 1,
    ///         word_list: word_list,
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
    /// Checks if the given text has a similarity match in any of the preprocessed tables.
    ///
    /// This function takes a reference to a text string, processes it by removing
    /// special characters, and then checks for similarity matches within the preprocessed
    /// tables using normalized Levenshtein similarity. It returns `true` if any similarity
    /// match with a score above the specified cutoff (0.8) is found, and `false` otherwise.
    ///
    /// # Parameters
    ///
    /// - `text` (&str): A reference to the text string to be processed and checked
    ///   against the preprocessed tables for similarity matches.
    ///
    /// # Returns
    ///
    /// - `bool`: A boolean value indicating whether a similarity match was found (`true`)
    ///   or not (`false`).
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
    ///         match_id: 1,
    ///         word_list: word_list,
    ///     },
    ///     // Add more SimTable instances as desired
    /// ];
    ///
    /// let matcher = SimMatcher::new(&sim_tables);
    ///
    /// let is_match_found = matcher.is_match("example3");
    /// println!("Is a similarity match found? {}", is_match_found);
    /// ```
    fn is_match(&self, text: &str) -> bool {
        let processed_text = self.remove_special_pattern.replace_all(text, "");

        self.sim_processed_table_list.iter().any(|sim_table| {
            sim_table.word_list.iter().any(|text| {
                levenshtein::normalized_similarity_with_args(
                    text.chars(),
                    processed_text.chars(),
                    &levenshtein::Args::default().score_cutoff(0.8),
                )
                .is_some()
            })
        })
    }

    /// Processes the given text and finds all similarity matches in the preprocessed tables.
    ///
    /// This function takes a reference to a text string, processes it by removing
    /// special characters, and then searches for similarity matches within the preprocessed
    /// tables using normalized Levenshtein similarity. It returns a vector of `SimResult`
    /// instances, capturing details of each word found to be similar along with its similarity
    /// score and associated identifiers.
    ///
    /// # Parameters
    ///
    /// - `text` (&str): A reference to the text string to be processed and checked
    ///   against the preprocessed tables for similarity matches.
    ///
    /// # Returns
    ///
    /// - `Vec<SimResult>`: A vector of `SimResult` instances, each representing a
    ///   word that was found to be similar, along with its similarity score and associated identifiers.
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
    ///         match_id: 1,
    ///         word_list: word_list,
    ///     },
    ///     // Add more SimTable instances as desired
    /// ];
    ///
    /// let matcher = SimMatcher::new(&sim_tables);
    ///
    /// let results = matcher.process("example3");
    ///
    /// for result in results {
    ///     println!(
    ///         "Matched word: {}, Table ID: {}, Match ID: {}, Similarity: {}",
    ///         result.word, result.table_id, result.match_id, result.similarity
    ///     );
    /// }
    /// ```
    fn process(&'a self, text: &str) -> Vec<SimResult<'a>> {
        let processed_text = self.remove_special_pattern.replace_all(text, "");

        let mut result_list = Vec::new();

        for sim_table in &self.sim_processed_table_list {
            result_list.extend(sim_table.word_list.iter().filter_map(|text| {
                levenshtein::normalized_similarity_with_args(
                    text.chars(),
                    processed_text.chars(),
                    &levenshtein::Args::default().score_cutoff(0.8),
                )
                .map(|similarity| SimResult {
                    word: Cow::Borrowed(text),
                    table_id: sim_table.table_id,
                    match_id: sim_table.match_id,
                    similarity,
                })
            }));
        }

        result_list
    }
}
