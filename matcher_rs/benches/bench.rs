use divan::Bencher;
use divan::counter::BytesCount;
use matcher_rs::{ProcessType, SimpleMatcher};
use std::collections::HashMap;
use std::hint::black_box;

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

const SIMPLE_WORD_MAP_SIZE_LIST: &[usize] = &[1000, 10000, 50000, 100000];
const COMBINED_TIMES_LIST: &[usize] = &[1, 2, 3, 4];

const DEFAULT_PROCESS_TYPE: ProcessType = ProcessType::None;
const DEFAULT_SIMPLE_WORD_MAP_SIZE: usize = 10000;
const DEFAULT_COMBINED_TIMES: usize = 1;

const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/三体.txt");

const EN_WORD_LIST: &str = include_str!("../../data/word/en/dictionary.txt");
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
        CN_WORD_LIST.lines().collect()
    } else {
        EN_WORD_LIST.lines().collect()
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

    macro_rules! define_build_bench {
        ($lang:ident) => {
            paste::item! {
                #[divan::bench(args = [<$lang:upper _PROCESS_TYPES>], max_time = 5)]
                fn [<$lang:lower _by_process_type>](bencher: Bencher, process_type: ProcessType) {
                    let simple_word_map = build_deterministic_map(
                        stringify!($lang).to_lowercase().as_str(),
                        DEFAULT_SIMPLE_WORD_MAP_SIZE,
                        DEFAULT_COMBINED_TIMES,
                        true,
                    );
                    bencher.bench(|| {
                        let mut simple_table = HashMap::new();
                        simple_table.insert(process_type, simple_word_map.clone());
                        let _ = black_box(SimpleMatcher::new(&simple_table));
                    });
                }

                #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
                fn [<$lang:lower _by_size>](bencher: Bencher, size: usize) {
                    bencher.bench_local(|| {
                        let mut simple_table = HashMap::new();
                        let simple_word_map = build_deterministic_map(
                            stringify!($lang).to_lowercase().as_str(),
                            size,
                            DEFAULT_COMBINED_TIMES,
                            true,
                        );
                        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
                        let _ = black_box(SimpleMatcher::new(&simple_table));
                    });
                }

                #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
                fn [<$lang:lower _by_combinations>](bencher: Bencher, combined_times: usize) {
                    let simple_word_map = build_deterministic_map(
                        stringify!($lang).to_lowercase().as_str(),
                        DEFAULT_SIMPLE_WORD_MAP_SIZE,
                        combined_times,
                        true,
                    );
                    bencher.bench(|| {
                        let mut simple_table = HashMap::new();
                        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map.clone());
                        let _ = black_box(SimpleMatcher::new(&simple_table));
                    });
                }
            }
        };
    }

    define_build_bench!(CN);
    define_build_bench!(EN);
}

macro_rules! define_search_bench {
    ($lang:ident, $match_scenario:expr, $method:ident) => {
        paste::item! {
            #[divan::bench(args = [<$lang:upper _PROCESS_TYPES>], max_time = 5)]
            fn [<$lang:lower _by_process_type>](bencher: Bencher, process_type: ProcessType) {
                let mut simple_table = HashMap::new();
                let simple_word_map = build_deterministic_map(
                    stringify!($lang).to_lowercase().as_str(),
                    DEFAULT_SIMPLE_WORD_MAP_SIZE,
                    DEFAULT_COMBINED_TIMES,
                    $match_scenario,
                );
                simple_table.insert(process_type, simple_word_map);
                let matcher = SimpleMatcher::new(&simple_table);
                let haystack = [<$lang:upper _HAYSTACK>];

                let total_bytes = haystack.len();

                bencher
                    .counter(BytesCount::new(total_bytes))
                    .bench(|| {
                        for line in haystack.lines() {
                            let _ = black_box(matcher.$method(line));
                        }
                    });
            }

            #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
            fn [<$lang:lower _by_size>](bencher: Bencher, size: usize) {
                let mut simple_table = HashMap::new();
                let simple_word_map = build_deterministic_map(
                    stringify!($lang).to_lowercase().as_str(),
                    size,
                    DEFAULT_COMBINED_TIMES,
                    $match_scenario,
                );
                simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
                let matcher = SimpleMatcher::new(&simple_table);
                let haystack = [<$lang:upper _HAYSTACK>];

                let total_bytes = haystack.len();

                bencher
                    .counter(BytesCount::new(total_bytes))
                    .bench(|| {
                        for line in haystack.lines() {
                            let _ = black_box(matcher.$method(line));
                        }
                    });
            }

            #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
            fn [<$lang:lower _by_combinations>](bencher: Bencher, combined_times: usize) {
                let mut simple_table = HashMap::new();
                let simple_word_map = build_deterministic_map(
                    stringify!($lang).to_lowercase().as_str(),
                    DEFAULT_SIMPLE_WORD_MAP_SIZE,
                    combined_times,
                    $match_scenario,
                );
                simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
                let matcher = SimpleMatcher::new(&simple_table);
                let haystack = [<$lang:upper _HAYSTACK>];

                let total_bytes = haystack.len();

                bencher
                    .counter(BytesCount::new(total_bytes))
                    .bench(|| {
                        for line in haystack.lines() {
                            let _ = black_box(matcher.$method(line));
                        }
                    });
            }
        }
    };
}

mod search_match {
    use super::*;

    define_search_bench!(CN, true, process);
    define_search_bench!(EN, true, process);
}

mod search_no_match {
    use super::*;

    define_search_bench!(CN, false, process);
    define_search_bench!(EN, false, process);
}

mod is_match_match {
    use super::*;
    define_search_bench!(CN, true, is_match);
    define_search_bench!(EN, true, is_match);
}

mod is_match_no_match {
    use super::*;

    define_search_bench!(CN, false, is_match);
    define_search_bench!(EN, false, is_match);
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
