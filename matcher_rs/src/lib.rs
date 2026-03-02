//! # Matcher
//!
//! A high-performance, multi-language word-matching library implemented in Rust.
//!
//! This library provides several engines for matching patterns in text:
//! - **`SimpleMatcher`**: Fast Aho-Corasick based matcher supporting AND (`&`) and NOT (`~`) logic.
//! - **`RegexMatcher`**: Matcher using regular expressions.
//! - **`SimMatcher`**: Similarity-based matcher for fuzzy matching.
//! - **`Matcher`**: A high-level orchestrator that combines all of the above.
//!
//! The library also includes a comprehensive text processing pipeline (Fanjian, Pinyin, etc.)
//! to normalize text before matching.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod builder;
pub use builder::{MatchTableBuilder, MatcherBuilder, SimpleMatcherBuilder};

mod process;
pub use process::process_matcher::{
    ProcessType, ProcessTypeError, ProcessedTextMasks, build_process_type_tree,
    get_process_matcher, reduce_text_process, reduce_text_process_emit,
    reduce_text_process_with_set, reduce_text_process_with_tree, text_process,
};

mod simple_matcher;
pub use simple_matcher::{SimpleMatcher, SimpleResult, SimpleTable, SimpleTableSerde};

#[cfg(feature = "vectorscan")]
pub mod vectorscan_matcher;

mod regex_matcher;
pub use regex_matcher::{RegexMatchType, RegexMatcher, RegexResult, RegexTable};

mod sim_matcher;
pub use sim_matcher::{SimMatchType, SimMatcher, SimResult, SimTable};

mod matcher;
pub use matcher::{
    MatchResult, MatchResultTrait, MatchTable, MatchTableMap, MatchTableMapSerde, MatchTableSerde,
    MatchTableType, Matcher, TextMatcherTrait,
};
