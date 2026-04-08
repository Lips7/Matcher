#[path = "common/mod.rs"]
mod common;

use std::hint::black_box;

use common::{
    CN_HAYSTACK, DEFAULT_RULE_COUNT, EN_HAYSTACK, RULE_COUNTS, build_mixed_script_map,
    build_shaped_map, wrap_table,
};
use divan::{Bencher, counter::BytesCount};
use matcher_rs::{ProcessType, SimpleMatcher};

// Question: How do rule shape and mixed-script patterns affect throughput?

mod rule_complexity {
    use super::*;

    const SHAPES: &[&str] = &[
        "literal",
        "and",
        "not",
        "or",
        "word_boundary",
        "deep_and",
        "deep_not",
        "mixed_ops",
    ];

    #[divan::bench(args = SHAPES, max_time = 5)]
    fn shape_is_match(bencher: Bencher, shape: &str) {
        let table = wrap_table(
            ProcessType::None,
            build_shaped_map("en", DEFAULT_RULE_COUNT, shape),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(args = SHAPES, max_time = 5)]
    fn shape_process(bencher: Bencher, shape: &str) {
        let table = wrap_table(
            ProcessType::None,
            build_shaped_map("en", DEFAULT_RULE_COUNT, shape),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn mixed_scripts(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_mixed_script_map(size));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = CN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }
}

fn main() {
    divan::main()
}
