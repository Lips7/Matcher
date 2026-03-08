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
