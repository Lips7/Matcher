#![allow(internal_features)]
#![feature(core_intrinsics)]
#![feature(portable_simd)]
#![feature(iter_repeat_n)]

#[global_allocator]
static GLOBAL: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

mod matcher;
pub use matcher::{
    MatchResultTrait, MatchTable, MatchTableMap, MatchTableType, Matcher, StrConvType,
    TextMatcherTrait,
};

mod simple_matcher;
pub use simple_matcher::{SimpleMatchType, SimpleMatchTypeWordMap, SimpleMatcher, SimpleResult};

mod regex_matcher;
pub use regex_matcher::{RegexMatcher, RegexResult, RegexTable};

mod sim_matcher;
pub use sim_matcher::{SimMatcher, SimResult, SimTable};
