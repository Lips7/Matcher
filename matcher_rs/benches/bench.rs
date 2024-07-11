#![feature(iter_intersperse)]

use divan::Bencher;
use matcher_rs::{SimpleMatchType, SimpleMatcher, TextMatcherTrait};
use nohash_hasher::IntMap;

const CN_SIMPLE_MATCH_TYPE_LIST: &[SimpleMatchType] = &[
    SimpleMatchType::None,
    SimpleMatchType::Fanjian,
    SimpleMatchType::Delete,
    SimpleMatchType::Normalize,
    SimpleMatchType::PinYin,
    SimpleMatchType::PinYinChar,
    SimpleMatchType::DeleteNormalize,
    SimpleMatchType::FanjianDeleteNormalize,
];
const CN_WORD_LIST_100000: &str = include_str!("../../data/word_list/cn/cn_words_100000.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/西游记.txt");

const EN_SIMPLE_MATCH_TYPE_LIST: &[SimpleMatchType] = &[
    SimpleMatchType::None,
    SimpleMatchType::Delete,
    SimpleMatchType::Normalize,
    SimpleMatchType::DeleteNormalize,
];
const EN_WORD_LIST_100000: &str = include_str!("../../data/word_list/en/en_words_100000.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

const SIMPLE_WORD_MAP_SIZE_LIST: &[usize] = &[100, 1000, 10000, 50000];
const COMBINED_TIMES_LIST: &[usize] = &[1, 2, 3, 4, 5];

const DEFAULT_SIMPLE_MATCH_TYPE: SimpleMatchType = SimpleMatchType::None;
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
            let word = &patterns[rng.usize(0..patterns.len())];
            combined_word_list.push(word);
        }
        let combined_word = combined_word_list
            .into_iter()
            .map(|s| s.as_str())
            .intersperse(rng.choice(["&", "~"]).unwrap())
            .collect::<String>();
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

mod build_cn {
    use super::*;

    #[divan::bench(args = CN_SIMPLE_MATCH_TYPE_LIST, max_time = 5)]
    fn build_cn_by_simple_match_type(bencher: Bencher, simple_match_type: SimpleMatchType) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(simple_match_type, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn build_cn_by_simple_word_map_size(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            simple_word_map_size,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn build_cn_by_combined_times(bencher: Bencher, combined_times: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            combined_times,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench]
    fn build_cn_by_multiple_simple_match_type(bencher: Bencher) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        for simple_match_type in [
            SimpleMatchType::Fanjian,
            SimpleMatchType::DeleteNormalize,
            SimpleMatchType::FanjianDeleteNormalize,
            SimpleMatchType::Delete,
            SimpleMatchType::Normalize,
        ] {
            let simple_word_map = build_simple_word_map(
                "cn",
                DEFAULT_SIMPLE_WORD_MAP_SIZE,
                DEFAULT_COMBINED_TIMES,
                &mut global_word_id,
            );
            simple_match_type_word_map.insert(simple_match_type, simple_word_map);
        }

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }
}

mod build_en {
    use super::*;

    #[divan::bench(args = EN_SIMPLE_MATCH_TYPE_LIST, max_time = 5)]
    fn build_en_by_simple_match_type(bencher: Bencher, simple_match_type: SimpleMatchType) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(simple_match_type, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn build_en_by_simple_word_map_size(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            simple_word_map_size,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn build_en_by_combined_times(bencher: Bencher, combined_times: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            combined_times,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench]
    fn build_en_by_multiple_simple_match_type(bencher: Bencher) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        for simple_match_type in [
            SimpleMatchType::None,
            SimpleMatchType::Delete,
            SimpleMatchType::DeleteNormalize,
        ] {
            let simple_word_map = build_simple_word_map(
                "cn",
                DEFAULT_SIMPLE_WORD_MAP_SIZE,
                DEFAULT_COMBINED_TIMES,
                &mut global_word_id,
            );
            simple_match_type_word_map.insert(simple_match_type, simple_word_map);
        }

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }
}

mod search_cn {
    use super::*;

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn search_cn_baseline(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let simple_word_map = build_simple_word_map_baseline("cn", simple_word_map_size);
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = CN_SIMPLE_MATCH_TYPE_LIST, max_time = 5)]
    fn search_cn_by_simple_match_type(bencher: Bencher, simple_match_type: SimpleMatchType) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(simple_match_type, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn search_cn_by_simple_word_map_size(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            simple_word_map_size,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn search_cn_by_combined_times(bencher: Bencher, combined_times: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "cn",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            combined_times,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench]
    fn search_cn_by_multiple_simple_match_type(bencher: Bencher) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        for simple_match_type in [
            SimpleMatchType::Fanjian,
            SimpleMatchType::DeleteNormalize,
            SimpleMatchType::FanjianDeleteNormalize,
            SimpleMatchType::Delete,
            SimpleMatchType::Normalize,
        ] {
            let simple_word_map = build_simple_word_map(
                "cn",
                DEFAULT_SIMPLE_WORD_MAP_SIZE,
                DEFAULT_COMBINED_TIMES,
                &mut global_word_id,
            );
            simple_match_type_word_map.insert(simple_match_type, simple_word_map);
        }
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

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
        let mut simple_match_type_word_map = IntMap::default();
        let simple_word_map = build_simple_word_map_baseline("en", simple_word_map_size);
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = EN_SIMPLE_MATCH_TYPE_LIST, max_time = 5)]
    fn search_en_by_simple_match_type(bencher: Bencher, simple_match_type: SimpleMatchType) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(simple_match_type, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn search_en_by_simple_word_map_size(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            simple_word_map_size,
            DEFAULT_COMBINED_TIMES,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn search_en_by_combined_times(bencher: Bencher, combined_times: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        let simple_word_map = build_simple_word_map(
            "en",
            DEFAULT_SIMPLE_WORD_MAP_SIZE,
            combined_times,
            &mut global_word_id,
        );
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench]
    fn search_en_by_multiple_simple_match_type(bencher: Bencher) {
        let mut simple_match_type_word_map = IntMap::default();
        let mut global_word_id = 0;
        for simple_match_type in [
            SimpleMatchType::None,
            SimpleMatchType::Delete,
            SimpleMatchType::DeleteNormalize,
        ] {
            let simple_word_map = build_simple_word_map(
                "cn",
                DEFAULT_SIMPLE_WORD_MAP_SIZE,
                DEFAULT_COMBINED_TIMES,
                &mut global_word_id,
            );
            simple_match_type_word_map.insert(simple_match_type, simple_word_map);
        }
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }
}

fn main() {
    println!("Current default simple match type: {DEFAULT_SIMPLE_MATCH_TYPE:?}");
    println!("Current default simple word map size: {DEFAULT_SIMPLE_WORD_MAP_SIZE:?}");
    println!("Current default combined times: {DEFAULT_COMBINED_TIMES:?}");

    divan::main()
}
