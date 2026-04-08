#[path = "common/mod.rs"]
mod common;

use std::hint::black_box;

use common::{DEFAULT_RULE_COUNT, EN_HAYSTACK, build_literal_map, wrap_table};
use divan::{Bencher, counter::BytesCount};
use matcher_rs::{ProcessType, SimpleMatcher};

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
