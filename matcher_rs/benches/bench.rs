use std::hint::black_box;

use divan::Bencher;
use matcher_rs::{ProcessType, SimpleMatcher, TextMatcherTrait};
use nohash_hasher::IntMap;

const CN_PROCESS_TYPE_LIST: &[ProcessType] = &[
    ProcessType::None,
    ProcessType::Fanjian,
    ProcessType::Delete,
    ProcessType::Normalize,
    ProcessType::PinYin,
    ProcessType::PinYinChar,
    ProcessType::DeleteNormalize,
    ProcessType::FanjianDeleteNormalize,
];
const CN_WORD_LIST_100000: &str = include_str!("../../data/word_list/cn/cn_words_100000.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/西游记.txt");

const EN_PROCESS_TYPE_LIST: &[ProcessType] = &[
    ProcessType::None,
    ProcessType::Delete,
    ProcessType::Normalize,
    ProcessType::DeleteNormalize,
];
const EN_WORD_LIST_100000: &str = include_str!("../../data/word_list/en/en_words_100000.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

const SIMPLE_WORD_MAP_SIZE_LIST: &[usize] = &[100, 1000, 10000, 50000];
const COMBINED_TIMES_LIST: &[usize] = &[1, 2, 3, 4, 5];

const DEFAULT_PROCESS_TYPE: ProcessType = ProcessType::None;
const DEFAULT_SIMPLE_WORD_MAP_SIZE: usize = 1000;
const DEFAULT_COMBINED_TIMES: usize = 2;

fn build_simple_word_map(
    en_or_cn: &str,
    simple_word_map_size: usize,
    combined_times: usize,
    global_word_id: &mut u32,
) -> IntMap<u32, String> {
    let mut patterns: Vec<String> = if en_or_cn == "cn" {
        CN_WORD_LIST_100000.lines().map(String::from).collect()
    } else {
        EN_WORD_LIST_100000.lines().map(String::from).collect()
    };
    patterns.sort_unstable();

    let mut simple_word_map = IntMap::default();

    let mut rng = fastrand::Rng::new();

    for _ in 0..simple_word_map_size {
        *global_word_id += 1;
        let mut combined_word_list = Vec::with_capacity(combined_times);
        for _ in 0..combined_times {
            let word = patterns[rng.usize(0..patterns.len())].as_str();
            combined_word_list.push(word);
        }
        let combined_word = combined_word_list.join(rng.choice(["&", "~"]).unwrap());
        simple_word_map.insert(*global_word_id, combined_word);
    }
    simple_word_map
}

fn build_simple_word_map_baseline(
    en_or_cn: &str,
    simple_word_map_size: usize,
) -> IntMap<u32, String> {
    let mut patterns: Vec<String> = if en_or_cn == "cn" {
        CN_WORD_LIST_100000.lines().map(String::from).collect()
    } else {
        EN_WORD_LIST_100000.lines().map(String::from).collect()
    };
    patterns.sort_unstable();

    let mut simple_word_map = IntMap::default();
    let mut global_word_id = 0;

    for word in patterns.iter().take(simple_word_map_size) {
        global_word_id += 1;
        simple_word_map.insert(global_word_id, word.to_owned());
    }
    simple_word_map
}

mod single_line {
    use super::*;

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn search_cn_single_line(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_table = IntMap::default();
        let simple_word_map = build_simple_word_map_baseline("cn", simple_word_map_size);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            simple_matcher.process(black_box("　　从传回的影像上看，在剩下的三秒钟时间里，章北海转向东方延绪方向，竟笑了一下，说出了几个字：“没关系的，都一样。”"));
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn search_en_single_line(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_table = IntMap::default();
        let simple_word_map = build_simple_word_map_baseline("en", simple_word_map_size);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            simple_matcher.process(black_box(
                r#""You will excuse this mask," continued our strange visitor. "The
august person who employs me wishes his agent to be unknown to
you, and I may confess at once that the title by which I have
just called myself is not exactly my own.""#,
            ));
        });
    }
}

mod build_cn {
    use super::*;

    #[divan::bench(args = CN_PROCESS_TYPE_LIST, max_time = 5)]
    fn build_cn_by_process_type(bencher: Bencher, process_type: ProcessType) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_table.insert(process_type, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_table);
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn build_cn_by_simple_word_map_size(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            simple_word_map_size,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_table);
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn build_cn_by_combined_times(bencher: Bencher, combined_times: usize) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            combined_times,
            &mut global_word_id,
        );
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_table);
        });
    }

    #[divan::bench]
    fn build_cn_by_multiple_process_type(bencher: Bencher) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        for process_type in [
            ProcessType::Fanjian,
            ProcessType::DeleteNormalize,
            ProcessType::FanjianDeleteNormalize,
            ProcessType::Delete,
            ProcessType::Normalize,
        ] {
            let simple_word_map = build_simple_word_map(
                "cn",
                DEFAULT_SIMPLE_WORD_MAP_SIZE,
                DEFAULT_COMBINED_TIMES,
                &mut global_word_id,
            );
            simple_table.insert(process_type, simple_word_map);
        }

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_table);
        });
    }
}

mod build_en {
    use super::*;

    #[divan::bench(args = EN_PROCESS_TYPE_LIST, max_time = 5)]
    fn build_en_by_process_type(bencher: Bencher, process_type: ProcessType) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_table.insert(process_type, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_table);
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn build_en_by_simple_word_map_size(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            simple_word_map_size,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_table);
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn build_en_by_combined_times(bencher: Bencher, combined_times: usize) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            combined_times,
            &mut global_word_id,
        );
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_table);
        });
    }

    #[divan::bench]
    fn build_en_by_multiple_process_type(bencher: Bencher) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        for process_type in [
            ProcessType::None,
            ProcessType::Delete,
            ProcessType::DeleteNormalize,
        ] {
            let simple_word_map = build_simple_word_map(
                "cn",
                DEFAULT_SIMPLE_WORD_MAP_SIZE,
                DEFAULT_COMBINED_TIMES,
                &mut global_word_id,
            );
            simple_table.insert(process_type, simple_word_map);
        }

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_table);
        });
    }
}

mod search_cn {
    use super::*;

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn search_cn_baseline(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_table = IntMap::default();
        let simple_word_map = build_simple_word_map_baseline("cn", simple_word_map_size);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = CN_PROCESS_TYPE_LIST, max_time = 5)]
    fn search_cn_by_process_type(bencher: Bencher, process_type: ProcessType) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
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
    fn search_cn_by_simple_word_map_size(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            simple_word_map_size,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn search_cn_by_combined_times(bencher: Bencher, combined_times: usize) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            combined_times,
            &mut global_word_id,
        );
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench]
    fn search_cn_by_multiple_process_type(bencher: Bencher) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        for process_type in [
            ProcessType::Fanjian,
            ProcessType::DeleteNormalize,
            ProcessType::FanjianDeleteNormalize,
            ProcessType::Delete,
            ProcessType::Normalize,
        ] {
            let simple_word_map = build_simple_word_map(
                "cn",
                DEFAULT_SIMPLE_WORD_MAP_SIZE,
                DEFAULT_COMBINED_TIMES,
                &mut global_word_id,
            );
            simple_table.insert(process_type, simple_word_map);
        }
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }
}

mod search_en {
    use super::*;

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn search_en_baseline(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_table = IntMap::default();
        let simple_word_map = build_simple_word_map_baseline("en", simple_word_map_size);
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = EN_PROCESS_TYPE_LIST, max_time = 5)]
    fn search_en_by_process_type(bencher: Bencher, process_type: ProcessType) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
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
    fn search_en_by_simple_word_map_size(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            simple_word_map_size,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn search_en_by_combined_times(bencher: Bencher, combined_times: usize) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            combined_times,
            &mut global_word_id,
        );
        simple_table.insert(DEFAULT_PROCESS_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench]
    fn search_en_by_multiple_process_type(bencher: Bencher) {
        let mut simple_table = IntMap::default();
        let mut global_word_id = 0;
        for process_type in [
            ProcessType::None,
            ProcessType::Delete,
            ProcessType::DeleteNormalize,
        ] {
            let simple_word_map = build_simple_word_map(
                "en",
                DEFAULT_SIMPLE_WORD_MAP_SIZE,
                DEFAULT_COMBINED_TIMES,
                &mut global_word_id,
            );
            simple_table.insert(process_type, simple_word_map);
        }
        let simple_matcher = SimpleMatcher::new(&simple_table);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }
}

fn main() {
    println!("Current default simple match type: {DEFAULT_PROCESS_TYPE:?}");
    println!("Current default simple word map size: {DEFAULT_SIMPLE_WORD_MAP_SIZE:?}");
    println!("Current default combined times: {DEFAULT_COMBINED_TIMES:?}");

    divan::main()
}
