use std::{borrow::Cow, collections::HashSet};

use regex::{Regex, RegexSet, escape};

use serde::{Deserialize, Serialize};

use crate::{
    matcher::{MatchResultTrait, TextMatcherTrait},
    process::process_matcher::{
        ProcessType, ProcessTypeBitNode, ProcessedTextMasks, build_process_type_tree,
        reduce_text_process_with_tree,
    },
};

/// Enum representing different types of regular expression matches, each with a unique strategy.
///
/// This enum is decorated with [`Serialize`] and [`Deserialize`] traits for (de)serialization,
/// [`Clone`] and [`Copy`] traits to allow copying, [`Debug`] for formatting, and [`PartialEq`] for
/// comparison. Uses snake_case for serialized representations.
///
/// # Variants
/// * `SimilarChar` - Represents a match type that finds similar characters.
/// * `Acrostic` - Matches acrostic patterns.
/// * `Regex` - General regular expression matches.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegexMatchType {
    SimilarChar,
    Acrostic,
    Regex,
}

/// A struct representing a table of regular expressions, containing metadata and a list of words.
///
/// # Type Parameters
/// * `'a` - The lifetime of the borrowed strings in the word list.
///
/// # Fields
/// * `table_id` - A unique identifier for the specific matching table.
/// * `match_id` - A unique identifier for the match operation.
/// * `process_type` - The text processing rules to be applied, represented by the [`ProcessType`] bitflags enum.
/// * `regex_match_type` - The type of match strategy used, defined by the [`RegexMatchType`] enum.
/// * `word_list` - A list of words to be used in the matching process.
#[derive(Debug, Clone)]
pub struct RegexTable<'a> {
    pub table_id: u32,
    pub match_id: u32,
    pub process_type: ProcessType,
    pub regex_match_type: RegexMatchType,
    pub word_list: Vec<&'a str>,
}

/// A struct to hold configuration metadata for matched regex patterns.
///
/// This struct maintains the link between a compiled regular expression and its original
/// metadata, allowing for the identification of the source table and word upon a match.
///
/// # Fields
/// * `table_id` - A unique identifier for the specific matching table.
/// * `match_id` - A unique identifier for the match operation.
/// * `process_type` - The text processing rules to be applied, represented by the [`ProcessType`] bitflags enum.
/// * `word_id` - A unique identifier for the word within the table.
/// * `word` - The original word or regex pattern string.
#[derive(Debug, Clone)]
struct RegexConf {
    table_id: u32,
    match_id: u32,
    process_type: ProcessType,
    word_id: u32,
    word: String,
}

/// A struct representing the result of a regex match operation.
///
/// This struct contains metadata about the match, including the identifiers for the match and table,
/// the word identifier, and the matched word itself.
///
/// # Type Parameters
/// * `'a` - The lifetime of the matched word content.
///
/// # Fields
/// * `match_id` - A unique identifier for the match operation.
/// * `table_id` - A unique identifier for the specific matching table.
/// * `word_id` - A unique identifier for the word within the table.
/// * `word` - The matched word, represented as a [`Cow`] (clone-on-write) type.
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
    fn similarity(&self) -> Option<f64> {
        None
    }
}

/// A structural text matcher for regular expressions.
///
/// Under the hood, this struct pre-compiles and aggregates regex rules into an optimized
/// `regex::RegexSet` DFA engine for parallel O(n) text matching passes over reduced text states.
///
/// It supports three strategies:
/// - **`Regex`**: Standard regular expression matching.
/// - **`SimilarChar`**: Matches characters with optional intermediate "noise" characters.
/// - **`Acrostic`**: Matches patterns where each character is separated by non-word characters.
///
/// # Detailed Explanation / Algorithm
/// 1. **Initialization**:
///    - For each `RegexTable`, it transforms the word list into regex patterns based on the `RegexMatchType`.
///    - `SimilarChar` patterns are escaped and joined with `.?`.
///    - `Acrostic` patterns are escaped and joined with `.*?[\s\pP]+?`.
///    - All patterns are compiled into a single `RegexSet` for simultaneous matching.
/// 2. **Matching**:
///    - Scans all pre-processed text variants using the `RegexSet`.
///    - For each variant hit, it validates if the hit's `ProcessType` is allowed by the variant's bitmask.
///    - Results are deduplicated by `(table_id, word_id)` to ensure each rule only triggers once per match.
///
/// # Fields
/// * `process_type_tree` - Workflow tree for efficient text transforms.
/// * `regex_set` - Optimized `RegexSet` managing parallel DFA matching passes.
/// * `regex_dedup_conf_list` - Metadata mapping automaton hits back to original rules.
///
/// # Examples
/// ```rust
/// use matcher_rs::{ProcessType, RegexTable, RegexMatchType, RegexMatcher, TextMatcherTrait};
///
/// let regex_table = RegexTable {
///     table_id: 1,
///     match_id: 1,
///     process_type: ProcessType::None,
///     regex_match_type: RegexMatchType::Regex,
///     word_list: vec!["^hello", "^world"],
/// };
///
/// let matcher = RegexMatcher::new(&[regex_table]);
///
/// assert!(matcher.is_match("hello world"));
/// ```
#[derive(Debug, Clone)]
pub struct RegexMatcher {
    process_type_tree: Box<[ProcessTypeBitNode]>,
    regex_set: RegexSet,
    regex_dedup_conf_list: Box<[RegexConf]>,
}

impl RegexMatcher {
    /// Constructs a new [`RegexMatcher`] from a list of [`RegexTable`].
    ///
    /// # Detailed Explanation / Algorithm
    /// This method performs several transformation steps:
    /// 1. Iterates over each table and its word list.
    /// 2. Applies strategy-specific transformations (e.g., escaping and joining characters).
    /// 3. Validates pattern length to prevent ReDoS (skipping patterns > 1024 characters).
    /// 4. Compiles the final `RegexSet`.
    ///
    /// # Arguments
    /// * `regex_table_list` - Configuration data for all regex rules.
    ///
    /// # Returns
    /// A fully initialized and compiled [`RegexMatcher`].
    pub fn new(regex_table_list: &[RegexTable]) -> RegexMatcher {
        let mut process_type_set = HashSet::with_capacity(regex_table_list.len());

        let mut regex_pattern_list = Vec::new();
        let mut regex_conf_list: Vec<RegexConf> = Vec::new();

        for regex_table in regex_table_list {
            process_type_set.insert(regex_table.process_type.bits());

            match regex_table.regex_match_type {
                RegexMatchType::SimilarChar => {
                    let pattern = regex_table
                        .word_list
                        .iter()
                        .map(|charstr| format!("(?:{})", escape(charstr).replace(',', "|")))
                        .collect::<Vec<String>>()
                        .join(".?");

                    if pattern.len() > 1024 {
                        eprintln!(
                            "SimilarChar pattern is too long ({}), potential ReDoS risk. Skipping.",
                            pattern.len()
                        );
                        continue;
                    }

                    if Regex::new(&pattern).is_ok() {
                        regex_pattern_list.push(pattern);
                        regex_conf_list.push(RegexConf {
                            table_id: regex_table.table_id,
                            match_id: regex_table.match_id,
                            process_type: regex_table.process_type,
                            word_id: 0,
                            word: regex_table.word_list.join(""),
                        });
                    }
                }
                RegexMatchType::Acrostic => {
                    for (index, &word) in regex_table.word_list.iter().enumerate() {
                        let pattern = format!(
                            r"(?i)(?:^|[\s\pP]+?){}",
                            escape(word).replace(',', r".*?[\s\pP]+?")
                        );
                        if pattern.len() > 1024 {
                            eprintln!("Acrostic pattern too long for word {}, skipping.", word);
                            continue;
                        }
                        if Regex::new(&pattern).is_ok() {
                            regex_pattern_list.push(pattern);
                            regex_conf_list.push(RegexConf {
                                table_id: regex_table.table_id,
                                match_id: regex_table.match_id,
                                process_type: regex_table.process_type,
                                word_id: index as u32,
                                word: word.to_owned(),
                            });
                        } else {
                            eprintln!("Acrostic word {word} is illegal, ignored.");
                        }
                    }
                }
                RegexMatchType::Regex => {
                    for (index, &word) in regex_table.word_list.iter().enumerate() {
                        if word.len() > 1024 {
                            eprintln!("Regex pattern too long, skipping: {:.20}...", word);
                            continue;
                        }
                        if Regex::new(word).is_ok() {
                            regex_pattern_list.push(word.to_string());
                            regex_conf_list.push(RegexConf {
                                table_id: regex_table.table_id,
                                match_id: regex_table.match_id,
                                process_type: regex_table.process_type,
                                word_id: index as u32,
                                word: word.to_owned(),
                            });
                        } else {
                            eprintln!("Regex word {word} is illegal, ignored.");
                        }
                    }
                }
            };
        }

        let process_type_tree = build_process_type_tree(&process_type_set).into_boxed_slice();

        let regex_set = match RegexSet::new(&regex_pattern_list) {
            Ok(regex_set) => regex_set,
            Err(e) => {
                eprintln!("Failed to compile regex set: {}", e);
                RegexSet::empty()
            }
        };

        RegexMatcher {
            process_type_tree,
            regex_set,
            regex_dedup_conf_list: regex_conf_list.into_boxed_slice(),
        }
    }
}

impl<'a> TextMatcherTrait<'a, RegexResult<'a>> for RegexMatcher {
    /// Checks if the given text matches any of the compiled regex patterns.
    ///
    /// This is a convenience method that delegates text pre-processing and calls
    /// `is_match_preprocessed`.
    ///
    /// # Arguments
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    /// * `true` if the text matches any of the compiled regex patterns, `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{ProcessType, RegexTable, RegexMatchType, RegexMatcher, TextMatcherTrait};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     process_type: ProcessType::None,
    ///     regex_match_type: RegexMatchType::Regex,
    ///     word_list: vec!["^hello"],
    /// };
    /// let matcher = RegexMatcher::new(&[regex_table]);
    /// assert!(matcher.is_match("hello world"));
    /// assert!(!matcher.is_match("world"));
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
    /// A [`Vec<RegexResult>`] containing the matching results.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{ProcessType, RegexTable, RegexMatchType, RegexMatcher, TextMatcherTrait};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     process_type: ProcessType::None,
    ///     regex_match_type: RegexMatchType::Regex,
    ///     word_list: vec!["hello", "world"],
    /// };
    /// let matcher = RegexMatcher::new(&[regex_table]);
    /// let results = matcher.process("hello world");
    /// assert_eq!(results.len(), 2);
    /// ```
    fn process(&'a self, text: &'a str) -> Vec<RegexResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.process_preprocessed(&processed_text_process_type_masks)
    }

    /// Checks if any pre-processed text variant matches a regex pattern.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Iterates through the pre-processed text variations.
    /// 2. Uses `regex_set.matches()` to get all hits for a text variant.
    /// 3. Validates each hit's `ProcessType` against the current variant's bitmask.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - Pre-processed variants and bitmasks.
    ///
    /// # Returns
    /// `true` if a valid match is found.
    fn is_match_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> bool {
        for (processed_text, process_type_mask) in processed_text_process_type_masks {
            let matches = self.regex_set.matches(processed_text);
            if !matches.matched_any() {
                continue;
            }

            for index in matches {
                let conf = &self.regex_dedup_conf_list[index];
                if (process_type_mask & (1u64 << conf.process_type.bits())) != 0 {
                    return true;
                }
            }
        }
        false
    }

    /// Evaluates pre-processed text and returns all unique regex match results.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Iterates through all text variants and checks for `RegexSet` hits.
    /// 2. For each hit, checks if the rule's `ProcessType` is allowed.
    /// 3. Deduplicates results by `(table_id, word_id)` to prevent multiple hits for the same rule.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - Pre-processed variants and bitmasks.
    ///
    /// # Returns
    /// A vector of [`RegexResult`] matches.
    fn process_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<RegexResult<'a>> {
        let mut result_list = Vec::new();
        let mut table_id_index_set = HashSet::new();

        for (processed_text, process_type_mask) in processed_text_process_type_masks {
            let matches = self.regex_set.matches(processed_text);
            if !matches.matched_any() {
                continue;
            }

            for index in matches {
                let conf = &self.regex_dedup_conf_list[index];
                if (process_type_mask & (1u64 << conf.process_type.bits())) == 0 {
                    continue;
                }

                // A match is deduped based on its table ID and word ID.
                let table_id_index = ((conf.table_id as usize) << 32) | (conf.word_id as usize);

                if table_id_index_set.insert(table_id_index) {
                    result_list.push(RegexResult {
                        match_id: conf.match_id,
                        table_id: conf.table_id,
                        word_id: conf.word_id,
                        word: Cow::Owned(conf.word.clone()),
                    });
                }
            }
        }

        result_list
    }
}
