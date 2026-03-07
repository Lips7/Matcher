use matcher_rs::{ProcessType, SimpleMatcher, TextMatcherTrait};
use std::collections::HashMap;
use std::hint::black_box;

const CN_WORD_LIST_100000: &str = include_str!("../../data/word/cn/jieba.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/三体.txt");

fn build_deterministic_map(
    simple_word_map_size: usize,
    combined_times: usize,
) -> HashMap<u32, String> {
    let mut patterns: Vec<&str> = CN_WORD_LIST_100000.lines().collect();
    patterns.sort_unstable();

    let mut simple_word_map = HashMap::new();
    let mut global_word_id = 0u32;
    let operators = ["&", "~"];

    for i in 0..simple_word_map_size {
        global_word_id += 1;
        let mut combined_word_list = Vec::with_capacity(combined_times);

        for j in 0..combined_times {
            let word_idx = (i * combined_times + j * 997) % patterns.len();
            combined_word_list.push(patterns[word_idx].to_string());
        }

        let op = operators[i % operators.len()];
        let combined_word = combined_word_list.join(op);
        simple_word_map.insert(global_word_id, combined_word);
    }
    simple_word_map
}

fn main() {
    eprintln!("Building matcher...");
    let mut simple_table = HashMap::new();
    let simple_word_map = build_deterministic_map(10000, 3);
    simple_table.insert(ProcessType::None, simple_word_map);
    let simple_matcher = SimpleMatcher::new(&simple_table);

    eprintln!("Running profiling workload (200 iterations over 三体)...");
    for _ in 0..200 {
        for line in CN_HAYSTACK.lines() {
            black_box(simple_matcher.process(line));
        }
    }
    eprintln!("Done.");
}
