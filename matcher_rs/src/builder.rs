use std::collections::HashMap;

use crate::{MatchTable, Matcher, ProcessType, SimpleMatcher};

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
        self.word_map
            .entry(process_type)
            .or_default()
            .insert(word_id, word);
        self
    }

    /// Consumes the builder and constructs the `SimpleMatcher`.
    pub fn build(self) -> SimpleMatcher {
        SimpleMatcher::new(&self.word_map)
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
