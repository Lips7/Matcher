use std::collections::HashMap;
use std::hint::black_box;

use divan::Bencher;
use matcher_rs::{ProcessType, SimpleMatcher, TextMatcherTrait};

const CN_PROCESS_TYPES: &[ProcessType] = &[
    ProcessType::None,
    ProcessType::Delete,
    ProcessType::Fanjian,
    ProcessType::FanjianDeleteNormalize,
];

const EN_PROCESS_TYPES: &[ProcessType] = &[
    ProcessType::None,
    ProcessType::Delete,
    ProcessType::DeleteNormalize,
];

const SIMPLE_WORD_MAP_SIZE_LIST: &[usize] = &[1000, 10000, 50000];
const COMBINED_TIMES_LIST: &[usize] = &[1, 3, 5];

const DEFAULT_PROCESS_TYPE: ProcessType = ProcessType::None;
const DEFAULT_SIMPLE_WORD_MAP_SIZE: usize = 10000;
const DEFAULT_COMBINED_TIMES: usize = 3;

const CN_WORD_LIST_100000: &str = include_str!("../../data/word_list/cn/cn_words_100000.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/西游记.txt");

const EN_WORD_LIST_100000: &str = include_str!("../../data/word_list/en/en_words_100000.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

/// Builds a simple word map deterministically.
///
/// * `en_or_cn`: "en" or "cn" dictating the vocabulary
/// * `simple_word_map_size`: Target number of combinations to generate
/// * `combined_times`: Target tokens per word (e.g., A & B & C)
/// * `match_scenario`: If false, the generated words are mangled so they will *never* match the haystack.
fn build_deterministic_map(
    en_or_cn: &str,
    simple_word_map_size: usize,
    combined_times: usize,
    match_scenario: bool,
) -> HashMap<u32, String> {
    let mut patterns: Vec<&str> = if en_or_cn == "cn" {
        CN_WORD_LIST_100000.lines().collect()
    } else {
        EN_WORD_LIST_100000.lines().collect()
    };
    patterns.sort_unstable();

    let mut simple_word_map = HashMap::new();
    let mut global_word_id = 0u32;
    let operators = ["&", "~"];

    for i in 0..simple_word_map_size {
        global_word_id += 1;
        let mut combined_word_list = Vec::with_capacity(combined_times);

        for j in 0..combined_times {
            // Deterministic word selection heavily distributed across the pattern list
            let word_idx = (i * combined_times + j * 997) % patterns.len();
            let mut word = patterns[word_idx].to_string();

            if !match_scenario {
                // Mangle the word so it cannot possibly match normal text
                word = format!("__impossible_{word_idx}_match_{i}__");
            }

            combined_word_list.push(word);
        }

        // Deterministic operator selection
        let op = operators[i % operators.len()];
        let combined_word = combined_word_list.join(op);
        simple_word_map.insert(global_word_id, combined_word);
    }
    simple_word_map
}

mod build {
    use super::*;

    #[divan::bench(args = CN_PROCESS_TYPES, max_time = 5)]
    fn cn_by_process_type(bencher: Bencher, process_type: ProcessType) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            true,
        );
        simple_table.insert(process_type, simple_word_map);

        bencher.bench(|| {
            let _ = black_box(SimpleMatcher::new(&simple_table));
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn cn_by_size(bencher: Bencher, size: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map("cn", size, DEFAULT_COMBINED_TIMES, true);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = black_box(SimpleMatcher::new(&simple_table));
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn cn_by_combinations(bencher: Bencher, combined_times: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map =
            build_deterministic_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, combined_times, true);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = black_box(SimpleMatcher::new(&simple_table));
        });
    }

    #[divan::bench(args = EN_PROCESS_TYPES, max_time = 5)]
    fn en_by_process_type(bencher: Bencher, process_type: ProcessType) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            true,
        );
        simple_table.insert(process_type, simple_word_map);

        bencher.bench(|| {
            let _ = black_box(SimpleMatcher::new(&simple_table));
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn en_by_size(bencher: Bencher, size: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map("en", size, DEFAULT_COMBINED_TIMES, true);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = black_box(SimpleMatcher::new(&simple_table));
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn en_by_combinations(bencher: Bencher, combined_times: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map =
            build_deterministic_map("en", DEFAULT_SIMPLE_WORD_MAP_SIZE, combined_times, true);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = black_box(SimpleMatcher::new(&simple_table));
        });
    }
}

mod search_match {
    use super::*;

    #[divan::bench(args = CN_PROCESS_TYPES, max_time = 5)]
    fn cn_by_process_type(bencher: Bencher, process_type: ProcessType) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            true,
        );
        simple_table.insert(process_type, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn cn_by_size(bencher: Bencher, size: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map("cn", size, DEFAULT_COMBINED_TIMES, true);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn cn_by_combinations(bencher: Bencher, combined_times: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map =
            build_deterministic_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, combined_times, true);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = EN_PROCESS_TYPES, max_time = 5)]
    fn en_by_process_type(bencher: Bencher, process_type: ProcessType) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            true,
        );
        simple_table.insert(process_type, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn en_by_size(bencher: Bencher, size: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map("en", size, DEFAULT_COMBINED_TIMES, true);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn en_by_combinations(bencher: Bencher, combined_times: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map =
            build_deterministic_map("en", DEFAULT_SIMPLE_WORD_MAP_SIZE, combined_times, true);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }
}

mod search_no_match {
    use super::*;

    #[divan::bench(args = CN_PROCESS_TYPES, max_time = 5)]
    fn cn_by_process_type(bencher: Bencher, process_type: ProcessType) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            false,
        );
        simple_table.insert(process_type, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn cn_by_size(bencher: Bencher, size: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map("cn", size, DEFAULT_COMBINED_TIMES, false);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn cn_by_combinations(bencher: Bencher, combined_times: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map =
            build_deterministic_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, combined_times, false);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn en_by_size(bencher: Bencher, size: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map("en", size, DEFAULT_COMBINED_TIMES, false);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = EN_PROCESS_TYPES, max_time = 5)]
    fn en_by_process_type(bencher: Bencher, process_type: ProcessType) {
        let mut simple_table = HashMap::new();
        let simple_word_map = build_deterministic_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            false,
        );
        simple_table.insert(process_type, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn en_by_combinations(bencher: Bencher, combined_times: usize) {
        let mut simple_table = HashMap::new();
        let simple_word_map =
            build_deterministic_map("en", DEFAULT_SIMPLE_WORD_MAP_SIZE, combined_times, false);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }
}

fn main() {
    println!(
        "Current default simple match type: {:?}",
        DEFAULT_PROCESS_TYPE
    );
    println!(
        "Current default simple word map size: {:?}",
        DEFAULT_SIMPLE_WORD_MAP_SIZE
    );
    println!(
        "Current default combined times: {:?}",
        DEFAULT_COMBINED_TIMES
    );

    divan::main()
}
