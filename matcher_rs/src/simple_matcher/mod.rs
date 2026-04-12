//! [`SimpleMatcher`] and [`SimpleResult`] — the public matching API.
//!
//! Prefer constructing via [`crate::SimpleMatcherBuilder`]. The type aliases
//! [`SimpleTable`] and [`SimpleTableSerde`] describe the raw rule-map format
//! accepted by [`SimpleMatcher::new`] for advanced use cases (e.g.
//! deserialization from JSON).
//!
//! # Module Layout
//!
//! The implementation is split across private child modules:
//!
//! - `build` — [`SimpleMatcher::new`] and rule parsing / deduplication.
//! - `scan` — Aho-Corasick automaton compilation (bytewise and charwise
//!   engines) and density-based dispatch.
//! - `pattern` — Deduplicated pattern storage, entry types, and dispatch.
//! - `rule` — Rule metadata (`Rule`/`RuleSet`) and state machine.
//! - `search` — Hot-path scan loops and rule evaluation.
//! - `state` — Thread-local scan state (`SimpleMatchState`, `ScanContext`).
//! - `tree` — Process-type trie construction for transform prefix sharing.

use std::{borrow::Cow, fmt};

#[cfg(feature = "rayon")]
use rayon::prelude::*;

mod build;
mod pattern;
mod rule;
mod scan;
mod search;
mod state;
pub(crate) mod tree;

use rule::RuleSet;
pub use rule::{SimpleTable, SimpleTableSerde};
use scan::ScanPlan;
use tree::ProcessTypeBitNode;

/// A single match returned by [`SimpleMatcher::process`] or
/// [`SimpleMatcher::process_into`].
///
/// The lifetime `'a` is tied to the [`SimpleMatcher`] that produced this
/// result. The `word` field borrows directly from the matcher's internal rule
/// storage, so no allocation occurs when collecting results.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, SimpleMatcherBuilder};
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
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SimpleResult<'a> {
    /// The caller-assigned identifier for the matched rule, as passed to
    /// [`SimpleMatcherBuilder::add_word`](crate::SimpleMatcherBuilder::add_word) or
    /// the raw [`SimpleTable`] map.
    pub word_id: u32,
    /// The original pattern string for the matched rule.
    ///
    /// This is a [`Cow::Borrowed`] reference into the matcher's internal
    /// storage, so it is cheap to produce. The lifetime `'a` is the
    /// lifetime of the [`SimpleMatcher`] that generated this result.
    pub word: Cow<'a, str>,
}

/// Multi-pattern matcher with logical operators and text normalization.
///
/// Prefer constructing via [`crate::SimpleMatcherBuilder`] rather than calling
/// [`new`](Self::new) directly.
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
/// **Pass 1 — Transform and Scan**: The input text is transformed through the
/// configured [`ProcessType`](crate::ProcessType) pipelines, producing the
/// distinct text variants needed for this matcher. Each variant is scanned by
/// the bytewise or charwise engine, selected by SIMD density scan (≤0.67
/// non-ASCII → bytewise, >0.67 → charwise). Hits update per-rule state;
/// simple rules stay on a bitmask fast path, while more complex rules fall
/// back to a per-rule counter matrix.
///
/// **Pass 2 — Evaluate**: Touched rules are checked: a rule fires if every AND
/// sub-pattern was satisfied in at least one text variant and no NOT
/// sub-pattern was triggered in any variant.
///
/// Composite process types can match across variants. For example,
/// `ProcessType::None | ProcessType::Romanize` lets one sub-pattern match the
/// raw text and another match the Romanize-transformed variant during the same
/// search. NOT segments are global across those variants: if a veto pattern
/// appears in any variant, the rule fails.
///
/// # Thread Safety
///
/// `SimpleMatcher` is [`Send`] + [`Sync`]. All mutable scan state is stored in
/// thread-local `SimpleMatchState` instances (one per thread), so concurrent
/// calls from different threads are fully independent with no contention or
/// locking. The matcher itself is immutable after construction.
///
/// # Performance
///
/// - **O(N) text scan**: All unique sub-patterns across all rules are
///   deduplicated into a single Aho-Corasick automaton, so scan time scales
///   with text length, not rule count.
/// - **O(1) state reset**: Generation-based sparse-set avoids clearing per-rule
///   state between calls (only touched rules are cleaned up).
/// - **Bitmask fast path**: Rules with ≤64 segments use a `u64` bitmask instead
///   of the full matrix, keeping the inner loop branch-free.
/// - **DAG reuse**: The transformation pipeline is structured as a trie so
///   intermediate results (e.g., Delete output) are computed once even when
///   multiple composite `ProcessType`s share a prefix.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, SimpleMatcherBuilder};
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
    tree: Vec<ProcessTypeBitNode>,
    scan: ScanPlan,
    rules: RuleSet,
    /// `true` when no text transforms are needed and every pattern is a
    /// simple single-segment literal. Enables `is_match` to delegate
    /// directly to the AC automaton without TLS state setup.
    is_match_fast: bool,
}

/// Formats as `SimpleMatcher { rule_count: …, .. }`.
///
/// Internal engine details (automaton sizes, pattern indices) are omitted to
/// keep the output concise and stable across versions.
impl fmt::Debug for SimpleMatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SimpleMatcher")
            .field("rule_count", &self.rules.len())
            .finish_non_exhaustive()
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
    /// use matcher_rs::{ProcessType, SimpleMatcherBuilder};
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
        // Fast path: no text transforms and all rules are simple literals —
        // delegate directly to the AC automaton without TLS state setup.
        if self.is_match_fast {
            return self.scan.is_match(text);
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
    /// use matcher_rs::{ProcessType, SimpleMatcherBuilder};
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
    /// This is the allocation-friendly variant of [`process`](Self::process).
    /// The caller retains ownership of `results` and can reuse it across
    /// many searches by calling [`Vec::clear`] between batches, avoiding
    /// repeated heap allocation for the output vector itself.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{ProcessType, SimpleMatcherBuilder, SimpleResult};
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
        self.walk_and_scan(text, false, Some(results));
    }

    /// Calls `on_match` for each matched rule, stopping early if the callback
    /// returns `true`.
    ///
    /// Returns `true` if the callback requested early exit.
    ///
    /// This is the zero-allocation alternative to [`process`](Self::process):
    /// no `Vec` is allocated for the results. All variants are scanned first
    /// (to resolve AND/NOT logic), then the callback fires for each satisfied
    /// rule.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{ProcessType, SimpleMatcherBuilder};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "hello")
    ///     .add_word(ProcessType::None, 2, "world")
    ///     .build()
    ///     .unwrap();
    ///
    /// // Collect the first match only.
    /// let mut first = None;
    /// matcher.for_each_match("hello world", |r| {
    ///     first = Some(r.word_id);
    ///     true // stop after first
    /// });
    /// assert!(first.is_some());
    /// ```
    pub fn for_each_match<'a>(
        &'a self,
        text: &'a str,
        on_match: impl FnMut(SimpleResult<'a>) -> bool,
    ) -> bool {
        if text.is_empty() {
            return false;
        }
        self.walk_and_scan_with(text, false, |rules, ss| {
            rules.for_each_satisfied(ss, on_match)
        })
        .1
        .unwrap_or(false)
    }

    /// Returns the first matching rule, or `None` if no rule matches.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{ProcessType, SimpleMatcherBuilder};
    ///
    /// let matcher = SimpleMatcherBuilder::new()
    ///     .add_word(ProcessType::None, 1, "hello")
    ///     .build()
    ///     .unwrap();
    ///
    /// assert_eq!(matcher.find_match("hello world").unwrap().word_id, 1);
    /// assert!(matcher.find_match("goodbye").is_none());
    /// ```
    #[must_use]
    pub fn find_match<'a>(&'a self, text: &'a str) -> Option<SimpleResult<'a>> {
        let mut found = None;
        self.for_each_match(text, |r| {
            found = Some(r);
            true
        });
        found
    }

    /// Returns the estimated heap memory in bytes owned by this matcher.
    ///
    /// Includes the AC automata, rule metadata, and the process-type
    /// tree. Does **not** include thread-local scan state or global transform
    /// caches (those are shared infrastructure, not per-matcher).
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.tree.capacity() * size_of::<ProcessTypeBitNode>()
            + self.tree.iter().map(|n| n.heap_bytes()).sum::<usize>()
            + self.scan.heap_bytes()
            + self.rules.heap_bytes()
    }
}

/// Parallel batch methods powered by [rayon](https://docs.rs/rayon).
///
/// These methods distribute work across all available CPU cores via rayon's
/// work-stealing scheduler. Each text is matched independently — the shared
/// `&self` is read-only, and all mutable state lives in per-thread
/// thread-local storage.
///
/// For small batches (<16 texts), the overhead of rayon scheduling may
/// outweigh the parallelism benefit. Use the single-text methods instead.
#[cfg(feature = "rayon")]
impl SimpleMatcher {
    /// Matches each text in parallel, returning a boolean per text.
    ///
    /// Equivalent to calling [`is_match`](Self::is_match) on each text, but
    /// distributed across CPU cores. The output order matches the input order.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::collections::HashMap;
    ///
    /// use matcher_rs::{ProcessType, SimpleMatcher, SimpleTable};
    ///
    /// let mut table: SimpleTable = HashMap::new();
    /// table
    ///     .entry(ProcessType::None)
    ///     .or_default()
    ///     .insert(1, "hello");
    ///
    /// let matcher = SimpleMatcher::new(&table).unwrap();
    /// let results = matcher.batch_is_match(&["hello world", "goodbye", "say hello"]);
    /// assert_eq!(results, vec![true, false, true]);
    /// ```
    #[must_use]
    pub fn batch_is_match(&self, texts: &[&str]) -> Vec<bool> {
        texts.par_iter().map(|text| self.is_match(text)).collect()
    }

    /// Collects all matching rules for each text in parallel.
    ///
    /// Equivalent to calling [`process`](Self::process) on each text, but
    /// distributed across CPU cores. The output order matches the input order.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::collections::HashMap;
    ///
    /// use matcher_rs::{ProcessType, SimpleMatcher, SimpleTable};
    ///
    /// let mut table: SimpleTable = HashMap::new();
    /// table
    ///     .entry(ProcessType::None)
    ///     .or_default()
    ///     .insert(1, "hello");
    ///
    /// let matcher = SimpleMatcher::new(&table).unwrap();
    /// let results = matcher.batch_process(&["hello world", "goodbye"]);
    /// assert_eq!(results[0].len(), 1);
    /// assert_eq!(results[0][0].word_id, 1);
    /// assert!(results[1].is_empty());
    /// ```
    #[must_use]
    pub fn batch_process<'a>(&'a self, texts: &[&'a str]) -> Vec<Vec<SimpleResult<'a>>> {
        texts.par_iter().map(|text| self.process(text)).collect()
    }

    /// Finds the first matching rule for each text in parallel.
    ///
    /// Equivalent to calling [`find_match`](Self::find_match) on each text,
    /// but distributed across CPU cores. The output order matches the input
    /// order.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::collections::HashMap;
    ///
    /// use matcher_rs::{ProcessType, SimpleMatcher, SimpleTable};
    ///
    /// let mut table: SimpleTable = HashMap::new();
    /// table
    ///     .entry(ProcessType::None)
    ///     .or_default()
    ///     .insert(1, "hello");
    ///
    /// let matcher = SimpleMatcher::new(&table).unwrap();
    /// let results = matcher.batch_find_match(&["hello world", "goodbye"]);
    /// assert_eq!(results[0].as_ref().unwrap().word_id, 1);
    /// assert!(results[1].is_none());
    /// ```
    #[must_use]
    pub fn batch_find_match<'a>(&'a self, texts: &[&'a str]) -> Vec<Option<SimpleResult<'a>>> {
        texts.par_iter().map(|text| self.find_match(text)).collect()
    }
}
