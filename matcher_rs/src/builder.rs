use std::collections::HashMap;

use crate::{MatchTable, MatchTableType, Matcher, ProcessType, SimpleMatcher};

/// A builder for constructing a `SimpleMatcher`.
///
/// This builder provides a convenient and ergonomic API for constructing a `SimpleMatcher`
/// without needing to manually build and nest HashMaps.
///
/// # Example
///
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
    /// Creates a new, empty `SimpleMatcherBuilder`.
    pub fn new() -> Self {
        Self {
            word_map: HashMap::new(),
        }
    }

    /// Adds a word to the builder for a specific `ProcessType`.
    ///
    /// # Arguments
    ///
    /// * `process_type` - The text processing pipeline to apply before matching this word.
    /// * `word_id` - The unique identifier for this word.
    /// * `word` - The actual word or pattern to match.
    pub fn add_word(mut self, process_type: ProcessType, word_id: u32, word: &'a str) -> Self {
        let bucket = self.word_map.entry(process_type).or_default();
        bucket.insert(word_id, word);
        self
    }

    /// Consumes the builder and constructs the `SimpleMatcher`.
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
/// # Example
///
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
    /// Creates a new `MatchTableBuilder` with the two required fields.
    ///
    /// # Arguments
    ///
    /// * `table_id` - The unique identifier for the table.
    /// * `match_table_type` - The matching strategy (Simple, Regex, or Similar).
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
    pub fn add_word(mut self, word: &'a str) -> Self {
        self.word_list.push(word);
        self
    }

    /// Appends multiple words to the match word list.
    pub fn add_words(mut self, words: impl IntoIterator<Item = &'a str>) -> Self {
        self.word_list.extend(words);
        self
    }

    /// Sets the [`ProcessType`] applied to exemption words.
    ///
    /// Defaults to [`ProcessType::None`] if not called.
    pub fn exemption_process_type(mut self, process_type: ProcessType) -> Self {
        self.exemption_process_type = process_type;
        self
    }

    /// Appends a single word to the exemption word list.
    pub fn add_exemption_word(mut self, word: &'a str) -> Self {
        self.exemption_word_list.push(word);
        self
    }

    /// Appends multiple words to the exemption word list.
    pub fn add_exemption_words(mut self, words: impl IntoIterator<Item = &'a str>) -> Self {
        self.exemption_word_list.extend(words);
        self
    }

    /// Consumes the builder and returns the configured [`MatchTable`].
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

/// A builder for constructing a `Matcher`.
///
/// This builder provides a convenient way to construct a `Matcher` interpolator
/// by adding complete `MatchTable` entries iteratively.
///
/// # Example
///
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
    /// Creates a new, empty `MatcherBuilder`.
    pub fn new() -> Self {
        Self {
            table_map: HashMap::new(), // Use strictly HashMap to bridge constructor compatibility
        }
    }

    /// Adds a `MatchTable` to a specific `match_id` group.
    ///
    /// # Arguments
    ///
    /// * `match_id` - The overarching match rule this table belongs to.
    /// * `table` - The `MatchTable` configuration.
    pub fn add_table(mut self, match_id: u32, table: MatchTable<'a>) -> Self {
        self.table_map.entry(match_id).or_default().push(table);
        self
    }

    /// Consumes the builder and constructs the unified `Matcher`.
    pub fn build(self) -> Matcher {
        Matcher::new(&self.table_map)
    }
}
