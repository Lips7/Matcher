//! [`SimpleMatcher`] and [`SimpleResult`] — the public matching API.
//!
//! Prefer constructing via [`crate::SimpleMatcherBuilder`]. The type aliases
//! [`SimpleTable`] and [`SimpleTableSerde`] describe the raw rule-map format accepted
//! by [`SimpleMatcher::new`] for advanced use cases (e.g. deserialization from JSON).
//!
//! # Module Layout
//!
//! The implementation is split across private child modules:
//!
//! - `build` — [`SimpleMatcher::new`] and rule parsing / deduplication.
//! - `engine` — Aho-Corasick automaton compilation (ASCII and charwise engines).
//! - `rule` — Rule metadata, pattern dispatch, and state transitions.
//! - `search` — Hot-path scan loops and rule evaluation.
//! - `state` — Thread-local scan state (`SimpleMatchState`, `ScanContext`).

use std::borrow::Cow;
use std::fmt;

use serde::Serialize;

use crate::process::graph::ProcessTypeBitNode;

mod build;
mod engine;
#[cfg(feature = "harry")]
mod harry;
mod rule;
mod search;
mod state;

use engine::ScanPlan;
#[cfg(feature = "harry")]
pub use harry::HarryMatcher;
use rule::RuleSet;
pub use rule::{SimpleTable, SimpleTableSerde};

/// A single match returned by [`SimpleMatcher::process`] or [`SimpleMatcher::process_into`].
///
/// The lifetime `'a` is tied to the [`SimpleMatcher`] that produced this result.
/// The `word` field borrows directly from the matcher's internal rule storage, so
/// no allocation occurs when collecting results.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 42, "hello")
///     .add_word(ProcessType::None, 7, "world")
///     .build()
///     .unwrap();
///
/// let results = matcher.process("hello world");
/// assert_eq!(results.len(), 2);
///
/// // Each result carries the caller-assigned word_id and the original pattern string.
/// let ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
/// assert!(ids.contains(&42));
/// assert!(ids.contains(&7));
///
/// // word is a Cow that borrows from the matcher — no extra allocation.
/// assert!(results.iter().any(|r| r.word == "hello"));
/// ```
#[must_use]
#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct SimpleResult<'a> {
    /// The caller-assigned identifier for the matched rule, as passed to
    /// [`SimpleMatcherBuilder::add_word`](crate::SimpleMatcherBuilder::add_word) or
    /// the raw [`SimpleTable`] map.
    pub word_id: u32,
    /// The original pattern string for the matched rule.
    ///
    /// This is a [`Cow::Borrowed`] reference into the matcher's internal storage,
    /// so it is cheap to produce. The lifetime `'a` is the lifetime of the
    /// [`SimpleMatcher`] that generated this result.
    pub word: Cow<'a, str>,
}

/// Multi-pattern matcher with logical operators and text normalization.
///
/// Prefer constructing via [`crate::SimpleMatcherBuilder`] rather than calling [`new`](Self::new) directly.
///
/// # Pattern Syntax
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
/// # Two-Pass Matching
///
/// **Pass 1 — Transform and Scan**: The input text is transformed through the configured
/// [`ProcessType`](crate::ProcessType) pipelines, producing the distinct text variants
/// needed for this matcher. Those variants are scanned one by one. Each variant first goes
/// through the ASCII engine, then through the charwise engine when the variant is not pure
/// ASCII. Hits update per-rule state; simple rules stay on a bitmask fast path, while more
/// complex rules fall back to a per-rule counter matrix.
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
/// # Thread Safety
///
/// `SimpleMatcher` is [`Send`] + [`Sync`]. All mutable scan state is stored in thread-local
/// `SimpleMatchState` instances (one per thread), so concurrent calls from different
/// threads are fully independent with no contention or locking. The matcher itself is
/// immutable after construction.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
///
/// let matcher = SimpleMatcherBuilder::new()
///     .add_word(ProcessType::None, 1, "apple&pie")
///     .add_word(ProcessType::None, 2, "banana~peel")
///     .build()
///     .unwrap();
///
/// assert!(matcher.is_match("I like apple and pie"));
/// assert!(!matcher.is_match("I like banana peel"));
///
/// let results = matcher.process("apple and pie");
/// assert_eq!(results.len(), 1);
/// assert_eq!(results[0].word_id, 1);
/// assert_eq!(results[0].word, "apple&pie");
/// ```
#[must_use]
#[derive(Clone)]
pub struct SimpleMatcher {
    process: ProcessPlan,
    scan: ScanPlan,
    rules: RuleSet,
}

/// Formats as `SimpleMatcher { search_mode: …, rule_count: …, .. }`.
///
/// Internal engine details (automaton sizes, pattern indices) are omitted to
/// keep the output concise and stable across versions. The two fields shown
/// give a quick summary of the matcher's dispatch mode and rule count.
impl fmt::Debug for SimpleMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SimpleMatcher")
            .field("search_mode", &self.process.mode())
            .field("rule_count", &self.rules.len())
            .finish_non_exhaustive()
    }
}

/// Immutable process-type traversal plan cached inside a [`SimpleMatcher`].
///
/// Stores the precomputed transformation trie and the [`SearchMode`] selected at
/// construction time. The trie is walked once per query to produce all required
/// text variants before scanning.
#[derive(Clone)]
pub(super) struct ProcessPlan {
    tree: Vec<ProcessTypeBitNode>,
    mode: SearchMode,
}

/// Dispatch mode selected at construction time to unlock fast paths during scanning.
///
/// The mode is determined by analyzing the rule set: if all rules are simple
/// single-fragment literals under the same [`ProcessType`](crate::ProcessType), the
/// matcher can skip the full state machine and use direct rule dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchMode {
    /// Every rule is a simple single-fragment literal with no `&`/`~` operators
    /// and no text transformation. The matcher bypasses state tracking entirely.
    AllSimple,
    /// Rules require text transformation and/or the full state machine.
    General,
}

/// Accessors for the immutable process plan stored inside a matcher.
impl ProcessPlan {
    /// Creates a new immutable process plan.
    #[inline(always)]
    pub(super) fn new(tree: Vec<ProcessTypeBitNode>, mode: SearchMode) -> Self {
        Self { tree, mode }
    }

    /// Returns the cached process-type traversal tree.
    #[inline(always)]
    pub(super) fn tree(&self) -> &[ProcessTypeBitNode] {
        &self.tree
    }

    /// Returns the search mode selected at construction time.
    #[inline(always)]
    pub(super) fn mode(&self) -> SearchMode {
        self.mode
    }

    /// Returns the estimated heap memory in bytes owned by the process plan.
    pub(super) fn heap_bytes(&self) -> usize {
        self.tree.capacity() * size_of::<ProcessTypeBitNode>()
            + self.tree.iter().map(|n| n.heap_bytes()).sum::<usize>()
    }

    /// Returns whether the matcher can use the simplest no-state fast path.
    #[inline(always)]
    pub(super) fn is_all_simple(&self) -> bool {
        matches!(self.mode, SearchMode::AllSimple)
    }
}

/// Public query and result APIs for the compiled matcher.
impl SimpleMatcher {
    /// Returns `true` if `text` satisfies at least one registered pattern.
    ///
    /// This is the fastest query method. It uses lazy transform-tree traversal
    /// and stops as soon as any rule is satisfied, without collecting results.
    /// Returns `false` immediately when `text` is empty.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "hello")
    ///     .add_word(ProcessType::None, 2, "foo&bar")
    ///     .build()
    ///     .unwrap();
    ///
    /// assert!(matcher.is_match("hello world"));
    /// assert!(matcher.is_match("foo and bar"));
    /// assert!(!matcher.is_match("foo only"));
    /// assert!(!matcher.is_match(""));
    /// ```
    #[must_use]
    pub fn is_match(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        if self.process.is_all_simple() {
            return self.is_match_simple(text);
        }
        self.walk_and_scan(text, true, None)
    }

    /// Returns all patterns that match `text`.
    ///
    /// Results borrow the stored pattern strings inside the matcher, so the
    /// returned [`SimpleResult`] values carry the matcher's lifetime.
    /// Returns an empty `Vec` when `text` is empty or no rules fire.
    ///
    /// For repeated calls where you want to reuse the output buffer, prefer
    /// [`process_into`](Self::process_into).
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "hello")
    ///     .add_word(ProcessType::None, 2, "world")
    ///     .add_word(ProcessType::None, 3, "missing")
    ///     .build()
    ///     .unwrap();
    ///
    /// let results = matcher.process("hello world");
    /// assert_eq!(results.len(), 2);
    ///
    /// let ids: Vec<u32> = results.iter().map(|r| r.word_id).collect();
    /// assert!(ids.contains(&1));
    /// assert!(ids.contains(&2));
    /// assert!(!ids.contains(&3));
    /// ```
    #[must_use]
    pub fn process<'a>(&'a self, text: &'a str) -> Vec<SimpleResult<'a>> {
        let mut results = Vec::new();
        self.process_into(text, &mut results);
        results
    }

    /// Appends all patterns that match `text` to `results`.
    ///
    /// This is the allocation-friendly variant of [`process`](Self::process). The caller
    /// retains ownership of `results` and can reuse it across many searches by calling
    /// [`Vec::clear`] between batches, avoiding repeated heap allocation for the output
    /// vector itself.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatcherBuilder, ProcessType, SimpleResult};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "hello")
    ///     .add_word(ProcessType::None, 2, "world")
    ///     .build()
    ///     .unwrap();
    ///
    /// // Reuse the same buffer for multiple searches.
    /// let mut results: Vec<SimpleResult<'_>> = Vec::new();
    ///
    /// matcher.process_into("hello", &mut results);
    /// assert_eq!(results.len(), 1);
    /// assert_eq!(results[0].word_id, 1);
    ///
    /// results.clear();
    /// matcher.process_into("world", &mut results);
    /// assert_eq!(results.len(), 1);
    /// assert_eq!(results[0].word_id, 2);
    ///
    /// results.clear();
    /// matcher.process_into("hello world", &mut results);
    /// assert_eq!(results.len(), 2);
    /// ```
    pub fn process_into<'a>(&'a self, text: &'a str, results: &mut Vec<SimpleResult<'a>>) {
        if text.is_empty() {
            return;
        }
        if self.process.is_all_simple() {
            return self.process_simple(text, results);
        }
        self.walk_and_scan(text, false, Some(results));
    }

    /// Returns the estimated heap memory in bytes owned by this matcher.
    ///
    /// Includes the AC automata, Harry tables, rule metadata, and the process-type
    /// tree. Does **not** include thread-local scan state or global transform caches
    /// (those are shared infrastructure, not per-matcher).
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.process.heap_bytes() + self.scan.heap_bytes() + self.rules.heap_bytes()
    }
}
