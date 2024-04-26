#![allow(internal_features)]
#![feature(core_intrinsics)]

#[global_allocator]
static GLOBAL: mimalloc_rust::GlobalMiMalloc = mimalloc_rust::GlobalMiMalloc;

mod matcher;
pub use matcher::{MatchTable, MatchTableDict, MatchTableType, Matcher, TextMatcherTrait};

mod simple_matcher;
pub use simple_matcher::{
    SimpleMatchType, SimpleMatcher, SimpleResult, SimpleWord, SimpleWordlistDict,
};

mod regex_matcher;
pub use regex_matcher::{RegexMatcher, RegexResult, RegexTable};

mod sim_matcher;
pub use sim_matcher::{SimMatcher, SimResult, SimTable};
