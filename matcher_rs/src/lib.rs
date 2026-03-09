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
//! # Feature Flags
//!
//! | Flag | Default | Effect |
//! |------|---------|--------|
//! | `dfa` | on | Uses an Aho-Corasick DFA for normalization; ~10× more memory than NFA but faster |
//! | `vectorscan` | off | SIMD-accelerated scanning via Intel Hyperscan; requires Boost, no Windows/ARM64 |
//! | `runtime_build` | off | Builds transformation tables from source text files at startup instead of embedding pre-compiled binaries |

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod builder;
pub use builder::SimpleMatcherBuilder;

mod process;
pub use process::process_matcher::{
    ProcessType, ProcessedTextMasks, build_process_type_tree, get_process_matcher,
    reduce_text_process, reduce_text_process_emit, reduce_text_process_with_set,
    reduce_text_process_with_tree, text_process,
};

mod simple_matcher;
pub use simple_matcher::{SimpleMatcher, SimpleResult, SimpleTable, SimpleTableSerde};

#[cfg(feature = "vectorscan")]
pub mod vectorscan;
