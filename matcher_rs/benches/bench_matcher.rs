use std::hint::black_box;
use divan::Bencher;
use matcher_rs::{ProcessType, MatcherBuilder, MatchTableBuilder, MatchTableType, TextMatcherTrait};

const CN_HAYSTACK: &str = include_str!("../../data/text/cn/西游记.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

#[divan::bench(max_time = 5)]
fn cn_matcher(bencher: Bencher) {
    let simple_table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
        .add_word("悟空")
        .add_word("八戒")
        .build();
    let matcher = MatcherBuilder::new().add_table(1, simple_table).build();

    bencher.bench(|| {
        for line in CN_HAYSTACK.lines() {
            let _ = black_box(matcher.process(line));
        }
    });
}

#[divan::bench(max_time = 5)]
fn en_matcher(bencher: Bencher) {
    let simple_table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
        .add_word("Sherlock")
        .add_word("Watson")
        .build();
    let matcher = MatcherBuilder::new().add_table(1, simple_table).build();

    bencher.bench(|| {
        for line in EN_HAYSTACK.lines() {
            let _ = black_box(matcher.process(line));
        }
    });
}

fn main() {
    divan::main()
}
