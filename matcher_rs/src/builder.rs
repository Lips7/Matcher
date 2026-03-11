use std::collections::HashMap;

use crate::{ProcessType, SimpleMatcher};

/// Builder for constructing a [`SimpleMatcher`].
///
/// Accumulates word patterns grouped by their [`ProcessType`] processing pipeline,
/// then compiles everything into an optimized automaton in a single shot via [`build`](Self::build).
/// Prefer this over calling [`SimpleMatcher::new`] directly.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 1, "hello")
///     .add_word(ProcessType::None, 2, "world")
///     .add_word(ProcessType::Fanjian, 3, "你好")
///     .build();
///
/// assert!(matcher.is_match("hello world"));
/// ```
#[derive(Default)]
pub struct SimpleMatcherBuilder<'a> {
    word_map: HashMap<ProcessType, HashMap<u32, &'a str>>,
}

impl<'a> SimpleMatcherBuilder<'a> {
    /// Creates an empty [`SimpleMatcherBuilder`].
    pub fn new() -> Self {
        Self {
            word_map: HashMap::new(),
        }
    }

    /// Registers a word pattern under the given [`ProcessType`] and ID.
    ///
    /// `process_type` controls which normalization steps are applied to the input
    /// text before this pattern is evaluated. `word_id` is the identifier returned
    /// in [`SimpleResult`](crate::SimpleResult) when the pattern matches. `word`
    /// supports logical operators: `&` (both sub-patterns must appear) and `~`
    /// (the rule fires only when the following sub-pattern is absent).
    ///
    /// `process_type` may be a composite flag. For example, `ProcessType::None |
    /// ProcessType::Delete` means the rule can match against both the raw input
    /// and the delete-normalized variant. If the same `(process_type, word_id)` is
    /// inserted multiple times, the most recent `word` replaces the previous one.
    ///
    /// Returns `self` for chaining.
    pub fn add_word(mut self, process_type: ProcessType, word_id: u32, word: &'a str) -> Self {
        let bucket = self.word_map.entry(process_type).or_default();
        bucket.insert(word_id, word);
        self
    }

    /// Consumes the builder and compiles the [`SimpleMatcher`].
    ///
    /// Parsing logical operators, deduplicating sub-patterns, and building the
    /// automaton all happen here. This call is relatively expensive; do it once
    /// and reuse the resulting matcher.
    pub fn build(self) -> SimpleMatcher {
        SimpleMatcher::new(&self.word_map)
    }
}
