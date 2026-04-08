#[path = "common/mod.rs"]
mod common;

use std::{collections::HashMap, hint::black_box};

use common::{CN_HAYSTACK, DEFAULT_RULE_COUNT, EN_HAYSTACK, build_literal_map, wrap_table};
use divan::{Bencher, counter::BytesCount};
use matcher_rs::{ProcessType, SimpleMatcher};

// ── Text Transform
// ───────────────────────────────────────────────────────────── Question: How
// does each text transformation step affect end-to-end matcher throughput?

mod text_transform {
    use super::*;

    const CN_TRANSFORMS: &[ProcessType] = &[
        ProcessType::VariantNorm,
        ProcessType::Delete,
        ProcessType::Normalize,
        ProcessType::Romanize,
        ProcessType::RomanizeChar,
        ProcessType::EmojiNorm,
    ];

    const EN_TRANSFORMS: &[ProcessType] = &[
        ProcessType::Delete,
        ProcessType::Normalize,
        ProcessType::EmojiNorm,
    ];

    #[divan::bench(args = CN_TRANSFORMS, max_time = 5)]
    fn cn(bencher: Bencher, pt: ProcessType) {
        let table = wrap_table(pt, build_literal_map("cn", DEFAULT_RULE_COUNT, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = CN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }

    #[divan::bench(args = EN_TRANSFORMS, max_time = 5)]
    fn en(bencher: Bencher, pt: ProcessType) {
        let table = wrap_table(pt, build_literal_map("en", DEFAULT_RULE_COUNT, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }
}

// ── Combined Process Types
// ───────────────────────────────────────────────────── Question: How do
// multi-transform pipelines compare to single transforms?

mod combined_process_types {
    use super::*;

    const COMBOS: &[ProcessType] = &[
        ProcessType::DeleteNormalize,
        ProcessType::VariantNormDeleteNormalize,
    ];

    #[divan::bench(args = COMBOS, max_time = 5)]
    fn process_cn(bencher: Bencher, pt: ProcessType) {
        let table = wrap_table(pt, build_literal_map("cn", DEFAULT_RULE_COUNT, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = CN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }

    #[divan::bench(max_time = 5)]
    fn multi_pt_table(bencher: Bencher) {
        let slice = DEFAULT_RULE_COUNT / 3;
        let table = HashMap::from([
            (
                ProcessType::DeleteNormalize,
                build_literal_map("cn", slice, true),
            ),
            (
                ProcessType::VariantNormDeleteNormalize,
                build_literal_map("cn", slice, true),
            ),
            (
                ProcessType::None,
                build_literal_map("en", DEFAULT_RULE_COUNT - slice * 2, true),
            ),
        ]);
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
