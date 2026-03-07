use std::collections::HashMap;

use crate::{ProcessType, SimpleMatcher};

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
