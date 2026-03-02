use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::process::process_matcher::{
    ProcessType, ProcessTypeBitNode, ProcessedTextMasks, build_process_type_tree,
    reduce_text_process_with_tree,
};
use crate::regex_matcher::{RegexMatchType, RegexMatcher, RegexResult, RegexTable};
use crate::sim_matcher::{SimMatchType, SimMatcher, SimResult, SimTable};
use crate::simple_matcher::{SimpleMatcher, SimpleTable};

/// Text-matching trait shared by all matcher types.
///
/// This trait provides a unified interface for checking matches and processing text
/// across different matching engines ([`SimpleMatcher`], [`RegexMatcher`], [`SimMatcher`], and the aggregate [`Matcher`]).
///
/// # Type Parameters
/// * `'a` - Lifetime parameter associated with the input text and match results.
/// * `T` - A type that implements [`MatchResultTrait<'a>`], representing the result of a match.
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement text matching",
    label = "this type cannot be used as a matcher",
    note = "implement `TextMatcherTrait` or use one of the built-in matchers: `SimpleMatcher`, `RegexMatcher`, `SimMatcher`, or `Matcher`"
)]
pub trait TextMatcherTrait<'a, T: MatchResultTrait<'a> + 'a> {
    /// Returns `true` if the given text matches any pattern in the matcher.
    ///
    /// # Arguments
    /// * `text` - The input string to check for matches.
    ///
    /// # Returns
    /// `true` if a match is found, `false` otherwise.
    fn is_match(&'a self, text: &'a str) -> bool;

    /// Processes the text and returns all matching results.
    ///
    /// # Arguments
    /// * `text` - The input string to search for patterns.
    ///
    /// # Returns
    /// A vector of match results `T`.
    fn process(&'a self, text: &'a str) -> Vec<T>;

    /// Returns an iterator over all match results.
    ///
    /// # Arguments
    /// * `text` - The input string to search for patterns.
    ///
    /// # Returns
    /// An iterator yielding match results `T`.
    fn process_iter(&'a self, text: &'a str) -> impl Iterator<Item = T> + 'a {
        self.process(text).into_iter()
    }

    /// Checks for matches using pre-processed text and its associated process type masks.
    ///
    /// # Detailed Explanation / Algorithm
    /// This is an optimization for scenarios where the text has already been normalized
    /// (e.g., converted to pinyin or simplified Chinese) by another part of the system.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - A collection of pre-processed string variants and their type bitmasks.
    ///
    /// # Returns
    /// `true` if a match is found.
    fn is_match_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> bool;

    /// Processes pre-processed text and returns all matching results.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - A collection of pre-processed string variants and their type bitmasks.
    ///
    /// # Returns
    /// A vector of match results `T`.
    fn process_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<T>;
}

/// A trait defining the required methods for a match result.
///
/// This trait ensures a consistent interface for accessing properties of a match,
/// such as which rule triggered it, which table it came from, and which specific word matched.
///
/// # Type Parameters
/// * `'a` - A lifetime parameter indicating how long the matched word reference remains valid.
///
/// # Examples
/// ```rust
/// use std::borrow::Cow;
/// use matcher_rs::MatchResultTrait;
///
/// struct MyResult<'a> {
///     word: &'a str,
/// }
///
/// impl<'a> MatchResultTrait<'a> for MyResult<'a> {
///     fn match_id(&self) -> u32 { 1 }
///     fn table_id(&self) -> u32 { 1 }
///     fn word_id(&self) -> u32 { 1 }
///     fn word(&self) -> &str { self.word }
///     fn similarity(&self) -> Option<f64> { None }
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement `MatchResultTrait`",
    label = "this type cannot be used as a match result",
    note = "implement `MatchResultTrait` with `match_id`, `table_id`, `word_id`, `word`, and `similarity` methods"
)]
pub trait MatchResultTrait<'a> {
    /// Returns the high-level match identifier.
    fn match_id(&self) -> u32;
    /// Returns the specific table identifier within the match rule.
    fn table_id(&self) -> u32;
    /// Returns the identifier of the matched word within the table.
    fn word_id(&self) -> u32;
    /// Returns the matched word string.
    fn word(&self) -> &str;
    /// Returns the similarity score, if applicable (e.g., for fuzzy matching).
    fn similarity(&self) -> Option<f64>;
}

/// An enumeration representing different types of match tables and their configurations.
///
/// This enum determines which matching engine is used and how the text is pre-processed for a given set of words.
///
/// # Variants
/// * `Simple` - Exact matching (with `&` and `~` support) using Aho-Corasick.
/// * `Regex` - Pattern matching using regular expressions.
/// * `Similar` - Fuzzy matching based on edit distance (Levenshtein, etc.).
///
/// # Fields
/// * `process_type` - The normalization pipeline to apply.
/// * `regex_match_type` - (Regex only) Strategy for regex matching.
/// * `sim_match_type` - (Similar only) The distance metric to use.
/// * `threshold` - (Similar only) The minimum similarity score (0.0 to 1.0) to consider a match.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MatchTableType {
    Simple {
        process_type: ProcessType,
    },
    Regex {
        regex_match_type: RegexMatchType,
        process_type: ProcessType,
    },
    Similar {
        sim_match_type: SimMatchType,
        threshold: f64,
        process_type: ProcessType,
    },
}

/// A trait for accessing configuration of a match table.
///
/// # Type Parameters
/// * `S` - A type that can be treated as a string slice (`AsRef<str>`).
pub trait MatchTableTrait<S: AsRef<str>> {
    /// Returns the unique identifier for this table.
    fn table_id(&self) -> u32;
    /// Returns the strategy and preprocessing config.
    fn match_table_type(&self) -> MatchTableType;
    /// Returns the list of patterns to match.
    fn word_list(&self) -> &[S];
    /// Returns the preprocessing to apply to exemptions.
    fn exemption_process_type(&self) -> ProcessType;
    /// Returns the list of words that block matches if they appear.
    fn exemption_word_list(&self) -> &[S];
}

/// A configuration structure representing a match table.
///
/// Match tables are the building blocks of the [`Matcher`]. They define a matching strategy,
/// a list of words to match, and optional exemption words that suppress results from this table.
///
/// # Type Parameters
/// * `'a` - The lifetime of the borrowed strings in the word lists.
///
/// # Fields
/// * `table_id` - A unique identifier for this table.
/// * `match_table_type` - The engine and preprocessing config (Simple, Regex, or Similar).
/// * `word_list` - The patterns to search for.
/// * `exemption_process_type` - Preprocessing to apply to exemption words.
/// * `exemption_word_list` - Words that, if matched, prevent this table from reporting any results.
///
/// # Examples
/// ```rust
/// use matcher_rs::{MatchTable, MatchTableType, ProcessType};
///
/// let table = MatchTable {
///     table_id: 1,
///     match_table_type: MatchTableType::Simple { process_type: ProcessType::None },
///     word_list: vec!["apple"],
///     exemption_process_type: ProcessType::None,
///     exemption_word_list: vec!["pineapple"],
/// };
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MatchTable<'a> {
    pub table_id: u32,
    pub match_table_type: MatchTableType,
    #[serde(borrow)]
    pub word_list: Vec<&'a str>,
    pub exemption_process_type: ProcessType,
    #[serde(borrow)]
    pub exemption_word_list: Vec<&'a str>,
}

impl<'a> MatchTableTrait<&'a str> for MatchTable<'a> {
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn match_table_type(&self) -> MatchTableType {
        self.match_table_type
    }
    fn word_list(&self) -> &[&'a str] {
        &self.word_list
    }
    fn exemption_process_type(&self) -> ProcessType {
        self.exemption_process_type
    }
    fn exemption_word_list(&self) -> &[&'a str] {
        &self.exemption_word_list
    }
}

/// A serializable version of [`MatchTable`] using `Cow` for string ownership.
///
/// This is used when the data is loaded from a source where strings are dynamically allocated (e.g., JSON).
///
/// # Type Parameters
/// * `'a` - The lifetime of the strings (can be owned or borrowed).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MatchTableSerde<'a> {
    pub table_id: u32,
    pub match_table_type: MatchTableType,
    #[serde(borrow)]
    pub word_list: Vec<Cow<'a, str>>,
    pub exemption_process_type: ProcessType,
    #[serde(borrow)]
    pub exemption_word_list: Vec<Cow<'a, str>>,
}

impl<'a> MatchTableTrait<Cow<'a, str>> for MatchTableSerde<'a> {
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn match_table_type(&self) -> MatchTableType {
        self.match_table_type
    }
    fn word_list(&self) -> &[Cow<'a, str>] {
        &self.word_list
    }
    fn exemption_process_type(&self) -> ProcessType {
        self.exemption_process_type
    }
    fn exemption_word_list(&self) -> &[Cow<'a, str>] {
        &self.exemption_word_list
    }
}

/// Internal metadata for mapping a simple word match back to its originating table.
#[derive(Debug, Clone)]
struct WordTableConf {
    match_id: u32,
    table_id: u32,
    offset: u32,
    is_exemption: bool,
}

/// The result of a matching operation.
///
/// # Type Parameters
/// * `'a` - The lifetime of the matched word string.
///
/// # Fields
/// * `match_id` - The ID of the top-level matching rule.
/// * `table_id` - The ID of the specific table that matched.
/// * `word_id` - The index of the matched word within its table.
/// * `word` - The matched string itself (or the original pattern for simple matches).
/// * `similarity` - Similarity score (0.0 to 1.0) for fuzzy matches; `None` for exact or regex matches.
#[derive(Serialize, Debug)]
pub struct MatchResult<'a> {
    pub match_id: u32,
    pub table_id: u32,
    pub word_id: u32,
    pub word: Cow<'a, str>,
    pub similarity: Option<f64>,
}

impl MatchResultTrait<'_> for MatchResult<'_> {
    fn match_id(&self) -> u32 {
        self.match_id
    }
    fn word_id(&self) -> u32 {
        self.word_id
    }
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
    fn similarity(&self) -> Option<f64> {
        self.similarity
    }
}

impl<'a, 'b: 'a> From<SimResult<'b>> for MatchResult<'a> {
    fn from(sim_result: SimResult<'b>) -> Self {
        MatchResult {
            match_id: sim_result.match_id,
            table_id: sim_result.table_id,
            word_id: sim_result.word_id,
            word: sim_result.word,
            similarity: Some(sim_result.similarity),
        }
    }
}

impl<'a, 'b: 'a> From<RegexResult<'b>> for MatchResult<'a> {
    fn from(regex_result: RegexResult<'b>) -> Self {
        MatchResult {
            match_id: regex_result.match_id,
            table_id: regex_result.table_id,
            word_id: regex_result.word_id,
            word: regex_result.word,
            similarity: None,
        }
    }
}

/// Alias for a map of match IDs to their corresponding tables.
pub type MatchTableMap<'a> = HashMap<u32, Vec<MatchTable<'a>>>;

/// Alias for a serializable map of match IDs to their corresponding tables.
pub type MatchTableMapSerde<'a> = HashMap<u32, Vec<MatchTableSerde<'a>>>;

/// Orchestrates multi-engine text matching.
///
/// [`Matcher`] is the primary entry point for complex matching tasks. It manages multiple
/// [`SimpleMatcher`], [`RegexMatcher`], and [`SimMatcher`] instances and handles
/// the orchestration of text preprocessing and result aggregation.
///
/// # Detailed Explanation / Algorithm
/// 1. **Initialization**: On `new()`, it compiles a unified `ProcessTypeBitNode` DAG to minimize
///    redundant text transformations. It groups tables by their engine type.
/// 2. **Matching**:
///    - It first applies all required `ProcessType` transformations to the input text.
///    - It dispatches the pre-processed variants to each active engine.
///    - **Exemptions**: If a `SimpleMatcher` hit occurs in an exemption list, it suppresses
///      any other hits from the same `table_id`.
/// 3. **Result Aggregation**: All hits are collected, mapped back to their user-defined
///    identifiers, and returned as a grouped map.
///
/// # Fields
/// * `process_type_tree` - Compiled DAG for efficient text transformations.
/// * `simple_word_table_conf_list` - Internal metadata for mapping simple hits back to IDs.
/// * `simple_word_table_conf_index_list` - O(1) index into the config list.
/// * `simple_matcher` - The exact matching engine.
/// * `regex_matcher` - The regex matching engine.
/// * `sim_matcher` - The fuzzy matching engine.
///
/// # Examples
/// ```rust
/// use matcher_rs::{Matcher, MatcherBuilder, MatchTableBuilder, MatchTableType, ProcessType};
///
/// let table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
///     .add_word("apple")
///     .build();
///
/// let matcher = MatcherBuilder::new()
///     .add_table(100, table)
///     .build();
///
/// let results = matcher.word_match("I have an apple");
/// assert!(results.contains_key(&100));
/// ```
#[derive(Debug, Clone)]
pub struct Matcher {
    process_type_tree: Box<[ProcessTypeBitNode]>,
    simple_word_table_conf_list: Box<[WordTableConf]>,
    simple_word_table_conf_index_list: Box<[usize]>,
    simple_matcher: Option<SimpleMatcher>,
    regex_matcher: Option<RegexMatcher>,
    sim_matcher: Option<SimMatcher>,
}

impl Matcher {
    /// Constructs a new [`Matcher`] from a map of tables.
    ///
    /// It is recommended to use [`MatcherBuilder`] instead of calling this directly.
    ///
    /// # Type Parameters
    /// * `S` - Hasher for the map.
    /// * `M` - The table type (implements [`MatchTableTrait`]).
    /// * `T` - The string type in the table (implements `AsRef<str>`).
    ///
    /// # Arguments
    /// * `match_table_map` - A map where keys are `match_id` and values are lists of tables.
    ///
    /// # Returns
    /// A fully initialized [`Matcher`].
    pub fn new<S, M, T>(match_table_map: &HashMap<u32, Vec<M>, S>) -> Matcher
    where
        M: MatchTableTrait<T>,
        T: AsRef<str>,
    {
        let mut process_type_set = HashSet::new();

        let mut simple_word_id = 0;
        let mut simple_word_table_conf_id = 0;
        let mut simple_word_table_conf_list = Vec::new();
        let mut simple_word_table_conf_index_list = Vec::new();
        let mut simple_table: SimpleTable = HashMap::new();

        let mut regex_table_list = Vec::new();
        let mut sim_table_list = Vec::new();

        for (&match_id, table_list) in match_table_map {
            for table in table_list {
                let table_id = table.table_id();
                let match_table_type = table.match_table_type();
                let word_list = table
                    .word_list()
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<&str>>();
                let exemption_process_type = table.exemption_process_type();
                let exemption_word_list = table
                    .exemption_word_list()
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<&str>>();

                if !word_list.is_empty() {
                    match match_table_type {
                        MatchTableType::Simple { process_type } => {
                            process_type_set.insert(process_type.bits());
                            simple_word_table_conf_list.push(WordTableConf {
                                match_id,
                                table_id,
                                offset: simple_word_id,
                                is_exemption: false,
                            });

                            let simple_word_map = simple_table.entry(process_type).or_default();

                            for word in word_list {
                                simple_word_table_conf_index_list.push(simple_word_table_conf_id);
                                simple_word_map.insert(simple_word_id, word);
                                simple_word_id += 1;
                            }

                            simple_word_table_conf_id += 1
                        }
                        MatchTableType::Similar {
                            process_type,
                            sim_match_type,
                            threshold,
                        } => {
                            process_type_set.insert(process_type.bits());
                            sim_table_list.push(SimTable {
                                table_id,
                                match_id,
                                process_type,
                                sim_match_type,
                                word_list,
                                threshold,
                            })
                        }
                        MatchTableType::Regex {
                            process_type,
                            regex_match_type,
                        } => {
                            process_type_set.insert(process_type.bits());
                            regex_table_list.push(RegexTable {
                                table_id,
                                match_id,
                                process_type,
                                regex_match_type,
                                word_list,
                            })
                        }
                    }
                }

                if !exemption_word_list.is_empty() {
                    process_type_set.insert(exemption_process_type.bits());
                    simple_word_table_conf_list.push(WordTableConf {
                        match_id,
                        table_id,
                        offset: simple_word_id,
                        is_exemption: true,
                    });

                    let simple_word_map = simple_table.entry(exemption_process_type).or_default();

                    for exemption_word in exemption_word_list {
                        simple_word_table_conf_index_list.push(simple_word_table_conf_id);
                        simple_word_map.insert(simple_word_id, exemption_word);
                        simple_word_id += 1;
                    }

                    simple_word_table_conf_id += 1
                }
            }
        }

        let process_type_tree = build_process_type_tree(&process_type_set).into_boxed_slice();

        Matcher {
            process_type_tree,
            simple_word_table_conf_list: simple_word_table_conf_list.into_boxed_slice(),
            simple_word_table_conf_index_list: simple_word_table_conf_index_list.into_boxed_slice(),
            simple_matcher: (!simple_table.is_empty()).then(|| SimpleMatcher::new(&simple_table)),
            regex_matcher: (!regex_table_list.is_empty())
                .then(|| RegexMatcher::new(&regex_table_list)),
            sim_matcher: (!sim_table_list.is_empty()).then(|| SimMatcher::new(&sim_table_list)),
        }
    }

    /// Matches words in the given text based on the configured match tables.
    ///
    /// This function performs the complete matching workflow, including text normalization,
    /// engine dispatch, and result aggregation.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. **Preprocessing**: Reduces the input text using the `process_type_tree` to generate
    ///    all required normalized variants (e.g., simplified, pinyin).
    /// 2. **Core Matching**: Calls `_word_match_with_processed_text_process_type_masks`.
    /// 3. **Result Collection**: Returns the aggregated map of results.
    ///
    /// # Arguments
    /// * `text` - The input string to search.
    ///
    /// # Returns
    /// A [`HashMap`] where keys are match IDs and values are vectors of [`MatchResult`] items.
    /// Returns an empty map if the input is empty.
    ///
    /// # Examples
    /// ```rust
    /// use matcher_rs::{MatchTableBuilder, MatchTableType, ProcessType, MatcherBuilder};
    ///
    /// let table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
    ///     .add_word("detect")
    ///     .build();
    ///
    /// let matcher = MatcherBuilder::new().add_table(1, table).build();
    ///
    /// let result = matcher.word_match("we should detect this");
    /// assert!(result.contains_key(&1));
    /// ```
    pub fn word_match<'a>(&'a self, text: &'a str) -> HashMap<u32, Vec<MatchResult<'a>>> {
        if text.is_empty() {
            return HashMap::new();
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._word_match_with_processed_text_process_type_masks(&processed_text_process_type_masks)
    }

    /// Internal core matching logic.
    ///
    /// # Detailed Explanation / Algorithm
    /// 1. Initializes result map and exemption tracker.
    /// 2. Executes `regex_matcher` and `sim_matcher` if available, adding their results directly.
    /// 3. Executes `simple_matcher`. For each hit:
    ///    - Maps the internal word ID back to its originating table using `simple_word_table_conf_list`.
    ///    - If the hit is an **exemption**, it marks the table as failed and removes any previous hits from that table.
    ///    - If it's a **standard hit**, it checks if the table is already marked as failed before adding the result.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - Pre-processed variants and their bitmasks.
    ///
    /// # Returns
    /// Aggregated match results grouped by match ID.
    fn _word_match_with_processed_text_process_type_masks<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> HashMap<u32, Vec<MatchResult<'a>>> {
        let mut match_result_dict = HashMap::new();
        let mut failed_match_table_id_set = HashSet::new();

        if let Some(regex_matcher) = &self.regex_matcher {
            for regex_result in
                regex_matcher.process_preprocessed(processed_text_process_type_masks)
            {
                let result_list: &mut Vec<MatchResult> =
                    match_result_dict.entry(regex_result.match_id).or_default();

                result_list.push(regex_result.into());
            }
        }

        if let Some(sim_matcher) = &self.sim_matcher {
            for sim_result in sim_matcher.process_preprocessed(processed_text_process_type_masks) {
                let result_list = match_result_dict.entry(sim_result.match_id).or_default();

                result_list.push(sim_result.into());
            }
        }

        if let Some(simple_matcher) = &self.simple_matcher {
            for simple_result in
                simple_matcher.process_preprocessed(processed_text_process_type_masks)
            {
                let word_table_conf = self.simple_word_table_conf_list.get(
                    self.simple_word_table_conf_index_list[simple_result.word_id as usize],
                ).expect("simple_word_table_conf_index_list` is pre-populated guaranteeing index mapping corresponds directly to valid indices mapped within `simple_word_table_conf_list`.");
                let match_table_id = ((word_table_conf.match_id as usize) << 32)
                    | (word_table_conf.table_id as usize);

                if failed_match_table_id_set.contains(&match_table_id) {
                    continue;
                }

                let result_list = match_result_dict
                    .entry(word_table_conf.match_id)
                    .or_default();
                if word_table_conf.is_exemption {
                    failed_match_table_id_set.insert(match_table_id);
                    result_list
                        .retain(|match_result| match_result.table_id != word_table_conf.table_id);
                } else {
                    result_list.push(MatchResult {
                        match_id: word_table_conf.match_id,
                        table_id: word_table_conf.table_id,
                        word_id: simple_result.word_id - word_table_conf.offset,
                        word: simple_result.word,
                        similarity: None,
                    });
                }
            }
        }

        match_result_dict.retain(|_, match_result_list| !match_result_list.is_empty());
        match_result_dict
    }
}

impl<'a> TextMatcherTrait<'a, MatchResult<'a>> for Matcher {
    /// Checks if the given text matches any pattern in the match tables.
    ///
    /// This function processes the input text using the `process_type_tree`
    /// defined for the [`Matcher`] instance and then checks if any matches
    /// are found using the underlying match tables (simple, regex, and
    /// similarity match tables).
    ///
    /// # Arguments
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    /// * `bool` - Returns `true` if any matches are found, otherwise returns `false`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{MatchTableBuilder, MatchTableType, ProcessType, MatcherBuilder, TextMatcherTrait};
    ///
    /// let match_table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
    ///     .add_word("detect")
    ///     .build();
    ///
    /// let matcher = MatcherBuilder::new().add_table(1, match_table).build();
    ///
    /// assert!(matcher.is_match("we should detect this"));
    /// assert!(!matcher.is_match("clean text"));
    /// ```
    fn is_match(&self, text: &str) -> bool {
        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.is_match_preprocessed(&processed_text_process_type_masks)
    }
    /// Processes the input text to generate a list of match results.
    ///
    /// This function takes an input text string, processes it according to the
    /// [`Matcher`] instance's configured process type tree, and then generates a
    /// list of match results by applying the processed text against the configured
    /// match tables.
    ///
    /// The process involves reducing the input text based on the type tree, transforming
    /// it into a structured format (`processed_text_process_type_masks`) suitable for
    /// matching operations. The results are then aggregated into a single list of
    /// [`MatchResult`] instances.
    ///
    /// # Arguments
    ///
    /// * `text` - A reference to the input text string to be processed.
    ///
    /// # Returns
    ///
    /// * [`Vec<MatchResult<'a>>`] - A vector containing match results corresponding to
    ///   the patterns defined in the match tables.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{MatchTableBuilder, MatchTableType, ProcessType, MatcherBuilder, TextMatcherTrait};
    ///
    /// let match_table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
    ///     .add_words(["apple", "banana"])
    ///     .build();
    ///
    /// let matcher = MatcherBuilder::new().add_table(1, match_table).build();
    ///
    /// let results = matcher.process("I have an apple and a banana");
    /// assert_eq!(results.len(), 2);
    /// ```
    fn process(&'a self, text: &'a str) -> Vec<MatchResult<'a>> {
        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.process_preprocessed(&processed_text_process_type_masks)
    }

    /// Checks if there are any matches for the processed text within the configured match tables.
    ///
    /// This function takes a reference to a processed text set and determines if any matches
    /// exist within the match tables of the [`Matcher`] instance. The function prioritizes
    /// checking the simple matcher first. If the simple matcher is not configured or
    /// doesn't find any matches, it proceeds to check the regex matcher and then the
    /// similarity matcher, in that order.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple
    ///   contains a processed text piece (as [`Cow<str>`]) and a
    ///   u64 bitmask of process type IDs (`u64`).
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if any matches are found within any of the matchers, otherwise `false`.
    ///
    /// # Safety
    ///
    /// This function is safe to use under normal circumstances but depends on the reliability
    /// of the underlying matchers and the integrity of the `processed_text_process_type_masks`
    /// input. Ensure the input data is correctly processed and the matchers are properly
    /// initialized before calling this function.
    fn is_match_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> bool {
        if self.simple_matcher.is_some() {
            return !self
                ._word_match_with_processed_text_process_type_masks(
                    processed_text_process_type_masks,
                )
                .is_empty();
        }
        if let Some(regex_matcher) = &self.regex_matcher
            && regex_matcher.is_match_preprocessed(processed_text_process_type_masks)
        {
            return true;
        }
        if let Some(sim_matcher) = &self.sim_matcher
            && sim_matcher.is_match_preprocessed(processed_text_process_type_masks)
        {
            return true;
        }
        false
    }

    /// Aggregates match results by processing the pre-processed text with the configured matchers.
    ///
    /// This function takes a reference to a pre-processed text set (a list of tuples containing
    /// processed text and associated [`HashSet`]) and generates match results using the instance's
    /// configured matchers. The function focuses on word-level matching and aggregates the
    /// results into a single list of [`MatchResult`] instances.
    ///
    /// The process involves invoking the appropriate matcher to obtain match results for the
    /// provided pre-processed text and then flattening the results into a single vector.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple
    ///   contains a processed text piece (as [`Cow<str>`]) and a
    ///   u64 bitmask of process type IDs (`u64`).
    ///
    /// # Returns
    ///
    /// * [`Vec<MatchResult<'a>>`] - A vector containing aggregated match results generated
    ///   from the match IDs.
    fn process_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<MatchResult<'a>> {
        self._word_match_with_processed_text_process_type_masks(processed_text_process_type_masks)
            .into_values()
            .flatten()
            .collect()
    }
}
