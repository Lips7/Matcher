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
/// Under the hood, this struct pre-compiles and aggregates regex rules into an optimized tree structure.
/// Depending on the `RegexMatchType`, it compiles all compatible expressions into a highly optimized
/// `regex::RegexSet` DFA engine for parallel O(n) text matching passes over reduced text states.
///
/// It holds a reference to a `ProcessTypeBitNode` tree, applying transformation passes (e.g., lowercasing,
/// full-width to half-width character mappings) to the text *before* attempting the regex comparisons.
///
/// # Fields
/// * `process_type_tree` - The compiled workflow tree ensuring text transforms happen exactly once per distinct branch sequence.
/// * `regex_set` - An optimized `RegexSet` instance managing parallel DFA matching passes.
/// * `regex_dedup_conf_list` - Deduplicated configuration references grouped by automaton match indexes.
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
    /// This function initializes a [`RegexMatcher`] by processing the provided `regex_table_list`.
    /// Each [`RegexTable`] entry is transformed based on its `regex_match_type` to create the
    /// appropriate regex patterns, which are then deduplicated and stored in the matcher's `RegexSet`.
    /// It performs early validation to skip excessively long patterns that could pose a ReDoS
    /// (Regular Expression Denial of Service) risk.
    ///
    /// # Arguments
    /// * `regex_table_list` - A slice of [`RegexTable`] containing the regex patterns and associated metadata.
    ///
    /// # Returns
    /// An instance of [`RegexMatcher`] initialized with the compiled regex patterns.
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
    fn is_match(&'a self, text: &'a str) -> bool {
        if text.is_empty() {
            return false;
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.is_match_preprocessed(&processed_text_process_type_masks)
    }

    /// Triggers the full execution passes across all patterns recursively.
    fn process(&'a self, text: &'a str) -> Vec<RegexResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.process_preprocessed(&processed_text_process_type_masks)
    }

    /// Checks if any of the given processed texts match any internal regex pattern.
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

    /// Eagerly evaluates the reduced text variations against all compiled regular expressions,
    /// returning a complete vector of matches.
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
