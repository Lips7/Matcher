#![cfg_attr(
    not(all(feature = "simd_runtime_dispatch", target_arch = "aarch64")),
    feature(portable_simd)
)]
//! High-performance multi-pattern text matcher with logical operators and text normalization.
//!
//! `matcher_rs` solves precision/recall problems in keyword matching by combining:
//!
//! - **Two-pass Aho-Corasick matching** — O(N) text scan regardless of the number of patterns.
//! - **Logical operators** — Patterns can require co-occurrence of sub-patterns (`&`) or
//!   veto a match when a sub-pattern is present (`~`).
//! - **Text normalization pipeline** — Input is transformed through configurable combinations
//!   of Traditional→Simplified Chinese ([`ProcessType::Fanjian`]), noise-character deletion
//!   ([`ProcessType::Delete`]), Unicode normalization ([`ProcessType::Normalize`]), and
//!   Pinyin transliteration ([`ProcessType::PinYin`] / [`ProcessType::PinYinChar`]).
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
//! the input and another via Pinyin transliteration.
//!
//! # Feature Flags
//!
//! | Flag | Default | Effect |
//! |------|---------|--------|
//! | `dfa` | on | Uses DFA-backed Aho-Corasick automata where applicable; faster than NFA-based matching but with higher memory use |
//! | `simd_runtime_dispatch` | on | Selects the best available transform kernel at runtime (`AVX2` on x86-64, `NEON` on ARM64, portable fallback elsewhere) |
//! | `runtime_build` | off | Builds transformation tables from source text files at startup instead of embedding pre-compiled binaries |

/// Use mimalloc as the global allocator for reduced fragmentation and better multi-threaded throughput.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod builder;
pub use builder::SimpleMatcherBuilder;

mod process;
pub use process::process_matcher::{
    ProcessType, build_process_type_tree, reduce_text_process, reduce_text_process_emit,
    text_process, walk_process_tree,
};

mod simple_matcher;
pub use simple_matcher::{SimpleMatcher, SimpleResult, SimpleTable, SimpleTableSerde};
