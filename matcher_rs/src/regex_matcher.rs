use std::borrow::Cow;

use fancy_regex::{escape, Regex};
use id_set::IdSet;
use regex::RegexSet;
use serde::{Deserialize, Serialize};

#[cfg(feature = "serde")]
use crate::util::serde::{serde_regex, serde_regex_list, serde_regex_set};
use crate::{
    matcher::{MatchResultTrait, TextMatcherTrait},
    process::process_matcher::{
        build_process_type_tree, reduce_text_process_with_tree, ProcessType, ProcessTypeBitNode,
    },
};

/// Enum representing different types of regular expression matches, each with a unique strategy.
///
/// This enum is decorated with [Serialize] and [Deserialize] traits for (de)serialization,
/// [Clone] and [Copy] traits to allow copying, [Debug] for formatting, and [PartialEq] for
/// comparison. Uses snake_case for serialized representations.
///
/// Variants:
/// - [RegexMatchType::SimilarChar]: Represents a match type that finds similar characters.
/// - [RegexMatchType::Acrostic]: Matches acrostic patterns.
/// - [RegexMatchType::Regex]: General regular expression matches.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegexMatchType {
    SimilarChar,
    Acrostic,
    Regex,
}

/// A struct representing a table of regular expressions, containing metadata and a list of words.
///
/// Fields:
/// - `table_id`: A unique identifier for the table.
/// - `match_id`: A unique identifier for the match.
/// - `process_type`: The type of process associated with the table, defined by the [ProcessType] enum.
/// - `regex_match_type`: The type of match strategy used, defined by the [RegexMatchType] enum.
/// - `word_list`: A list of words used in the regular expression matching, borrowed for the lifetime `'a`.
#[derive(Debug, Clone)]
pub struct RegexTable<'a> {
    pub table_id: u32,
    pub match_id: u32,
    pub process_type: ProcessType,
    pub regex_match_type: RegexMatchType,
    pub word_list: Vec<&'a str>,
}

/// Enum representing different types of regex patterns used in the regex matcher.
///
/// The enum variants encapsulate different storage and matching strategies:
/// - `Standard`: A single compiled regex pattern.
/// - `List`: A list of compiled regex patterns along with corresponding words.
/// - `Set`: A set of compiled regex patterns optimized for simultaneous matching, along with corresponding words.
///
/// Each variant uses specific serialization and deserialization strategies provided by `serde`.
///
/// Variants:
/// - `Standard { regex }`:
///   - Fields:
///     - `regex: Regex` - A single compiled regex pattern. Uses custom serialization with `serde_regex`.
/// - `List { regex_list, word_list }`:
///   - Fields:
///     - `regex_list: Vec<Regex>` - A list of compiled regex patterns. Uses custom serialization with `serde_regex_list`.
///     - `word_list: Vec<String>` - A list of words corresponding to the regex patterns.
/// - `Set { regex_set, word_list }`:
///   - Fields:
///     - `regex_set: RegexSet` - A set of compiled regex patterns optimized for simultaneous matching. Uses custom serialization with `serde_regex_set`.
///     - `word_list: Vec<String>` - A list of words corresponding to the regex patterns in the set.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
enum RegexType {
    Standard {
        #[cfg_attr(feature = "serde", serde(with = "serde_regex"))]
        regex: Regex,
    },
    List {
        #[cfg_attr(feature = "serde", serde(with = "serde_regex_list"))]
        regex_list: Vec<Regex>,
        word_list: Vec<String>,
    },
    Set {
        #[cfg_attr(feature = "serde", serde(with = "serde_regex_set"))]
        regex_set: RegexSet,
        word_list: Vec<String>,
    },
}

/// A struct representing a table of regex patterns, containing metadata and the type of regex patterns.
///
/// Fields:
/// - `table_id`: A unique identifier for the table.
/// - `match_id`: A unique identifier for the match.
/// - `process_type`: The type of process associated with the table, defined by the [ProcessType] enum.
/// - `regex_type`: The type of regex pattern(s) used, defined by the [RegexType] enum.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct RegexPatternTable {
    table_id: u32,
    match_id: u32,
    process_type: ProcessType,
    regex_type: RegexType,
}

/// A struct representing the result of a regex match operation.
///
/// This struct contains metadata about the match, including the identifiers for the match and table,
/// the word identifier, and the matched word itself.
///
/// Fields:
/// - `match_id`: A unique identifier for the match.
/// - `table_id`: A unique identifier for the table.
/// - `word_id`: A unique identifier for the word in the match.
/// - `word`: The matched word, represented as a [Cow] (clone-on-write) type, borrowed for the lifetime `'a`.
#[derive(Debug, Clone)]
pub struct RegexResult<'a> {
    pub match_id: u32,
    pub table_id: u32,
    pub word_id: u32,
    pub word: Cow<'a, str>,
}

impl MatchResultTrait<'_> for RegexResult<'_> {
    fn match_id(&self) -> u32 {
        self.match_id
    }
    fn table_id(&self) -> u32 {
        self.table_id
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

/// A struct representing a regex matcher.
///
/// This struct is used to match text against a collection of regular expression patterns
/// organized by different processing types. It supports serialization and deserialization
/// with the `serde` crate (when the "serde" feature is enabled).
///
/// Fields:
/// - `process_type_tree`: A vector of `ProcessTypeBitNode`s representing the hierarchical tree structure of process types.
/// - `regex_pattern_table_list`: A vector of `RegexPatternTable` instances containing metadata and regex patterns.
///
/// # Examples
///
/// ```
/// use matcher_rs::{ProcessType, RegexTable, RegexMatchType, RegexMatcher, TextMatcherTrait};
///
/// // Create a sample RegexTable
/// let regex_table = RegexTable {
///     table_id: 1,
///     match_id: 1,
///     process_type: ProcessType::None,
///     regex_match_type: RegexMatchType::Regex,
///     word_list: vec!["^hello", "^world"],
/// };
///
/// // Initialize RegexMatcher with a list of RegexTable
/// let matcher = RegexMatcher::new(&[regex_table]);
///
/// // Sample text to match against
/// let text = "hello world";
///
/// // Check if text matches any regex pattern
/// assert!(matcher.is_match(text));
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RegexMatcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    regex_pattern_table_list: Vec<RegexPatternTable>,
}

impl RegexMatcher {
    /// Constructs a new [RegexMatcher] from a list of [RegexTable].
    ///
    /// This function initializes a [RegexMatcher] by processing the provided `regex_table_list`.
    /// Each [RegexTable] entry is transformed based on its `regex_match_type` to create the
    /// appropriate regex patterns, which are then stored in the matcher.
    ///
    /// # Arguments
    ///
    /// * `regex_table_list` - A slice of [RegexTable] containing the regex patterns and associated metadata.
    ///
    /// # Returns
    ///
    /// * [RegexMatcher] - An instance of [RegexMatcher] initialized with the given `regex_table_list`.
    ///
    /// # Regex Match Types
    ///
    /// The function handles three types of regex match types:
    ///
    /// * [RegexMatchType::SimilarChar]: Generates a single regex pattern that matches similar characters
    ///   in sequence with optional characters in between.
    ///
    /// * [RegexMatchType::Acrostic]: Generates individual regex patterns for each word in the table,
    ///   recognizing them as acrostic patterns. This includes optional separator characters.
    ///
    /// * [RegexMatchType::Regex]: Directly uses the provided words as regex patterns or lists, and tries to
    ///   compile them into a [RegexSet]; if it fails, it falls back to a list.
    ///
    /// For each [RegexTable] entry, the function creates a corresponding `RegexPatternTable` with appropriate
    /// regex patterns or lists, then constructs the final [RegexMatcher] with a process type tree.
    pub fn new(regex_table_list: &[RegexTable]) -> RegexMatcher {
        let mut process_type_set = IdSet::with_capacity(regex_table_list.len());
        let mut regex_pattern_table_list = Vec::with_capacity(regex_table_list.len());

        for regex_table in regex_table_list {
            process_type_set.insert(regex_table.process_type.bits() as usize);

            let size = regex_table.word_list.len();

            match regex_table.regex_match_type {
                RegexMatchType::SimilarChar => {
                    let pattern = regex_table
                        .word_list
                        .iter()
                        .map(|charstr| format!("({})", escape(charstr).replace(',', "|")))
                        .collect::<Vec<String>>()
                        .join(".?");

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        process_type: regex_table.process_type,
                        regex_type: RegexType::Standard {
                            regex: Regex::new(&pattern).unwrap(),
                        },
                    });
                }
                RegexMatchType::Acrostic => {
                    let mut word_list = Vec::with_capacity(size);
                    let mut regex_list = Vec::with_capacity(size);
                    let mut pattern_list = Vec::with_capacity(size);

                    for &word in regex_table.word_list.iter() {
                        let pattern = format!(
                            r"(?i)(?:^|[\s\pP]+?){}",
                            escape(word).replace(',', r".*?[\s\pP]+?")
                        );
                        match Regex::new(&pattern) {
                            Ok(regex) => {
                                regex_list.push(regex);
                                word_list.push(word.to_owned());
                                pattern_list.push(pattern);
                            }
                            Err(e) => {
                                println!("Acrostic word {word} is illegal, ignored. Error: {e}");
                            }
                        }
                    }

                    let regex_type = RegexSet::new(pattern_list).map_or(
                        RegexType::List {
                            regex_list,
                            word_list: word_list.clone(),
                        },
                        |regex_set| RegexType::Set {
                            regex_set,
                            word_list,
                        },
                    );

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        process_type: regex_table.process_type,
                        regex_type,
                    });
                }
                RegexMatchType::Regex => {
                    let mut word_list = Vec::with_capacity(size);
                    let mut regex_list = Vec::with_capacity(size);

                    for &word in regex_table.word_list.iter() {
                        match Regex::new(word) {
                            Ok(regex) => {
                                regex_list.push(regex);
                                word_list.push(word.to_owned());
                            }
                            Err(e) => {
                                println!("Regex word {word} is illegal, ignored. Error: {e}");
                            }
                        }
                    }

                    let regex_type = RegexSet::new(&word_list).map_or(
                        RegexType::List {
                            regex_list,
                            word_list: word_list.clone(),
                        },
                        |regex_set| RegexType::Set {
                            regex_set,
                            word_list,
                        },
                    );

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        process_type: regex_table.process_type,
                        regex_type,
                    });
                }
            };
        }

        let process_type_tree = build_process_type_tree(&process_type_set);

        RegexMatcher {
            process_type_tree,
            regex_pattern_table_list,
        }
    }
}

impl<'a> TextMatcherTrait<'a, RegexResult<'a>> for RegexMatcher {
    /// Checks if the given text matches any of the regex patterns in the [RegexMatcher].
    ///
    /// This function first processes the input text using the `process_type_tree` of the [RegexMatcher],
    /// which prepares the text for matching by applying various transformation rules.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice that holds the text to be checked against the regex patterns.
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if there is a match, otherwise returns `false`.
    fn is_match(&'a self, text: &'a str) -> bool {
        if text.is_empty() {
            return false;
        }

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._is_match_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    /// Checks if any of the given processed texts match any of the regex patterns in the [RegexMatcher].
    ///
    /// This function iterates over the pairs of processed text and their associated processing type sets.
    /// It checks if any of the regex patterns in the `regex_pattern_table_list` match the processed text.
    ///
    /// The function first verifies that the `process_type` of a regex pattern is present in the current
    /// `process_type_set`. If it is, it evaluates the match for different types of regex patterns:
    /// - `Standard`: Uses a standard regex match.
    /// - `List`: Checks if any regex in the list matches.
    /// - `Set`: Checks if the regex set matches.
    ///
    /// If any of the regex patterns match the processed text, the function returns `true`.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_set` - A slice of tuples where the first element is the processed text
    ///     and the second element is the set of process types associated with that text.
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if at least one regex pattern matches any processed text, otherwise returns `false`.
    fn _is_match_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> bool {
        for (processed_text, process_type_set) in processed_text_process_type_set {
            for regex_pattern_table in &self.regex_pattern_table_list {
                if !process_type_set.contains(regex_pattern_table.process_type.bits() as usize) {
                    continue;
                }

                let is_match = match &regex_pattern_table.regex_type {
                    RegexType::Standard { regex } => regex.is_match(processed_text).unwrap(),
                    RegexType::List { regex_list, .. } => regex_list
                        .iter()
                        .any(|regex| regex.is_match(processed_text).unwrap()),
                    RegexType::Set { regex_set, .. } => regex_set.is_match(processed_text),
                };

                if is_match {
                    return true;
                }
            }
        }
        false
    }

    /// Processes the given text and returns a list of [RegexResult] containing matches from the [RegexMatcher].
    ///
    /// This function processes the input text using the `process_type_tree` of the [RegexMatcher],
    /// preparing the text for matching by applying various transformation rules. It then uses the
    /// `_process_with_processed_text_process_type_set` function to find and return regex matches
    /// based on the processed text and process type set.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice that holds the text to be processed and matched against the regex patterns.
    ///
    /// # Returns
    ///
    /// * [`Vec<RegexResult>`] - A vector of [RegexResult] instances, each representing a match found in the text.
    fn process(&'a self, text: &'a str) -> Vec<RegexResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._process_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    /// Processes the `processed_text_process_type_set` to find and return regex matches.
    ///
    /// This function iterates over the pairs of processed text and their associated processing type sets.
    /// It then checks against the regex patterns in the `regex_pattern_table_list` to find matches.
    ///
    /// For each regex pattern, the function first verifies that the `process_type` of a regex pattern is present
    /// in the current `process_type_set`. If it is, it processes matches based on different types of regex patterns:
    /// - `Standard`: Uses a standard regex match and stores the captures.
    /// - `List`: Checks each regex in the list for a match and stores the corresponding words.
    /// - `Set`: Checks the regex set for matches and stores the corresponding words.
    ///
    /// The function keeps track of matches using `table_id_index_set` to avoid duplicate entries.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_set` - A slice of tuples where the first element is the processed text
    ///   and the second element is the set of process types associated with that text.
    ///
    /// # Returns
    ///
    /// * [`Vec<RegexResult>`] - A vector of [RegexResult] instances, each representing a match found in the processed text.
    fn _process_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> Vec<RegexResult<'a>> {
        let mut result_list = Vec::new();
        let mut table_id_index_set = IdSet::new();

        for (processed_text, process_type_set) in processed_text_process_type_set {
            for regex_pattern_table in &self.regex_pattern_table_list {
                if !process_type_set.contains(regex_pattern_table.process_type.bits() as usize) {
                    continue;
                }
                match &regex_pattern_table.regex_type {
                    RegexType::Standard { regex } => {
                        if table_id_index_set.insert(regex_pattern_table.table_id as usize) {
                            for caps in regex.captures_iter(processed_text).flatten() {
                                result_list.push(RegexResult {
                                    match_id: regex_pattern_table.match_id,
                                    table_id: regex_pattern_table.table_id,
                                    word_id: 0,
                                    word: Cow::Owned(
                                        caps.iter()
                                            .skip(1)
                                            .filter_map(|m| m.map(|match_char| match_char.as_str()))
                                            .collect::<String>(),
                                    ),
                                });
                            }
                        }
                    }
                    RegexType::List {
                        regex_list,
                        word_list,
                    } => {
                        for (index, regex) in regex_list.iter().enumerate() {
                            let table_id_index =
                                ((regex_pattern_table.table_id as usize) << 32) | index;

                            if table_id_index_set.insert(table_id_index) {
                                if let Ok(is_match) = regex.is_match(processed_text) {
                                    if is_match {
                                        result_list.push(RegexResult {
                                            match_id: regex_pattern_table.match_id,
                                            table_id: regex_pattern_table.table_id,
                                            word_id: index as u32,
                                            word: Cow::Borrowed(&word_list[index]),
                                        });
                                    }
                                }
                            }
                        }
                    }
                    RegexType::Set {
                        regex_set,
                        word_list,
                    } => {
                        for index in regex_set.matches(processed_text) {
                            let table_id_index =
                                ((regex_pattern_table.table_id as usize) << 32) | index;

                            if table_id_index_set.insert(table_id_index) {
                                result_list.push(RegexResult {
                                    match_id: regex_pattern_table.match_id,
                                    table_id: regex_pattern_table.table_id,
                                    word_id: index as u32,
                                    word: Cow::Borrowed(&word_list[index]),
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
