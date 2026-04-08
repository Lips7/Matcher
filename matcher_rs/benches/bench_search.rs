#[path = "common/mod.rs"]
mod common;

use std::hint::black_box;

use common::{
    DEFAULT_RULE_COUNT, EN_HAYSTACK, build_literal_map, build_multi_process_table, wrap_table,
};
use divan::{Bencher, counter::BytesCount};
use matcher_rs::{ProcessType, SimpleMatcher};

// ── Search Mode
// ────────────────────────────────────────────────────────────────
// Question: How do the two SearchMode fast paths compare in throughput?
//
// AllSimple: PT=None, all literals  (bypasses state tracking entirely)
// General:   4 PTs via build_multi_process_table (full state machine)

mod search_mode {
    use super::*;

    mod all_simple {
        use super::*;

        #[divan::bench(max_time = 5)]
        fn is_match(bencher: Bencher) {
            let table = wrap_table(
                ProcessType::None,
                build_literal_map("en", DEFAULT_RULE_COUNT, true),
            );
            let matcher = SimpleMatcher::new(&table).unwrap();
            let haystack = EN_HAYSTACK;
            bencher.counter(BytesCount::new(haystack.len())).bench(|| {
                for line in haystack.lines() {
                    let _ = black_box(matcher.is_match(line));
                }
            });
        }

        #[divan::bench(max_time = 5)]
        fn process(bencher: Bencher) {
            let table = wrap_table(
                ProcessType::None,
                build_literal_map("en", DEFAULT_RULE_COUNT, true),
            );
            let matcher = SimpleMatcher::new(&table).unwrap();
            let haystack = EN_HAYSTACK;
            bencher.counter(BytesCount::new(haystack.len())).bench(|| {
                for line in haystack.lines() {
                    let _ = black_box(matcher.process(line));
                }
            });
        }
    }

    mod general {
        use super::*;

        #[divan::bench(max_time = 5)]
        fn is_match(bencher: Bencher) {
            let table = build_multi_process_table(DEFAULT_RULE_COUNT);
            let matcher = SimpleMatcher::new(&table).unwrap();
            let haystack = EN_HAYSTACK;
            bencher.counter(BytesCount::new(haystack.len())).bench(|| {
                for line in haystack.lines() {
                    let _ = black_box(matcher.is_match(line));
                }
            });
        }

        #[divan::bench(max_time = 5)]
        fn process(bencher: Bencher) {
            let table = build_multi_process_table(DEFAULT_RULE_COUNT);
            let matcher = SimpleMatcher::new(&table).unwrap();
            let haystack = EN_HAYSTACK;
            bencher.counter(BytesCount::new(haystack.len())).bench(|| {
                for line in haystack.lines() {
                    let _ = black_box(matcher.process(line));
                }
            });
        }
    }
}

// ── Match vs No-Match
// ────────────────────────────────────────────────────────── Question: What's
// the throughput difference when patterns match vs. don't match?

mod match_vs_nomatch {
    use super::*;

    #[divan::bench(max_time = 5)]
    fn is_match_hit(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, true),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(max_time = 5)]
    fn is_match_miss(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, false),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(max_time = 5)]
    fn process_hit(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, true),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }

    #[divan::bench(max_time = 5)]
    fn process_miss(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, false),
        );
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
