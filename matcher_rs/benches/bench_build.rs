#[path = "common/mod.rs"]
mod common;

use std::hint::black_box;

use common::{
    BUILD_PROCESS_TYPES, DEFAULT_RULE_COUNT, RULE_COUNTS, build_literal_map,
    build_multi_process_table, wrap_table,
};
use divan::Bencher;
use matcher_rs::{ProcessType, SimpleMatcher};

// Question: How fast is SimpleMatcher::new(), and what drives construction
// cost?

mod build {
    use super::*;

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn by_size(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_literal_map("en", size, true));
        bencher.bench_local(|| {
            let _ = black_box(SimpleMatcher::new(&table).unwrap());
        });
    }

    #[divan::bench(args = BUILD_PROCESS_TYPES, max_time = 5)]
    fn by_process_type(bencher: Bencher, pt: ProcessType) {
        let table = wrap_table(pt, build_literal_map("cn", DEFAULT_RULE_COUNT, true));
        bencher.bench_local(|| {
            let _ = black_box(SimpleMatcher::new(&table).unwrap());
        });
    }

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn multi_process_type(bencher: Bencher, size: usize) {
        let table = build_multi_process_table(size);
        bencher.bench_local(|| {
            let _ = black_box(SimpleMatcher::new(&table).unwrap());
        });
    }
}

fn main() {
    divan::main()
}
