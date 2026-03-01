use std::{borrow::Cow, collections::HashSet};

use fancy_regex::{Regex, escape};
use regex::RegexSet;
use serde::{Deserialize, Serialize};

use crate::{
    matcher::{MatchResultTrait, TextMatcherInternal, TextMatcherTrait},
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

/// Enum representing different types of regex patterns used in the regex matcher.
///
/// The enum variants encapsulate different storage and matching strategies:
/// - `Standard`: A single compiled regex pattern.
/// - `List`: A list of compiled regex patterns along with corresponding words.
/// - `Set`: A set of compiled regex patterns optimized for simultaneous matching, along with corresponding words.
#[derive(Debug, Clone)]
enum RegexType {
    Standard {
        regex: Regex,
    },
    List {
        regex_list: Vec<Regex>,
        word_list: Vec<String>,
    },
    Set {
        regex_set: RegexSet,
        word_list: Vec<String>,
    },
}

/// A struct representing a table of regex patterns, containing metadata and the type of regex patterns.
///
/// # Fields
/// * `table_id` - A unique identifier for the specific matching table.
/// * `match_id` - A unique identifier for the match operation.
/// * `process_type` - The text processing rules to be applied, represented by the [`ProcessType`] bitflags enum.
/// * `regex_type` - The type of regex pattern(s) used, defined by the [`RegexType`] enum.
#[derive(Debug, Clone)]
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
/// Depending on the `RegexMatchType`, it either checks single standard regexes, iterates over a list of
/// distinct patterns, or leverages a highly optimized `RegexSet` for simultaneous pattern matching.
///
/// It holds a reference to a `ProcessTypeBitNode` tree, applying transformation passes (e.g., lowercasing,
/// full-width to half-width character mappings) to the text *before* attempting the regex comparisons.
///
/// # Algorithm
/// 1. Iterates over `regex_table_list`.
/// 2. For each `RegexTable`, handles 3 match strategies:
///    * [`RegexMatchType::SimilarChar`]: Concatenates characters with `.?.` matching variants. If length > 1024, skips.
///    * [`RegexMatchType::Acrostic`]: Transforms each word into a pattern matching word boundaries / whitespaces `(?i)(?:^|[\s\pP]+?)`.
///      Attempts to create a `RegexSet` from all valid patterns. If `RegexSet` creation fails (e.g., due to varying named captures), it falls back to a list of sequential `Regex` objects (`RegexType::List`).
///    * [`RegexMatchType::Regex`]: Validates standard regexes (skips > 1024 char limits or invalid syntax). Compiles into a `RegexSet` or `RegexType::List` fallback.
/// 3. Builds a `ProcessTypeBitNode` tree based on the union of all active `ProcessType`s.
///
/// # Fields
/// * `process_type_tree` - The compiled workflow tree ensuring text transforms happen exactly once per distinct branch sequence.
/// * `regex_pattern_table_list` - A list of specifically mapped regex table evaluation rules and engines.
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
    regex_pattern_table_list: Box<[RegexPatternTable]>,
}

impl RegexMatcher {
    /// Constructs a new [`RegexMatcher`] from a list of [`RegexTable`].
    ///
    /// This function initializes a [`RegexMatcher`] by processing the provided `regex_table_list`.
    /// Each [`RegexTable`] entry is transformed based on its `regex_match_type` to create the
    /// appropriate regex patterns, which are then stored in the matcher. It performs early
    /// validation to skip excessively long patterns that could pose a ReDoS (Regular Expression Denial of Service) risk.
    ///
    /// # Arguments
    /// * `regex_table_list` - A slice of [`RegexTable`] containing the regex patterns and associated metadata.
    ///
    /// # Returns
    /// An instance of [`RegexMatcher`] initialized with the compiled regex patterns.
    pub fn new(regex_table_list: &[RegexTable]) -> RegexMatcher {
        let mut process_type_set = HashSet::with_capacity(regex_table_list.len());
        let mut regex_pattern_table_list = Vec::with_capacity(regex_table_list.len());

        for regex_table in regex_table_list {
            process_type_set.insert(regex_table.process_type.bits());

            let size = regex_table.word_list.len();

            match regex_table.regex_match_type {
                RegexMatchType::SimilarChar => {
                    let pattern = regex_table
                        .word_list
                        .iter()
                        .map(|charstr| format!("({})", escape(charstr).replace(',', "|")))
                        .collect::<Vec<String>>()
                        .join(".?");

                    if pattern.len() > 1024 {
                        eprintln!(
                            "SimilarChar pattern is too long ({}), potential ReDoS risk. Skipping.",
                            pattern.len()
                        );
                        continue;
                    }

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

                    for &word in &regex_table.word_list {
                        let pattern = format!(
                            r"(?i)(?:^|[\s\pP]+?){}",
                            escape(word).replace(',', r".*?[\s\pP]+?")
                        );
                        if pattern.len() > 1024 {
                            eprintln!("Acrostic pattern too long for word {}, skipping.", word);
                            continue;
                        }
                        match Regex::new(&pattern) {
                            Ok(regex) => {
                                regex_list.push(regex);
                                word_list.push(word.to_owned());
                                pattern_list.push(pattern);
                            }
                            Err(e) => {
                                eprintln!("Acrostic word {word} is illegal, ignored. Error: {e}");
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

                    for &word in &regex_table.word_list {
                        if word.len() > 1024 {
                            eprintln!("Regex pattern too long, skipping: {:.20}...", word);
                            continue;
                        }
                        match Regex::new(word) {
                            Ok(regex) => {
                                regex_list.push(regex);
                                word_list.push(word.to_owned());
                            }
                            Err(e) => {
                                eprintln!("Regex word {word} is illegal, ignored. Error: {e}");
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

        let process_type_tree = build_process_type_tree(&process_type_set).into_boxed_slice();

        RegexMatcher {
            process_type_tree,
            regex_pattern_table_list: regex_pattern_table_list.into_boxed_slice(),
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
    /// `true` if there is a match against any active regex table; otherwise `false`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{RegexTable, RegexMatchType, ProcessType, RegexMatcher, TextMatcherTrait};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     process_type: ProcessType::None,
    ///     regex_match_type: RegexMatchType::Regex,
    ///     word_list: vec!["^hello", "world$"],
    /// };
    ///
    /// let matcher = RegexMatcher::new(&[regex_table]);
    ///
    /// assert!(matcher.is_match("hello there"));
    /// assert!(matcher.is_match("beautiful world"));
    /// assert!(!matcher.is_match("hi world!"));
    /// ```
    fn is_match(&'a self, text: &'a str) -> bool {
        if text.is_empty() {
            return false;
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.is_match_preprocessed(&processed_text_process_type_masks)
    }
    /// Returns a **lazy** iterator over [`RegexResult`] matches for the given text.
    ///
    /// Text preprocessing (`reduce_text_process_with_tree`) is performed once upfront.
    /// Pattern matching is then driven table-by-table. By utilizing a generator (`gen move`),
    /// matches from a particular regex target are yielded immediately before proceeding to
    /// evaluate subsequent regex patterns.
    ///
    /// A `HashSet` named `table_id_index_set` is utilized to deduplicate matches where the
    /// same rule hits exactly the same token across different text-processing variants
    /// (e.g. hitting both on lowercase and Fanjian normalized outputs).
    ///
    /// # Arguments
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    /// An `impl Iterator<Item = RegexResult<'a>>` — a lazy sequence yielding matched expressions.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{RegexTable, RegexMatchType, ProcessType, RegexMatcher, TextMatcherTrait};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     process_type: ProcessType::None,
    ///     regex_match_type: RegexMatchType::Regex,
    ///     word_list: vec!["apple", "banana"],
    /// };
    ///
    /// let matcher = RegexMatcher::new(&[regex_table]);
    ///
    /// let mut iter = matcher.process_iter("I have an apple and a banana");
    /// assert!(iter.next().is_some());
    /// assert!(iter.next().is_some());
    /// assert!(iter.next().is_none());
    /// ```
    fn process_iter(&'a self, text: &'a str) -> impl Iterator<Item = RegexResult<'a>> + 'a {
        gen move {
            if text.is_empty() {
                return;
            }

            let processed_text_process_type_masks =
                reduce_text_process_with_tree(&self.process_type_tree, text);

            let mut table_id_index_set = HashSet::new();

            for (processed_text, process_type_mask) in processed_text_process_type_masks {
                for regex_pattern_table in self.regex_pattern_table_list.iter() {
                    if (process_type_mask & (1u64 << regex_pattern_table.process_type.bits())) == 0
                    {
                        continue;
                    }

                    match &regex_pattern_table.regex_type {
                        RegexType::Standard { regex } => {
                            if table_id_index_set.insert(regex_pattern_table.table_id as usize) {
                                let mut temp = Vec::new();
                                for caps in regex.captures_iter(&processed_text).flatten() {
                                    temp.push(RegexResult {
                                        match_id: regex_pattern_table.match_id,
                                        table_id: regex_pattern_table.table_id,
                                        word_id: 0,
                                        word: Cow::Owned(
                                            caps.iter()
                                                .skip(1)
                                                .filter_map(|m| m.map(|mc| mc.as_str()))
                                                .collect::<String>(),
                                        ),
                                    });
                                }
                                for r in temp {
                                    yield r;
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

                                if table_id_index_set.insert(table_id_index)
                                    && let Ok(true) = regex.is_match(&processed_text)
                                {
                                    yield RegexResult {
                                        match_id: regex_pattern_table.match_id,
                                        table_id: regex_pattern_table.table_id,
                                        word_id: index as u32,
                                        word: Cow::Borrowed(&word_list[index]),
                                    };
                                }
                            }
                        }
                        RegexType::Set {
                            regex_set,
                            word_list,
                        } => {
                            let mut temp = Vec::new();
                            for index in regex_set.matches(&processed_text) {
                                let table_id_index =
                                    ((regex_pattern_table.table_id as usize) << 32) | index;

                                if table_id_index_set.insert(table_id_index) {
                                    temp.push(RegexResult {
                                        match_id: regex_pattern_table.match_id,
                                        table_id: regex_pattern_table.table_id,
                                        word_id: index as u32,
                                        word: Cow::Borrowed(&word_list[index]),
                                    });
                                }
                            }
                            for r in temp {
                                yield r;
                            }
                        }
                    }
                }
            }
        }
    }
}

impl<'a> TextMatcherInternal<'a, RegexResult<'a>> for RegexMatcher {
    /// Checks if any of the given processed texts match any internal regex pattern.
    ///
    /// # Algorithm
    /// The function iterates over all `(processed_text, process_type_mask)` pairs representing
    /// variations of the original text. It iterates over internal regex tables:
    /// 1. It checks if the table’s `process_type` bits align with the current `process_type_mask`.
    /// 2. If valid, evaluates the `RegexType`:
    ///    - `Standard`: executes `.is_match()`.
    ///    - `List`: iterates the list returning true on the first `.is_match()`.
    ///    - `Set`: executes `.is_match()` directly on the highly-optimized combined regex DFA.
    ///
    /// As soon as any match evaluates to `true`, the loop terminates early.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple contains a processed text variant (as [`Cow<'a, str>`]) and a `u64` bitmask of applicable process type IDs.
    ///
    /// # Returns
    /// `true` if at least one regex matches the preprocessed text variant; `false` otherwise.
    fn is_match_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> bool {
        for (processed_text, process_type_mask) in processed_text_process_type_masks {
            for regex_pattern_table in &self.regex_pattern_table_list {
                if (process_type_mask & (1u64 << regex_pattern_table.process_type.bits())) == 0 {
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

    /// Eagerly evaluates the reduced text variations against all compiled regular expressions,
    /// returning a complete vector of matches.
    ///
    /// # Algorithm
    /// Similar to `is_match_preprocessed`, but gathers *all* distinct rules that matched.
    /// - `table_id_index_set` guarantees that a given Regex identifier (`(table_id << 32) | id_index`)
    ///   is only recorded once, even if it fires across multiple text manipulations (e.g. original vs lowecased).
    /// - For a `RegexType::Standard`, it leverages `captures_iter` to extract matched token subgroups.
    /// - For `RegexType::Set` or `RegexType::List`, it identifies which internal index triggered
    ///   the hit and associates it with the corresponding original word from the `word_list`.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple contains a processed text variant (as [`Cow<'a, str>`]) and a `u64` bitmask of applicable process type IDs.
    ///
    /// # Returns
    /// A vector of [`RegexResult`] instances corresponding to every distinct pattern that successfully matched.
    fn process_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<RegexResult<'a>> {
        let mut result_list = Vec::new();
        let mut table_id_index_set = HashSet::new();

        for (processed_text, process_type_mask) in processed_text_process_type_masks {
            for regex_pattern_table in &self.regex_pattern_table_list {
                if (process_type_mask & (1u64 << regex_pattern_table.process_type.bits())) == 0 {
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

                            if table_id_index_set.insert(table_id_index)
                                && let Ok(is_match) = regex.is_match(processed_text)
                                && is_match
                            {
                                result_list.push(RegexResult {
                                    match_id: regex_pattern_table.match_id,
                                    table_id: regex_pattern_table.table_id,
                                    word_id: index as u32,
                                    word: Cow::Borrowed(&word_list[index]),
                                });
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
