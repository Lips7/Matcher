#![feature(portable_simd)]
#![feature(iter_repeat_n)]

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod process;
pub use process::process_matcher::{
    get_process_matcher, reduce_text_process, text_process, ProcessMatcher,
};

mod matcher;
pub use matcher::{
    MatchResult, MatchResultTrait, MatchTable, MatchTableMap, MatchTableType, Matcher,
    TextMatcherTrait,
};

mod simple_matcher;
pub use simple_matcher::{SimpleMatchType, SimpleMatchTypeWordMap, SimpleMatcher, SimpleResult};

mod regex_matcher;
pub use regex_matcher::{RegexMatchType, RegexMatcher, RegexResult, RegexTable};

mod sim_matcher;
pub use sim_matcher::{SimMatchType, SimMatcher, SimResult, SimTable};
