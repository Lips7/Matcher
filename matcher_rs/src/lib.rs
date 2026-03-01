#![feature(gen_blocks)]

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

mod regex_matcher;
pub use regex_matcher::{RegexMatchType, RegexMatcher, RegexResult, RegexTable};

mod sim_matcher;
pub use sim_matcher::{SimMatchType, SimMatcher, SimResult, SimTable};

mod matcher;
pub use matcher::{
    MatchResult, MatchResultTrait, MatchTable, MatchTableMap, MatchTableMapSerde, MatchTableSerde,
    MatchTableType, Matcher, TextMatcherTrait,
};
