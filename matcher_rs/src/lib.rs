#![feature(thread_local)]
#![cfg_attr(
    not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")),
    feature(portable_simd)
)]
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(clippy::undocumented_unsafe_blocks)]
//! High-performance multi-pattern text matcher with logical operators and transformation pipelines.
//!
//! `matcher_rs` is designed for rule matching tasks where plain substring search is too rigid.
//! A rule can combine multiple sub-patterns, veto on other sub-patterns, and match against
//! raw text, transformed text, or both.
//!
//! The crate is built around three ideas:
//!
//! - **Logical operators** — Rules can require co-occurrence of sub-patterns (`&`) or
//!   veto a match when a sub-pattern is present (`~`).
//! - **Transformation pipelines** — Input can be matched after Traditional→Simplified
//!   Chinese conversion ([`ProcessType::Fanjian`]), deletion of configured codepoints
//!   ([`ProcessType::Delete`]), replacement-table normalization ([`ProcessType::Normalize`]),
//!   and Pinyin transliteration ([`ProcessType::PinYin`] / [`ProcessType::PinYinChar`]).
//! - **Two-pass evaluation** — Construction deduplicates emitted patterns and partitions them
//!   into ASCII and charwise matcher engines. Search walks the needed transform tree once,
//!   scans each produced text variant, then evaluates only touched rules.
//!
//! # Quick Start
//!
//! ```rust
//! use matcher_rs::{SimpleMatcherBuilder, ProcessType};
//!
//! let matcher = SimpleMatcherBuilder::new()
//!     .add_word(ProcessType::None, 1, "hello")
//!     // Matches after converting Traditional Chinese and removing noise chars
//!     .add_word(ProcessType::FanjianDeleteNormalize, 2, "你好")
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
//! Composite [`ProcessType`] values can also include [`ProcessType::None`] to match
//! against both the raw text and a transformed variant. For example, a rule with
//! `ProcessType::None | ProcessType::PinYin` can satisfy one sub-pattern directly from
//! the input and another via Pinyin transliteration during the same search.
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
//! | `STRING_POOL` | `process/variant.rs` |
//!
//! These use `#[thread_local]` + `UnsafeCell` instead of the `thread_local!` macro
//! to avoid per-access closure overhead. Safety relies on two invariants:
//! (1) `#[thread_local]` guarantees single-threaded access — no data races.
//! (2) No public function is re-entrant: the borrow from `UnsafeCell::get()` is
//! always dropped before any call that could re-enter the same pool.
//!
//! ## Bounds-elided indexing
//!
//! Hot loops use `get_unchecked` / `get_unchecked_mut` to avoid repeated bounds
//! checks on indices that are structurally guaranteed in-bounds by construction
//! (e.g. automaton values, rule indices). Every such site is guarded by a
//! `debug_assert!` that validates the index in debug builds.
//!
//! # Feature Flags
//!
//! | Flag | Default | Effect |
//! |------|---------|--------|
//! | `dfa` | on | Enables `aho-corasick` DFA mode in the places where this crate chooses it; other paths still use `daachorse`-backed matchers |
//! | `simd_runtime_dispatch` | on | Selects the best available transform kernel at runtime (`AVX2` on x86-64, `NEON` on ARM64, portable fallback elsewhere) |
//! | `runtime_build` | off | Parses the source transform maps at runtime instead of loading build-time artifacts lazily on first use |

/// Uses [`mimalloc`](https://github.com/purpleprotocol/mimalloc_rust) as the global allocator.
///
/// `mimalloc` was chosen because `SimpleMatcher` scanning relies heavily on thread-local
/// buffer pools and short-lived allocations during text transformation. `mimalloc`
/// provides lower fragmentation under these allocation patterns and significantly better
/// multi-threaded throughput compared to the system allocator, especially on workloads
/// where many threads match concurrently.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

use std::fmt;

/// Error returned when [`SimpleMatcher`] construction fails.
///
/// This is an opaque struct (not an enum) to avoid coupling the public API to
/// third-party error types and to allow adding new error variants in the future
/// without breaking callers. The human-readable message is available via the
/// [`Display`](fmt::Display) implementation.
///
/// # When does construction fail?
///
/// - **Invalid [`ProcessType`] bits** — the caller passed a bitflag value with
///   undefined bits (bits 6–7) set.
/// - **Automaton build failure** — the underlying Aho-Corasick libraries
///   (`daachorse` or `aho-corasick`) rejected the compiled pattern set
///   (e.g., the pattern set exceeded internal capacity limits).
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{SimpleMatcher, SimpleTable, ProcessType};
/// use std::collections::HashMap;
///
/// // Construction can be checked with standard Result handling.
/// let empty: SimpleTable = HashMap::new();
/// match SimpleMatcher::new(&empty) {
///     Ok(matcher) => assert!(!matcher.is_match("anything")),
///     Err(e) => panic!("unexpected error: {e}"),
/// }
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct MatcherError {
    message: String,
}

/// Formats the error as a human-readable message describing what went wrong
/// during [`SimpleMatcher`] construction.
impl fmt::Display for MatcherError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Enables use with `?` and the standard error-handling ecosystem.
impl std::error::Error for MatcherError {}

impl MatcherError {
    /// Wraps a third-party automaton build error (from `daachorse` or
    /// `aho-corasick`) into a [`MatcherError`].
    ///
    /// The `source` message is prefixed with `"automaton build failed: "`.
    fn automaton_build(source: impl fmt::Display) -> Self {
        Self {
            message: format!("automaton build failed: {source}"),
        }
    }

    /// Creates an error for a [`ProcessType`] value with undefined bits set.
    ///
    /// Bits 6–7 are reserved; passing them to [`SimpleMatcher::new`] would
    /// cause out-of-bounds indexing in downstream lookup tables sized for the
    /// 6-bit flag space.
    pub(crate) fn invalid_process_type(bits: u8) -> Self {
        Self {
            message: format!(
                "invalid ProcessType bits: {bits:#04x} (only bits 0–5 are defined; \
                 bits 6–7 must be zero)"
            ),
        }
    }
}

mod builder;
pub use builder::SimpleMatcherBuilder;

mod process;
pub use process::{ProcessType, reduce_text_process, reduce_text_process_emit, text_process};

mod simple_matcher;
pub use simple_matcher::{SimpleMatcher, SimpleResult, SimpleTable, SimpleTableSerde};
