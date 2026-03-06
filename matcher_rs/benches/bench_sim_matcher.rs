use std::hint::black_box;
use divan::Bencher;
use matcher_rs::{ProcessType, SimMatcher, SimTable, SimMatchType, TextMatcherTrait};

const CN_HAYSTACK: &str = include_str!("../../data/text/cn/西游记.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

const SIM_WORDS: &[&str] = &["悟空", "Sherlock"];
const THRESHOLDS: &[f64] = &[0.7, 0.8, 0.9];

#[divan::bench(args = THRESHOLDS, max_time = 5)]
fn cn_sim(bencher: Bencher, threshold: f64) {
    let sim_table = SimTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        sim_match_type: SimMatchType::Levenshtein,
        word_list: vec!["悟空"],
        threshold,
    };
    let matcher = SimMatcher::new(&[sim_table]);

    bencher.bench(|| {
        for line in CN_HAYSTACK.lines() {
            let _ = black_box(matcher.process(line));
        }
    });
}

#[divan::bench(args = THRESHOLDS, max_time = 5)]
fn en_sim(bencher: Bencher, threshold: f64) {
    let sim_table = SimTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        sim_match_type: SimMatchType::Levenshtein,
        word_list: vec!["Sherlock"],
        threshold,
    };
    let matcher = SimMatcher::new(&[sim_table]);

    bencher.bench(|| {
        for line in EN_HAYSTACK.lines() {
            let _ = black_box(matcher.process(line));
        }
    });
}

fn main() {
    divan::main()
}
