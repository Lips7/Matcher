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
//!
//! This uses `#[thread_local]` + `UnsafeCell` instead of the `thread_local!`
//! macro to avoid per-access closure overhead. Safety relies on two invariants:
//! (1) `#[thread_local]` guarantees single-threaded access — no data races.
//! (2) Re-entrancy is enforced at runtime by `ScanGuard` (a `Cell<bool>` TLS
//! flag that panics if a scan is already active on the current thread). This
//! converts potential aliased-`&mut` UB into a defined panic if a user callback
//! (e.g. inside `for_each_match`) calls any matcher method on the same thread.
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
//!
//! # Terminology
//!
//! | Term | Meaning |
//! |------|---------|
//! | **Rule** | A user-supplied pattern string, possibly with `&` (AND), `~` (NOT), `\|` (OR) operators. Identified by a caller-chosen `word_id`. |
//! | **Segment** | One sub-pattern within a rule, delimited by `&` or `~`. A segment may contain `\|`-separated alternatives. |
//! | **Pattern** | A deduplicated sub-pattern string stored in the AC automaton. Multiple rules may share the same pattern. |
//! | **Variant** | One transformed form of the input text (e.g., after VariantNorm, after Delete). Each variant gets a unique index. |
//! | **Generation** | A monotonic `u16` counter enabling O(1) amortized state reset between scans. Wraps every ~65K scans. |
//! | **Direct encoding** | Bit-packing a single-entry pattern's metadata into the automaton value, bypassing entry-table indirection. See `simple_matcher::pattern`. |
//!
//! For the full architectural walkthrough, see [DESIGN.md](https://github.com/search?q=repo%3Afoster_guo%2FMatcher+DESIGN.md).

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

mod builder;
pub use builder::SimpleMatcherBuilder;

mod process;
pub use process::{ProcessType, reduce_text_process, reduce_text_process_emit, text_process};

mod simple_matcher;
pub use simple_matcher::{
    MatcherError, SimpleMatcher, SimpleResult, SimpleTable, SimpleTableSerde,
};
