use std::hint::black_box;
use divan::Bencher;
use matcher_rs::{ProcessType, RegexMatcher, RegexTable, RegexMatchType, TextMatcherTrait};

const CN_HAYSTACK: &str = include_str!("../../data/text/cn/西游记.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

const REGEX_PATTERNS: &[&str] = &["悟空", "悟空.*八戒", "Sherlock", "Sherlock.*Watson"];

#[divan::bench(args = REGEX_PATTERNS, max_time = 5)]
fn cn_regex(bencher: Bencher, pattern: &str) {
    let regex_table = RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Regex,
        word_list: vec![pattern],
    };
    let matcher = RegexMatcher::new(&[regex_table]);

    bencher.bench(|| {
        for line in CN_HAYSTACK.lines() {
            let _ = black_box(matcher.process(line));
        }
    });
}

#[divan::bench(args = REGEX_PATTERNS, max_time = 5)]
fn en_regex(bencher: Bencher, pattern: &str) {
    let regex_table = RegexTable {
        table_id: 1,
        match_id: 1,
        process_type: ProcessType::None,
        regex_match_type: RegexMatchType::Regex,
        word_list: vec![pattern],
    };
    let matcher = RegexMatcher::new(&[regex_table]);

    bencher.bench(|| {
        for line in EN_HAYSTACK.lines() {
            let _ = black_box(matcher.process(line));
        }
    });
}

fn main() {
    divan::main()
}
