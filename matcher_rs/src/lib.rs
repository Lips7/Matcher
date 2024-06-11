#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(portable_simd)]
#![feature(iter_repeat_n)]

/// The global allocator for the program is set to use the MiMalloc allocator.
/// MiMalloc is a general purpose allocator that is designed to be fast and efficient.
/// It is a drop-in replacement for the system allocator, and can be used in place of it without any changes to the code.
/// This allows for easy performance optimization without having to modify the existing codebase.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod matcher;
pub use matcher::{
    MatchResult, MatchResultTrait, MatchTable, MatchTableMap, MatchTableType, Matcher, StrConvType,
    TextMatcherTrait,
};

mod simple_matcher;
pub use simple_matcher::{SimpleMatchType, SimpleMatchTypeWordMap, SimpleMatcher, SimpleResult};

mod regex_matcher;
pub use regex_matcher::{RegexMatcher, RegexResult, RegexTable};

mod sim_matcher;
pub use sim_matcher::{SimMatcher, SimResult, SimTable};
