use std::collections::HashMap;
use std::time::Instant;

use matcher_rs::{ProcessType, SimpleMatcher};

const DEFAULT_SIMPLE_WORD_MAP_SIZE: usize = 10000;
const DEFAULT_COMBINED_TIMES: usize = 2;

const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/三体.txt");

fn build_deterministic_map(
    simple_word_map_size: usize,
    combined_times: usize,
) -> HashMap<u32, String> {
    let mut patterns: Vec<&str> = CN_WORD_LIST.lines().collect();
    patterns.sort_unstable();

    let mut simple_word_map = HashMap::new();
    let mut global_word_id = 0u32;
    let operators = ["&", "~"];

    for i in 0..simple_word_map_size {
        global_word_id += 1;
        let mut combined_word_list = Vec::with_capacity(combined_times);

        for j in 0..combined_times {
            let word_idx = (i * combined_times + j * 997) % patterns.len();
            let word = patterns[word_idx].to_string();
            combined_word_list.push(word);
        }

        let op = operators[i % operators.len()];
        let combined_word = combined_word_list.join(op);
        simple_word_map.insert(global_word_id, combined_word);
    }
    simple_word_map
}

fn main() {
    println!("Building word map...");
    let map = build_deterministic_map(DEFAULT_SIMPLE_WORD_MAP_SIZE, DEFAULT_COMBINED_TIMES);
    let mut table = HashMap::new();
    table.insert(ProcessType::FanjianDeleteNormalize, map);

    println!("Building SimpleMatcher...");
    let matcher = SimpleMatcher::new(&table);

    println!("Starting profiling loop...");
    let start = Instant::now();
    let mut count = 0;

    // Run for about 30 seconds
    while start.elapsed().as_secs() < 30 {
        for line in CN_HAYSTACK.lines() {
            let _ = matcher.process(line);
        }
        count += 1;
    }

    println!(
        "Processed haystack {} times in {:?}",
        count,
        start.elapsed()
    );
}
