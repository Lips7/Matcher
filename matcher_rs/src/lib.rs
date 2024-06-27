#[cfg(all(feature = "prebuilt", feature = "runtime_build"))]
compile_error!("feature \"prebuilt\" and feature \"runtime_build\" cannot be enabled at the same time");

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod util;

mod process;
pub use process::process_matcher::{reduce_text_process, text_process};

mod simple_matcher;
pub use simple_matcher::{SimpleMatchType, SimpleMatchTypeWordMap, SimpleMatcher, SimpleResult};

mod regex_matcher;
pub use regex_matcher::{RegexMatchType, RegexMatcher, RegexTable};

mod sim_matcher;
pub use sim_matcher::{SimMatchType, SimMatcher, SimTable};

mod matcher;
pub use matcher::{
    MatchResult, MatchResultTrait, MatchTable, MatchTableMap, MatchTableType, Matcher,
    TextMatcherTrait,
};
