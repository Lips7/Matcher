//! Fluent builder API for constructing a [`crate::SimpleMatcher`].
//!
//! [`SimpleMatcherBuilder`] accumulates patterns grouped by [`crate::ProcessType`] pipeline
//! and compiles them into an optimized automaton in one shot via [`SimpleMatcherBuilder::build`].

use std::borrow::Cow;
use std::collections::HashMap;

use crate::{MatcherError, ProcessType, SimpleMatcher};

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
///     .build()
///     .unwrap();
///
/// assert!(matcher.is_match("hello world"));
/// ```
///
/// Owned strings work without keeping the originals alive:
///
/// ```rust
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 1, String::from("hello"))
///     .build()
///     .unwrap();
///
/// assert!(matcher.is_match("hello world"));
/// ```
#[must_use]
#[derive(Default)]
pub struct SimpleMatcherBuilder<'a> {
    word_map: HashMap<ProcessType, HashMap<u32, Cow<'a, str>>>,
}

/// Builder operations for accumulating and compiling rules.
impl<'a> SimpleMatcherBuilder<'a> {
    /// Creates an empty [`SimpleMatcherBuilder`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::SimpleMatcherBuilder;
    ///
    /// let builder = SimpleMatcherBuilder::new();
    /// // The builder starts with no registered patterns.
    /// let matcher = builder.build().unwrap();
    /// assert!(!matcher.is_match("anything"));
    /// ```
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
    /// supports logical operators:
    ///
    /// - `&` — AND: both adjacent sub-patterns must appear (order-independent).
    /// - `~` — NOT: the following sub-pattern must be absent for the rule to fire.
    ///
    /// `process_type` may be a composite flag. For example, `ProcessType::None |
    /// ProcessType::Delete` means the rule can match against both the raw input
    /// and the delete-normalized variant. If the same `(process_type, word_id)` is
    /// inserted multiple times, the most recent `word` replaces the previous one.
    ///
    /// Returns `self` for chaining.
    ///
    /// # Examples
    ///
    /// Logical operators:
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
    ///
    /// // AND: both "apple" and "pie" must appear
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "apple&pie")
    ///     .build()
    ///     .unwrap();
    ///
    /// assert!(matcher.is_match("apple and pie"));
    /// assert!(!matcher.is_match("apple only"));
    ///
    /// // NOT: "banana" must appear, "peel" must be absent
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "banana~peel")
    ///     .build()
    ///     .unwrap();
    ///
    /// assert!(matcher.is_match("banana split"));
    /// assert!(!matcher.is_match("banana peel"));
    ///
    /// // Combined: "fox" AND "jump" present, "lazy" absent
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "fox&jump~lazy")
    ///     .build()
    ///     .unwrap();
    ///
    /// assert!(matcher.is_match("the fox can jump"));
    /// assert!(!matcher.is_match("the lazy fox can jump"));
    /// ```
    ///
    /// Composite [`ProcessType`] for matching across raw and transformed text:
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     // Match against both raw input and Fanjian-converted text
    ///     .add_word(ProcessType::None | ProcessType::Fanjian, 1, "你好")
    ///     // Match after deleting noise characters and normalizing
    ///     .add_word(ProcessType::FanjianDeleteNormalize, 2, "测试")
    ///     .build()
    ///     .unwrap();
    ///
    /// // Raw "你好" matches via the ProcessType::None path
    /// assert!(matcher.is_match("你好世界"));
    /// // Traditional "妳好" matches via the ProcessType::Fanjian path
    /// assert!(matcher.is_match("妳好世界"));
    /// ```
    #[must_use = "builder methods return a new builder; dropping it discards the added word"]
    pub fn add_word(
        mut self,
        process_type: ProcessType,
        word_id: u32,
        word: impl Into<Cow<'a, str>>,
    ) -> Self {
        let bucket = self.word_map.entry(process_type).or_default();
        bucket.insert(word_id, word.into());
        self
    }

    /// Consumes the builder and compiles the [`SimpleMatcher`].
    ///
    /// Parsing logical operators, deduplicating sub-patterns, and building the
    /// Aho-Corasick automaton all happen here. This is the most expensive call in
    /// the API — it should be done once at startup, and the resulting
    /// [`SimpleMatcher`] reused for the lifetime of the application.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "hello")
    ///     .build()
    ///     .unwrap();
    ///
    /// // Reuse the matcher across many searches.
    /// assert!(matcher.is_match("hello world"));
    /// assert!(!matcher.is_match("goodbye"));
    /// ```
    pub fn build(self) -> Result<SimpleMatcher, MatcherError> {
        SimpleMatcher::new(&self.word_map)
    }
}
