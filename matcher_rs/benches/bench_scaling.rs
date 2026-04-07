#[path = "common/mod.rs"]
mod common;

use common::{CN_HAYSTACK, EN_HAYSTACK, RULE_COUNTS, build_literal_map, wrap_table};
use divan::Bencher;
use divan::counter::BytesCount;
use matcher_rs::{ProcessType, SimpleMatcher};
use std::hint::black_box;

// Question: How does throughput scale with rule count?

mod scaling {
    use super::*;

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn is_match_en(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_literal_map("en", size, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn is_match_cn(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_literal_map("cn", size, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = CN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn process_en(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_literal_map("en", size, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn process_cn(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_literal_map("cn", size, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = CN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }
}

fn main() {
    divan::main()
}
