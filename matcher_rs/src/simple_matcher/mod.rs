//! [`SimpleMatcher`] and [`SimpleResult`] — the public matching API.
//!
//! Prefer constructing via [`crate::SimpleMatcherBuilder`]. The type aliases
//! [`SimpleTable`] and [`SimpleTableSerde`] describe the rule-map format accepted
//! by [`SimpleMatcher::new`].

use std::borrow::Cow;

use serde::Serialize;

use crate::process::{ProcessTypeBitNode, return_processed_string_to_pool, walk_process_tree};

mod build;
mod engine;
mod rule;
mod search;
mod state;

use engine::ScanPlan;
use rule::RuleSet;
pub use rule::{SimpleTable, SimpleTableSerde};

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
    pub word_id: u32,
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
    process: ProcessPlan,
    scan: ScanPlan,
    rules: RuleSet,
}

#[derive(Clone)]
pub(super) struct ProcessPlan {
    tree: Vec<ProcessTypeBitNode>,
    mode: SearchMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchMode {
    AllSimple,
    SingleProcessType { pt_index: u8 },
    General,
}

impl ProcessPlan {
    #[inline(always)]
    pub(super) fn new(tree: Vec<ProcessTypeBitNode>, mode: SearchMode) -> Self {
        Self { tree, mode }
    }

    #[inline(always)]
    pub(super) fn tree(&self) -> &[ProcessTypeBitNode] {
        &self.tree
    }

    #[inline(always)]
    pub(super) fn mode(&self) -> SearchMode {
        self.mode
    }

    #[inline(always)]
    pub(super) fn is_all_simple(&self) -> bool {
        matches!(self.mode, SearchMode::AllSimple)
    }
}

impl SearchMode {
    #[inline(always)]
    pub(super) fn single_pt_index(self) -> Option<u8> {
        match self {
            Self::SingleProcessType { pt_index } => Some(pt_index),
            Self::AllSimple | Self::General => None,
        }
    }
}

impl SimpleMatcher {
    /// Returns `true` if `text` satisfies at least one registered pattern.
    pub fn is_match(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        if self.process.is_all_simple() {
            return self.is_match_simple(text);
        }
        if self.process.mode().single_pt_index().is_some() {
            self.is_match_inner::<true>(text)
        } else {
            self.is_match_inner::<false>(text)
        }
    }

    /// Returns all patterns that match `text`.
    pub fn process<'a>(&'a self, text: &'a str) -> Vec<SimpleResult<'a>> {
        let mut results = Vec::new();
        self.process_into(text, &mut results);
        results
    }

    /// Appends all patterns that match `text` to `results`.
    pub fn process_into<'a>(&'a self, text: &'a str, results: &mut Vec<SimpleResult<'a>>) {
        if text.is_empty() {
            return;
        }
        if self.process.is_all_simple() {
            return self.process_simple(text, results);
        }
        let (processed, _) =
            walk_process_tree::<false, _>(self.process.tree(), text, &mut |_, _, _, _| false);
        self.process_preprocessed_into(&processed, results);
        return_processed_string_to_pool(processed);
    }
}
