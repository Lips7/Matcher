#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

mod util;
pub use util::word::SimpleWord;

mod process;
pub use process::process_matcher::{
    build_process_type_tree, reduce_text_process, reduce_text_process_emit,
    reduce_text_process_with_list, reduce_text_process_with_tree, text_process, ProcessType,
};

mod simple_matcher;
pub use simple_matcher::{SimpleMatcher, SimpleResult, SimpleTable};

mod regex_matcher;
pub use regex_matcher::{RegexMatchType, RegexMatcher, RegexTable};

mod sim_matcher;
pub use sim_matcher::{SimMatchType, SimMatcher, SimTable};

mod matcher;
pub use matcher::{
    MatchResult, MatchResultTrait, MatchTable, MatchTableMap, MatchTableType, Matcher,
    TextMatcherTrait,
};
