use std::collections::HashMap;

use crate::{MatchTable, MatchTableType, Matcher, ProcessType, SimpleMatcher};

/// A builder for constructing a [`SimpleMatcher`].
///
/// This builder provides a convenient and ergonomic API for constructing a [`SimpleMatcher`]
/// without needing to manually build and nest HashMaps. It allows adding words incrementally,
/// grouped by their intended text processing pipeline.
///
/// # Detailed Explanation / Algorithm
/// The builder collects word patterns into a nested map structure: `HashMap<ProcessType, HashMap<word_id, word_pattern>>`.
/// When `build()` is called, this structure is passed to `SimpleMatcher::new`, which then:
/// 1. Parses logical operators (`&`, `~`) in each pattern.
/// 2. Deduplicates sub-patterns across different process types.
/// 3. Compiles an optimized Aho-Corasick automaton for efficient matching.
///
/// # Type Parameters
/// * `'a` - The lifetime of the word patterns and strings.
///
/// # Fields
/// * `word_map` - A nested hash map storing words grouped by their [`ProcessType`] and uniquely identified by a `word_id`.
///
/// # Examples
/// ```rust
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 1, "hello")
///     .add_word(ProcessType::None, 2, "world")
///     .add_word(ProcessType::Fanjian, 3, "你好")
///     .build();
/// ```
#[derive(Default)]
pub struct SimpleMatcherBuilder<'a> {
    word_map: HashMap<ProcessType, HashMap<u32, &'a str>>,
}

impl<'a> SimpleMatcherBuilder<'a> {
    /// Creates a new, empty [`SimpleMatcherBuilder`].
    ///
    /// # Returns
    /// An empty [`SimpleMatcherBuilder`] with a default `word_map`.
    pub fn new() -> Self {
        Self {
            word_map: HashMap::new(),
        }
    }

    /// Adds a word to the builder for a specific [`ProcessType`].
    ///
    /// # Detailed Explanation / Algorithm
    /// This method inserts or updates a word in the internal `word_map`. It ensures that each
    /// word is associated with a specific `ProcessType`, allowing the matcher to apply the correct
    /// transformations (like Traditional to Simplified Chinese) before attempting a match.
    ///
    /// # Arguments
    /// * `process_type` - The text processing pipeline to apply before matching this word.
    /// * `word_id` - The unique identifier for this word, used to identify it in match results.
    /// * `word` - The actual word or pattern (supporting `&` and `~`) to match.
    ///
    /// # Returns
    /// The modified [`SimpleMatcherBuilder`] (fluent interface).
    pub fn add_word(mut self, process_type: ProcessType, word_id: u32, word: &'a str) -> Self {
        let bucket = self.word_map.entry(process_type).or_default();
        bucket.insert(word_id, word);
        self
    }

    /// Consumes the builder and constructs the [`SimpleMatcher`].
    ///
    /// # Detailed Explanation / Algorithm
    /// This method transfers the accumulated word patterns to the `SimpleMatcher::new` constructor.
    /// The initialization of `SimpleMatcher` is computationally expensive as it involves parsing,
    /// deduplication, and DFA compilation.
    ///
    /// # Returns
    /// The fully initialized and compiled [`SimpleMatcher`].
    pub fn build(self) -> SimpleMatcher {
        SimpleMatcher::new(&self.word_map)
    }
}

/// A builder for constructing a single [`MatchTable`].
///
/// This builder provides a fluent, ergonomic API for building a [`MatchTable`]
/// without having to construct the struct literal directly. The two required
/// fields — `table_id` and `match_table_type` — are supplied upfront in
/// [`MatchTableBuilder::new`]; everything else is optional and can be added
/// incrementally before calling [`build`](MatchTableBuilder::build).
///
/// # Detailed Explanation / Algorithm
/// The builder accumulates configuration for a single match table:
/// 1. A unique `table_id` for tracking.
/// 2. A [`MatchTableType`] defining the engine and preprocessing to use.
/// 3. Lists of match patterns and optional exemption patterns.
///
/// When `build()` is called, these are bundled into a [`MatchTable`] struct which
/// can then be added to a [`MatcherBuilder`].
///
/// # Type Parameters
/// * `'a` - The lifetime of the word patterns and strings.
///
/// # Fields
/// * `table_id` - A unique identifier for the specific matching table.
/// * `match_table_type` - The specific matching strategy and configuration used for this table.
/// * `word_list` - A list of words to be used in the matching process.
/// * `exemption_process_type` - The text processing rules to be applied to exemption words.
/// * `exemption_word_list` - A list of words that trigger exemptions from matching.
///
/// # Examples
/// ```rust
/// use matcher_rs::{MatchTableBuilder, MatchTableType, ProcessType, MatcherBuilder};
///
/// let table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
///     .add_word("hello")
///     .add_word("world")
///     .add_exemption_word("goodbye")
///     .build();
///
/// let matcher = MatcherBuilder::new()
///     .add_table(1, table)
///     .build();
/// ```
pub struct MatchTableBuilder<'a> {
    table_id: u32,
    match_table_type: MatchTableType,
    word_list: Vec<&'a str>,
    exemption_process_type: ProcessType,
    exemption_word_list: Vec<&'a str>,
}

impl<'a> MatchTableBuilder<'a> {
    /// Creates a new [`MatchTableBuilder`] with the two required fields.
    ///
    /// # Arguments
    /// * `table_id` - The unique identifier for the table.
    /// * `match_table_type` - The matching strategy (Simple, Regex, or Similar).
    ///
    /// # Returns
    /// A new [`MatchTableBuilder`] with empty word lists.
    pub fn new(table_id: u32, match_table_type: MatchTableType) -> Self {
        Self {
            table_id,
            match_table_type,
            word_list: Vec::new(),
            exemption_process_type: ProcessType::None,
            exemption_word_list: Vec::new(),
        }
    }

    /// Appends a single word to the match word list.
    ///
    /// # Arguments
    /// * `word` - A word or pattern to match.
    ///
    /// # Returns
    /// The modified [`MatchTableBuilder`] (fluent interface).
    pub fn add_word(mut self, word: &'a str) -> Self {
        self.word_list.push(word);
        self
    }

    /// Appends multiple words to the match word list.
    ///
    /// # Arguments
    /// * `words` - An iterator of words to append to the match list.
    ///
    /// # Returns
    /// The modified [`MatchTableBuilder`] (fluent interface).
    pub fn add_words(mut self, words: impl IntoIterator<Item = &'a str>) -> Self {
        self.word_list.extend(words);
        self
    }

    /// Sets the [`ProcessType`] applied to exemption words.
    ///
    /// Defaults to [`ProcessType::None`] if not called.
    ///
    /// # Arguments
    /// * `process_type` - The text processing pipeline for exemptions.
    ///
    /// # Returns
    /// The modified [`MatchTableBuilder`] (fluent interface).
    pub fn exemption_process_type(mut self, process_type: ProcessType) -> Self {
        self.exemption_process_type = process_type;
        self
    }

    /// Appends a single word to the exemption word list.
    ///
    /// # Arguments
    /// * `word` - A word that, if matched, will exempt the entire table from reporting results.
    ///
    /// # Returns
    /// The modified [`MatchTableBuilder`] (fluent interface).
    pub fn add_exemption_word(mut self, word: &'a str) -> Self {
        self.exemption_word_list.push(word);
        self
    }

    /// Appends multiple words to the exemption word list.
    ///
    /// # Arguments
    /// * `words` - An iterator of words to exempt from matching results.
    ///
    /// # Returns
    /// The modified [`MatchTableBuilder`] (fluent interface).
    pub fn add_exemption_words(mut self, words: impl IntoIterator<Item = &'a str>) -> Self {
        self.exemption_word_list.extend(words);
        self
    }

    /// Consumes the builder and returns the configured [`MatchTable`].
    ///
    /// # Returns
    /// The constructed [`MatchTable`] containing all added words and configurations.
    pub fn build(self) -> MatchTable<'a> {
        MatchTable {
            table_id: self.table_id,
            match_table_type: self.match_table_type,
            word_list: self.word_list,
            exemption_process_type: self.exemption_process_type,
            exemption_word_list: self.exemption_word_list,
        }
    }
}

/// A builder for constructing a [`Matcher`].
///
/// This builder provides a convenient way to construct a [`Matcher`]
/// by adding complete [`MatchTable`] entries iteratively.
///
/// # Detailed Explanation / Algorithm
/// The `MatcherBuilder` groups [`MatchTable`]s under rule-level identifiers (`match_id`).
/// When `build()` is called, it constructs a complex [`Matcher`] that:
/// 1. Combines all [`MatchTable`] entries.
/// 2. Deduplicates text processing workflows across tables.
/// 3. Initializes the internal engines (`SimpleMatcher`, `RegexMatcher`, `SimMatcher`) for each table category.
///
/// # Type Parameters
/// * `'a` - The lifetime of the word patterns and strings inside the tables.
///
/// # Fields
/// * `table_map` - A map grouping [`MatchTable`] instances under rule-level identifiers (`match_id`).
///
/// # Examples
/// ```rust
/// use matcher_rs::{MatcherBuilder, MatchTable, MatchTableType, ProcessType};
///
/// let table = MatchTable {
///     table_id: 1,
///     match_table_type: MatchTableType::Simple { process_type: ProcessType::None },
///     word_list: vec!["hello"],
///     exemption_process_type: ProcessType::None,
///     exemption_word_list: vec![],
/// };
///
/// let matcher = MatcherBuilder::new()
///     .add_table(1, table)
///     .build();
/// ```
#[derive(Default)]
pub struct MatcherBuilder<'a> {
    table_map: HashMap<u32, Vec<MatchTable<'a>>>,
}

impl<'a> MatcherBuilder<'a> {
    /// Creates a new, empty [`MatcherBuilder`].
    ///
    /// # Returns
    /// An empty [`MatcherBuilder`] with a default `table_map`.
    pub fn new() -> Self {
        Self {
            table_map: HashMap::new(), // Use strictly HashMap to bridge constructor compatibility
        }
    }

    /// Adds a [`MatchTable`] to a specific `match_id` group.
    ///
    /// # Arguments
    /// * `match_id` - The overarching match rule this table belongs to.
    /// * `table` - The [`MatchTable`] configuration.
    ///
    /// # Returns
    /// The modified [`MatcherBuilder`] (fluent interface).
    pub fn add_table(mut self, match_id: u32, table: MatchTable<'a>) -> Self {
        self.table_map.entry(match_id).or_default().push(table);
        self
    }

    /// Consumes the builder and constructs the unified [`Matcher`].
    ///
    /// # Returns
    /// The constructed [`Matcher`] with its engines and workflow DAG fully compiled.
    pub fn build(self) -> Matcher {
        Matcher::new(&self.table_map)
    }
}
