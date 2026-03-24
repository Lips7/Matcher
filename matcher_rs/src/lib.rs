#![cfg_attr(
    not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")),
    feature(portable_simd)
)]
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
//!   into bytewise and charwise matcher engines. Search walks the needed transform tree once,
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
//!     .build();
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
//! # Feature Flags
//!
//! | Flag | Default | Effect |
//! |------|---------|--------|
//! | `dfa` | on | Enables `aho-corasick` DFA mode in the places where this crate chooses it; other paths still use `daachorse`-backed matchers |
//! | `simd_runtime_dispatch` | on | Selects the best available transform kernel at runtime (`AVX2` on x86-64, `NEON` on ARM64, portable fallback elsewhere) |
//! | `runtime_build` | off | Parses the source transform maps at runtime instead of loading build-time artifacts lazily on first use |

/// Use mimalloc as the global allocator for reduced fragmentation and better multi-threaded throughput.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod builder;
pub use builder::SimpleMatcherBuilder;

mod process;
pub use process::process_matcher::{reduce_text_process, reduce_text_process_emit, text_process};
pub use process::{
    ProcessType, ProcessedTextMasks, TextVariant, build_process_type_tree, walk_process_tree,
};

mod simple_matcher;
pub use simple_matcher::{SimpleMatcher, SimpleResult, SimpleTable, SimpleTableSerde};
