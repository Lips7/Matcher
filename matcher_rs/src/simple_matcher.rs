//! [`SimpleMatcher`] and [`SimpleResult`] — the public matching API.
//!
//! Prefer constructing via [`crate::SimpleMatcherBuilder`]. The type aliases
//! [`SimpleTable`] and [`SimpleTableSerde`] describe the rule-map format accepted
//! by [`SimpleMatcher::new`].

use std::borrow::Cow;

use serde::Serialize;

use crate::process::{ProcessTypeBitNode, return_processed_string_to_pool, walk_process_tree};

mod construction;
mod scan;
mod types;

use types::{AsciiMatcher, NonAsciiMatcher, PatternEntry, RuleCold, RuleHot};
pub use types::{SimpleTable, SimpleTableSerde};

/// A single match returned by [`SimpleMatcher::process`].
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 42, "hello")
///     .build();
///
/// let results = matcher.process("say hello");
/// assert_eq!(results[0].word_id, 42);
/// assert_eq!(results[0].word, "hello");
/// ```
#[derive(Serialize, Debug)]
pub struct SimpleResult<'a> {
    /// Caller-assigned identifier from the input [`SimpleTable`].
    pub word_id: u32,
    /// The original pattern string, borrowed from the compiled rule.
    pub word: Cow<'a, str>,
}

/// Multi-pattern matcher with logical operators and text normalization.
///
/// Prefer constructing via [`crate::SimpleMatcherBuilder`] rather than calling [`new`](Self::new) directly.
///
/// ## Pattern Syntax
///
/// Each pattern string may contain two special operators:
///
/// | Operator | Meaning |
/// |----------|---------|
/// | `&` | All adjacent sub-patterns must appear (order-independent AND) |
/// | `~` | The following sub-pattern must be **absent** (NOT) |
///
/// ```text
/// "apple&pie"      -- fires only when both "apple" and "pie" appear
/// "banana~peel"    -- fires when "banana" appears but "peel" does not
/// "a&b~c"          -- fires when both "a" and "b" appear and "c" does not
/// "a&a~b~b"        -- fires when "a" appears twice and "b" appears fewer than twice
/// ```
///
/// ## Two-Pass Matching
///
/// **Pass 1 — Transform and Scan**: The input text is transformed through the configured
/// [`crate::ProcessType`] pipelines, producing the distinct text variants needed for this
/// matcher. Those variants are scanned one by one. Each variant first goes through the
/// ASCII engine, then through the charwise engine when the variant is not pure ASCII.
/// Hits update per-rule state; simple rules stay on a bitmask fast path, while more complex
/// rules fall back to a per-rule counter matrix.
///
/// **Pass 2 — Evaluate**: Touched rules are checked: a rule fires if every AND
/// sub-pattern was satisfied in at least one text variant and no NOT sub-pattern was
/// triggered in any variant.
///
/// Composite process types can match across variants. For example,
/// `ProcessType::None | ProcessType::PinYin` lets one sub-pattern match the raw text and
/// another match the Pinyin-transformed variant during the same search. NOT segments are
/// global across those variants: if a veto pattern appears in any variant, the rule fails.
///
/// ## Thread Safety
///
/// `SimpleMatcher` is `Send + Sync`. All mutable scan state is stored in thread-local
/// `SimpleMatchState` instances, so concurrent calls from different threads are
/// independent with no contention.
///
/// ## Examples
///
/// ```rust
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 1, "apple&pie")
///     .add_word(ProcessType::None, 2, "banana~peel")
///     .build();
///
/// assert!(matcher.is_match("I like apple and pie"));
/// assert!(!matcher.is_match("I like banana peel"));
///
/// let results = matcher.process("apple and pie");
/// assert_eq!(results.len(), 1);
/// assert_eq!(results[0].word_id, 1);
/// ```
#[derive(Clone)]
pub struct SimpleMatcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    ascii_matcher: Option<AsciiMatcher>,
    non_ascii_matcher: Option<NonAsciiMatcher>,
    single_pt_index: Option<u8>,
    ac_dedup_entries: Vec<PatternEntry>,
    ac_dedup_ranges: Vec<(usize, usize)>,
    rule_hot: Vec<RuleHot>,
    rule_cold: Vec<RuleCold>,
    /// `true` when the tree has only the root node (ProcessType::None only) and every
    /// pattern entry is `PatternKind::Simple`. Enables a zero-overhead `is_match` fast path.
    all_simple: bool,
}

impl SimpleMatcher {
    /// Returns `true` if `text` satisfies at least one registered pattern.
    ///
    /// Equivalent to `!self.process(text).is_empty()`, but it can stop scanning as soon as
    /// one rule is confirmed.
    /// Returns `false` immediately for empty input.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "hello")
    ///     .add_word(ProcessType::None, 2, "world")
    ///     .build();
    ///
    /// assert!(matcher.is_match("hello there"));
    /// assert!(matcher.is_match("beautiful world"));
    /// assert!(!matcher.is_match("hi planet!"));
    /// ```
    pub fn is_match(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        if self.all_simple {
            return self.is_match_simple(text);
        }
        if self.single_pt_index.is_some() {
            self.is_match_inner::<true>(text)
        } else {
            self.is_match_inner::<false>(text)
        }
    }

    /// Returns all patterns that match `text`.
    ///
    /// Unlike [`is_match`](Self::is_match), this always completes the full two-pass scan
    /// and collects every satisfied rule. Returns an empty `Vec` for empty input.
    /// Results are appended in the matcher's discovery order. That order is deterministic
    /// for one constructed matcher, but it is not a public sorting guarantee.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "apple")
    ///     .add_word(ProcessType::None, 2, "banana")
    ///     .build();
    ///
    /// let results = matcher.process("I have an apple and a banana");
    /// assert_eq!(results.len(), 2);
    /// ```
    pub fn process<'a>(&'a self, text: &'a str) -> Vec<SimpleResult<'a>> {
        let mut results = Vec::new();
        self.process_into(text, &mut results);
        results
    }

    /// Appends all patterns that match `text` to `results`.
    ///
    /// Like [`process`](Self::process) but reuses a caller-supplied buffer, avoiding a
    /// `Vec` allocation per call. Useful in high-throughput loops where the caller can
    /// clear and reuse the same `Vec` across iterations.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType, SimpleResult};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "apple")
    ///     .build();
    ///
    /// let mut results: Vec<SimpleResult> = Vec::new();
    /// matcher.process_into("I have an apple", &mut results);
    /// assert_eq!(results.len(), 1);
    /// ```
    pub fn process_into<'a>(&'a self, text: &'a str, results: &mut Vec<SimpleResult<'a>>) {
        if text.is_empty() {
            return;
        }
        if self.all_simple {
            return self.process_simple(text, results);
        }
        let (processed, _) =
            walk_process_tree::<false, _>(&self.process_type_tree, text, &mut |_, _, _, _| false);
        self.process_preprocessed_into(&processed, results);
        return_processed_string_to_pool(processed);
    }
}
