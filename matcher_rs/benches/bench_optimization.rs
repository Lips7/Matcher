#[path = "common/mod.rs"]
mod common;

use common::{DEFAULT_RULE_COUNT, EN_HAYSTACK, build_literal_map};
use divan::Bencher;
use divan::counter::BytesCount;
use matcher_rs::{ProcessType, SimpleMatcher};
use std::collections::HashMap;
use std::hint::black_box;

// Question: How much throughput is gained by folding no-op transform scans?
//
// Uses multiple PTs where VariantNorm and Romanize are no-ops on ASCII text.
// Miss scenario isolates scan cost (no hit processing, no early exit).

mod noop_fold {
    use super::*;

    fn build_noop_heavy_table(size: usize) -> HashMap<ProcessType, HashMap<u32, String>> {
        let slice = (size / 4).max(1);
        HashMap::from([
            (ProcessType::None, build_literal_map("en", slice, false)),
            (
                ProcessType::VariantNorm,
                build_literal_map("en", slice, false),
            ),
            (ProcessType::Romanize, build_literal_map("en", slice, false)),
            (
                ProcessType::Delete,
                build_literal_map("en", size - slice * 3, false),
            ),
        ])
    }

    #[divan::bench(max_time = 5)]
    fn is_match_miss(bencher: Bencher) {
        let table = build_noop_heavy_table(DEFAULT_RULE_COUNT);
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(max_time = 5)]
    fn process_miss(bencher: Bencher) {
        let table = build_noop_heavy_table(DEFAULT_RULE_COUNT);
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
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
