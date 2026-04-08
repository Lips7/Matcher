#![feature(thread_local)]
#![cfg_attr(doc, feature(doc_cfg))]
#![feature(optimize_attribute)]
#![cfg_attr(
    not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")),
    feature(portable_simd)
)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]
#![warn(clippy::undocumented_unsafe_blocks)]
//! High-performance multi-pattern text matcher with logical operators and
//! transformation pipelines.
//!
//! `matcher_rs` is designed for rule matching tasks where plain substring
//! search is too rigid. A rule can combine multiple sub-patterns, veto on other
//! sub-patterns, and match against raw text, transformed text, or both.
//!
//! The crate is built around three ideas:
//!
//! - **Logical operators** — Rules can require co-occurrence of sub-patterns
//!   (`&`) or veto a match when a sub-pattern is present (`~`).
//! - **Transformation pipelines** — Input can be matched after
//!   Traditional→Simplified CJK variant normalization
//!   ([`ProcessType::VariantNorm`]), deletion of configured codepoints
//!   ([`ProcessType::Delete`]), replacement-table normalization
//!   ([`ProcessType::Normalize`]), and CJK romanization
//!   ([`ProcessType::Romanize`] / [`ProcessType::RomanizeChar`]).
//! - **Two-pass evaluation** — Construction deduplicates emitted patterns and
//!   partitions them into ASCII and charwise matcher engines. Search walks the
//!   needed transform tree once, scans each produced text variant, then
//!   evaluates only touched rules.
//!
//! # Quick Start
//!
//! ```rust
//! use matcher_rs::{ProcessType, SimpleMatcherBuilder};
//!
//! let matcher = SimpleMatcherBuilder::new()
//!     .add_word(ProcessType::None, 1, "hello")
//!     // Matches after converting Traditional Chinese and removing noise chars
//!     .add_word(ProcessType::VariantNormDeleteNormalize, 2, "你好")
//!     // Both sub-patterns must appear in the text
//!     .add_word(ProcessType::None, 3, "apple&pie")
//!     // "banana" matches only when "peel" is absent
//!     .add_word(ProcessType::None, 4, "banana~peel")
//!     .build()
//!     .unwrap();
//!
//! assert!(matcher.is_match("hello world"));
//! assert!(matcher.is_match("apple and pie"));
//! assert!(!matcher.is_match("banana peel")); // vetoed by ~peel
//!
//! let results = matcher.process("hello world");
//! assert_eq!(results[0].word_id, 1);
//! ```
//!
//! Composite [`ProcessType`] values can also include [`ProcessType::None`] to
//! match against both the raw text and a transformed variant. For example, a
//! rule with `ProcessType::None | ProcessType::Romanize` can satisfy one
//! sub-pattern directly from the input and another via CJK romanization during
//! the same search.
//!
//! # Safety
//!
//! This crate uses `unsafe` in three categories:
//!
//! ## Thread-local state via `#[thread_local]` + `UnsafeCell`
//!
//! | Static | Location |
//! |--------|----------|
//! | `SIMPLE_MATCH_STATE` | `simple_matcher/state.rs` |
//! | `STRING_POOL` | `process/string_pool.rs` |
//!
//! These use `#[thread_local]` + `UnsafeCell` instead of the `thread_local!`
//! macro to avoid per-access closure overhead. Safety relies on two invariants:
//! (1) `#[thread_local]` guarantees single-threaded access — no data races.
//! (2) No public function is re-entrant: the borrow from `UnsafeCell::get()` is
//! always dropped before any call that could re-enter the same pool.
//!
//! ## Bounds-elided indexing
//!
//! Hot loops use `get_unchecked` / `get_unchecked_mut` to avoid repeated bounds
//! checks on indices that are structurally guaranteed in-bounds by construction
//! (e.g. automaton values, rule indices). Every such site communicates the
//! invariant to the optimizer via [`core::hint::assert_unchecked`].
//!
//! # Feature Flags
//!
//! | Flag | Default | Effect |
//! |------|---------|--------|
//! | `perf` | on | Meta-feature enabling `dfa + simd_runtime_dispatch` |
//! | `dfa` | via `perf` | Enables `aho-corasick` DFA mode in the places where this crate chooses it; other paths still use `daachorse`-backed matchers |
//! | `simd_runtime_dispatch` | via `perf` | Selects the best available transform kernel at runtime (`AVX2` on x86-64, `NEON` on ARM64, portable fallback elsewhere) |
//! | `serde` | off | Enables `Serialize`/`Deserialize` impls for [`ProcessType`] and `Serialize` for [`SimpleResult`] |

/// Uses [`mimalloc`](https://github.com/purpleprotocol/mimalloc_rust) as the global allocator.
///
/// `mimalloc` was chosen because `SimpleMatcher` scanning relies heavily on
/// thread-local buffer pools and short-lived allocations during text
/// transformation. `mimalloc` provides lower fragmentation under these
/// allocation patterns and significantly better multi-threaded throughput
/// compared to the system allocator, especially on workloads where many threads
/// match concurrently.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::fmt;

/// Error returned when [`SimpleMatcher`] construction fails.
///
/// Each variant describes a specific failure mode. The enum is
/// `#[non_exhaustive]`, so new variants may be added in future minor releases
/// without breaking callers who use a wildcard arm.
///
/// # When does construction fail?
///
/// - **Empty pattern set** — no patterns remain after parsing (all entries were
///   empty strings or pure-NOT rules).
/// - **Invalid [`ProcessType`] bits** — the caller passed a bitflag value with
///   undefined bits (bits 6–7) set.
/// - **Automaton build failure** — the underlying Aho-Corasick libraries
///   (`daachorse` or `aho-corasick`) rejected the compiled pattern set (e.g.,
///   the pattern set exceeded internal capacity limits).
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
///
/// use matcher_rs::{ProcessType, SimpleMatcher, SimpleTable};
///
/// // Empty tables are rejected.
/// let empty: SimpleTable = HashMap::new();
/// assert!(SimpleMatcher::new(&empty).is_err());
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum MatcherError {
    /// The underlying Aho-Corasick library (`daachorse` or `aho-corasick`)
    /// failed to compile the pattern set.
    AutomatonBuild {
        /// Human-readable description from the third-party builder.
        reason: String,
    },

    /// A [`ProcessType`] value contained undefined bits (bits 6–7 set).
    InvalidProcessType {
        /// The raw bitflag value that was rejected.
        bits: u8,
    },

    /// The pattern set is empty — no scannable patterns remain after parsing.
    ///
    /// This can happen when the input table is entirely empty, all pattern
    /// strings are empty, or every rule was a pure-NOT rule (which is
    /// unsatisfiable and skipped during parsing).
    EmptyPatterns,
}

impl fmt::Display for MatcherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AutomatonBuild { reason } => write!(f, "automaton build failed: {reason}"),
            Self::InvalidProcessType { bits } => write!(
                f,
                "invalid ProcessType bits: {bits:#04x} \
                 (only bits 0\u{2013}5 are defined; bits 6\u{2013}7 must be zero)"
            ),
            Self::EmptyPatterns => write!(
                f,
                "empty pattern set: at least one scannable pattern is required"
            ),
        }
    }
}

impl std::error::Error for MatcherError {}

impl MatcherError {
    /// Wraps a third-party automaton build error (from `daachorse` or
    /// `aho-corasick`) into a [`MatcherError`].
    fn automaton_build(source: impl fmt::Display) -> Self {
        Self::AutomatonBuild {
            reason: source.to_string(),
        }
    }

    /// Creates an error for a [`ProcessType`] value with undefined bits set.
    pub(crate) fn invalid_process_type(bits: u8) -> Self {
        Self::InvalidProcessType { bits }
    }
}

mod builder;
pub use builder::SimpleMatcherBuilder;

mod process;
pub use process::{ProcessType, reduce_text_process, reduce_text_process_emit, text_process};

mod simple_matcher;
pub use simple_matcher::{
    SimpleMatchIter, SimpleMatcher, SimpleResult, SimpleTable, SimpleTableSerde,
};
