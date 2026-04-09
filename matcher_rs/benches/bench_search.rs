#[path = "common/mod.rs"]
mod common;

use std::hint::black_box;

use common::{
    CN_HAYSTACK, DEFAULT_RULE_COUNT, EN_HAYSTACK, RULE_COUNTS, build_literal_map,
    build_mixed_script_map, build_multi_process_table, build_shaped_map, wrap_table,
};
use divan::{Bencher, counter::BytesCount};
use matcher_rs::{ProcessType, SimpleMatcher};

// ── Search Mode
// ────────────────────────────────────────────────────────────────
// Question: How does simple-literal throughput compare to multi-transform?
//
// all_simple: PT=None, all literals  (is_match uses AC-direct fast path)
// general:    4 PTs via build_multi_process_table (full trie walk + state
// machine)

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

// ── Scaling
// ────────────────────────────────────────────────────────────────────
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

// ── Rule Complexity
// ──────────────────────────────────────────────────────────────
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

// ── Text Length
// ──────────────────────────────────────────────────────────────────
// Question: How does input text length affect per-call latency?

mod text_length {
    use super::*;

    const SHORT_TEXTS: &[&str] = &["hello", "test", "a b c", "matcher"];

    #[divan::bench(max_time = 5)]
    fn short_is_match(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, true),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        bencher.bench(|| {
            for text in SHORT_TEXTS {
                let _ = black_box(matcher.is_match(text));
            }
        });
    }

    #[divan::bench(max_time = 5)]
    fn short_process(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, true),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        bencher.bench(|| {
            for text in SHORT_TEXTS {
                let _ = black_box(matcher.process(text));
            }
        });
    }

    #[divan::bench(max_time = 5)]
    fn long_is_match(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, true),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        bencher
            .counter(BytesCount::new(EN_HAYSTACK.len()))
            .bench(|| {
                let _ = black_box(matcher.is_match(EN_HAYSTACK));
            });
    }

    #[divan::bench(max_time = 5)]
    fn long_process(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, true),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        bencher
            .counter(BytesCount::new(EN_HAYSTACK.len()))
            .bench(|| {
                let _ = black_box(matcher.process(EN_HAYSTACK));
            });
    }
}

fn main() {
    divan::main()
}
