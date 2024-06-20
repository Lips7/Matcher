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
) -> IntMap<u64, String> {
    let mut patterns: Vec<String> = if en_or_cn == "cn" {
        CN_WORD_LIST_100000.lines().map(String::from).collect()
    } else {
        EN_WORD_LIST_100000.lines().map(String::from).collect()
    };
    patterns.sort_unstable();

    let mut simple_word_map = IntMap::default();
    let mut global_word_id = 0;

    for _ in 0..simple_word_map_size {
        global_word_id += 1;
        let combined_word = fastrand::choose_multiple(patterns.iter(), combined_times)
            .iter()
            .map(|s| s.as_str())
            .intersperse(",")
            .collect::<String>();
        simple_word_map.insert(global_word_id, combined_word);
    }
    simple_word_map
}

mod bench_test {
    use aho_corasick::AhoCorasick;
    use divan::Bencher;

    #[divan::bench]
    fn bench_test_find(bencher: Bencher) {
        let patterns = &["apple", "maple", "snapple"];
        let haystack = "helpdsaifnsajifdqkwehirjksaghujksandhkfjansfgajfdiaosfsajkndjkas";

        let ac = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(patterns)
            .unwrap();

        bencher.bench(|| {
            let mut matches = vec![];
            for mat in ac.find_iter(haystack) {
                matches.push((mat.pattern(), mat.start(), mat.end()));
            }
        });
    }

    #[divan::bench]
    fn bench_test_clone(bencher: Bencher) {
        let haystack = "helpdsaifnsajifdqkwehirjksaghujksandhkfjansfgajfdiaosfsajkndjkas";
        bencher.bench(|| {
            let _ = divan::black_box(haystack).to_string();
        });
    }
}

mod build_cn {
    use super::*;

    #[divan::bench(args = CN_SIMPLE_MATCH_TYPE_LIST, max_time = 5)]
    fn build_cn_by_simple_match_type(bencher: Bencher, simple_match_type: SimpleMatchType) {
        let mut simple_match_type_word_map = IntMap::default();
        let simple_word_map =
            build_simple_word_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, DEFAULT_COMBINED_TIMES);
        simple_match_type_word_map.insert(simple_match_type, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn build_cn_by_simple_word_map_size(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let simple_word_map =
            build_simple_word_map("cn", simple_word_map_size, DEFAULT_COMBINED_TIMES);
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn build_cn_by_combined_times(bencher: Bencher, combined_times: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let simple_word_map =
            build_simple_word_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, combined_times);
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench]
    fn build_cn_by_mutiple_simple_match_type(bencher: Bencher) {
        let mut simple_match_type_word_map = IntMap::default();
        for simple_match_type in [
            SimpleMatchType::Fanjian,
            SimpleMatchType::DeleteNormalize,
            SimpleMatchType::FanjianDeleteNormalize,
        ] {
            let simple_word_map =
                build_simple_word_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, DEFAULT_COMBINED_TIMES);
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
        let simple_word_map =
            build_simple_word_map("en", DEFAULT_SIMPLE_WORD_MAP_SIZE, DEFAULT_COMBINED_TIMES);
        simple_match_type_word_map.insert(simple_match_type, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench(args = SIMPLE_WORD_MAP_SIZE_LIST, max_time = 5)]
    fn build_en_by_simple_word_map_size(bencher: Bencher, simple_word_map_size: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let simple_word_map =
            build_simple_word_map("en", simple_word_map_size, DEFAULT_COMBINED_TIMES);
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench(args = COMBINED_TIMES_LIST, max_time = 5)]
    fn build_en_by_combined_times(bencher: Bencher, combined_times: usize) {
        let mut simple_match_type_word_map = IntMap::default();
        let simple_word_map =
            build_simple_word_map("en", DEFAULT_SIMPLE_WORD_MAP_SIZE, combined_times);
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }

    #[divan::bench]
    fn build_en_by_mutiple_simple_match_type(bencher: Bencher) {
        let mut simple_match_type_word_map = IntMap::default();
        for simple_match_type in [
            SimpleMatchType::None,
            SimpleMatchType::Delete,
            SimpleMatchType::DeleteNormalize,
        ] {
            let simple_word_map =
                build_simple_word_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, DEFAULT_COMBINED_TIMES);
            simple_match_type_word_map.insert(simple_match_type, simple_word_map);
        }

        bencher.bench(|| {
            let _ = SimpleMatcher::new(&simple_match_type_word_map);
        });
    }
}

mod search_cn {
    use super::*;

    #[divan::bench(args = CN_SIMPLE_MATCH_TYPE_LIST, max_time = 5)]
    fn search_cn_by_simple_match_type(bencher: Bencher, simple_match_type: SimpleMatchType) {
        let mut simple_match_type_word_map = IntMap::default();
        let simple_word_map =
            build_simple_word_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, DEFAULT_COMBINED_TIMES);
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
        let simple_word_map =
            build_simple_word_map("cn", simple_word_map_size, DEFAULT_COMBINED_TIMES);
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
        let simple_word_map =
            build_simple_word_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, combined_times);
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in CN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench]
    fn search_cn_by_mutiple_simple_match_type(bencher: Bencher) {
        let mut simple_match_type_word_map = IntMap::default();
        for simple_match_type in [
            SimpleMatchType::Fanjian,
            SimpleMatchType::DeleteNormalize,
            SimpleMatchType::FanjianDeleteNormalize,
        ] {
            let simple_word_map =
                build_simple_word_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, DEFAULT_COMBINED_TIMES);
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

    #[divan::bench(args = EN_SIMPLE_MATCH_TYPE_LIST, max_time = 5)]
    fn search_en_by_simple_match_type(bencher: Bencher, simple_match_type: SimpleMatchType) {
        let mut simple_match_type_word_map = IntMap::default();
        let simple_word_map =
            build_simple_word_map("en", DEFAULT_SIMPLE_WORD_MAP_SIZE, DEFAULT_COMBINED_TIMES);
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
        let simple_word_map =
            build_simple_word_map("en", simple_word_map_size, DEFAULT_COMBINED_TIMES);
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
        let simple_word_map =
            build_simple_word_map("en", DEFAULT_SIMPLE_WORD_MAP_SIZE, combined_times);
        simple_match_type_word_map.insert(DEFAULT_SIMPLE_MATCH_TYPE, simple_word_map);
        let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);

        bencher.bench(|| {
            for line in EN_HAYSTACK.lines() {
                simple_matcher.process(line);
            }
        });
    }

    #[divan::bench]
    fn search_en_by_mutiple_simple_match_type(bencher: Bencher) {
        let mut simple_match_type_word_map = IntMap::default();
        for simple_match_type in [
            SimpleMatchType::None,
            SimpleMatchType::Delete,
            SimpleMatchType::DeleteNormalize,
        ] {
            let simple_word_map =
                build_simple_word_map("cn", DEFAULT_SIMPLE_WORD_MAP_SIZE, DEFAULT_COMBINED_TIMES);
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
