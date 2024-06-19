#![feature(iter_intersperse)]

use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::time::Duration;

use criterion::{
    criterion_group, criterion_main, measurement::WallTime, BenchmarkGroup, Criterion, SamplingMode,
};

use matcher_rs::TextMatcherTrait;
use matcher_rs::{SimpleMatchType, SimpleMatcher};
use nohash_hasher::IntMap;

const BUILD_SAMPLE_SIZE: usize = 10;
const BUILD_WARM_UP_TIME: Duration = Duration::from_millis(500);
const BUILD_MEASURE_TIME: Duration = Duration::from_secs(2);

const SEARCH_SAMPLE_SIZE: usize = 50;
const SEARCH_WARM_UP_TIME: Duration = Duration::from_millis(500);
const SEARCH_MEASURE_TIME: Duration = Duration::from_secs(2);

fn load_file<P>(path: P) -> Vec<String>
where
    P: AsRef<Path>,
{
    let file = File::open(path).unwrap();
    let buf = BufReader::new(file);
    buf.lines().map(|line| line.unwrap()).collect()
}

fn build_simple_word_map(
    patterns: &[String],
    combined_times: usize,
    simple_word_map_size: usize,
    global_word_id: &mut u64,
) -> IntMap<u64, String> {
    let mut simple_word_map = IntMap::default();
    for _ in 0..simple_word_map_size {
        *global_word_id += 1;
        let combined_word = fastrand::choose_multiple(patterns.iter(), combined_times)
            .iter()
            .map(|s| s.as_str())
            .intersperse(",")
            .collect::<String>();
        simple_word_map.insert(*global_word_id, combined_word);
    }
    simple_word_map
}

fn add_build_benches_cn(group: &mut BenchmarkGroup<WallTime>, patterns: &[String]) {
    let mut global_word_id: u64 = 0;
    for simple_match_type in [
        SimpleMatchType::None,
        SimpleMatchType::Fanjian,
        SimpleMatchType::Delete,
        SimpleMatchType::Normalize,
        SimpleMatchType::PinYin,
        SimpleMatchType::PinYinChar,
        SimpleMatchType::DeleteNormalize,
        SimpleMatchType::FanjianDeleteNormalize,
        SimpleMatchType::FanjianDeleteNormalize | SimpleMatchType::PinYin,
    ] {
        for simple_word_map_size in [100, 1000, 10000, 50000] {
            for combined_times in [1, 2, 3, 4] {
                let mut simple_match_type_word_map = IntMap::default();
                let simple_word_map = build_simple_word_map(
                    patterns,
                    combined_times,
                    simple_word_map_size,
                    &mut global_word_id,
                );
                simple_match_type_word_map.insert(simple_match_type, simple_word_map);
                group.bench_function(
                    format!("simple_matcher_build_{simple_match_type}_{simple_word_map_size}_{combined_times}"),
                    |b| {
                        b.iter(|| {
                            let _ = SimpleMatcher::new(&simple_match_type_word_map);
                        })
                    },
                );
            }
        }
    }
}

fn add_build_benches_en(group: &mut BenchmarkGroup<WallTime>, patterns: &[String]) {
    let mut global_word_id: u64 = 0;
    for simple_match_type in [
        SimpleMatchType::None,
        SimpleMatchType::Delete,
        SimpleMatchType::Normalize,
        SimpleMatchType::DeleteNormalize,
    ] {
        for simple_word_map_size in [100, 1000, 10000, 50000] {
            for combined_times in [1, 2, 3, 4] {
                let mut simple_match_type_word_map = IntMap::default();
                let simple_word_map = build_simple_word_map(
                    patterns,
                    combined_times,
                    simple_word_map_size,
                    &mut global_word_id,
                );
                simple_match_type_word_map.insert(simple_match_type, simple_word_map);
                group.bench_function(
                    format!("simple_matcher_build_{simple_match_type}_{simple_word_map_size}_{combined_times}"),
                    |b| {
                        b.iter(|| {
                            let _ = SimpleMatcher::new(&simple_match_type_word_map);
                        })
                    },
                );
            }
        }
    }
}

fn add_search_benches_cn(
    group: &mut BenchmarkGroup<WallTime>,
    patterns: &[String],
    haystacks: &[String],
) {
    let mut global_word_id: u64 = 0;
    for simple_match_type in [
        SimpleMatchType::None,
        SimpleMatchType::Fanjian,
        SimpleMatchType::Delete,
        SimpleMatchType::Normalize,
        SimpleMatchType::PinYin,
        SimpleMatchType::PinYinChar,
        SimpleMatchType::DeleteNormalize,
        SimpleMatchType::FanjianDeleteNormalize,
        SimpleMatchType::FanjianDeleteNormalize | SimpleMatchType::PinYin,
    ] {
        for simple_word_map_size in [100, 1000, 10000, 50000] {
            for combined_times in [1, 2, 3, 4] {
                let mut simple_match_type_word_map = IntMap::default();
                let simple_word_map = build_simple_word_map(
                    patterns,
                    combined_times,
                    simple_word_map_size,
                    &mut global_word_id,
                );
                simple_match_type_word_map.insert(simple_match_type, simple_word_map);
                let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);
                group.bench_function(
                    format!("simple_matcher_search_{simple_match_type}_{simple_word_map_size}_{combined_times}"),
                    |b| {
                        b.iter(|| {
                            for haystack in haystacks {
                                simple_matcher.process(haystack);
                            }
                        })
                    },
                );
            }
        }
    }
}

fn add_search_benches_en(
    group: &mut BenchmarkGroup<WallTime>,
    patterns: &[String],
    haystacks: &[String],
) {
    let mut global_word_id: u64 = 0;
    for simple_match_type in [
        SimpleMatchType::None,
        SimpleMatchType::Delete,
        SimpleMatchType::Normalize,
        SimpleMatchType::DeleteNormalize,
    ] {
        for simple_word_map_size in [100, 1000, 10000, 50000] {
            for combined_times in [1, 2, 3, 4] {
                let mut simple_match_type_word_map = IntMap::default();
                let simple_word_map = build_simple_word_map(
                    patterns,
                    combined_times,
                    simple_word_map_size,
                    &mut global_word_id,
                );
                simple_match_type_word_map.insert(simple_match_type, simple_word_map);
                let simple_matcher = SimpleMatcher::new(&simple_match_type_word_map);
                group.bench_function(
                    format!("simple_matcher_search_{simple_match_type}_{simple_word_map_size}_{combined_times}"),
                    |b| {
                        b.iter(|| {
                            for haystack in haystacks {
                                simple_matcher.process(haystack);
                            }
                        })
                    },
                );
            }
        }
    }
}

macro_rules! define_build_bench_cn {
    ( $func_name:ident, $group:literal, $corpus:literal ) => {
        fn $func_name(c: &mut Criterion) {
            let mut group = c.benchmark_group($group);
            group.sample_size(BUILD_SAMPLE_SIZE);
            group.warm_up_time(BUILD_WARM_UP_TIME);
            group.measurement_time(BUILD_MEASURE_TIME);
            group.sampling_mode(SamplingMode::Flat);
            let mut patterns = load_file($corpus);
            patterns.sort_unstable();
            add_build_benches_cn(&mut group, &patterns);
        }
    };
}

macro_rules! define_build_bench_en {
    ( $func_name:ident, $group:literal, $corpus:literal ) => {
        fn $func_name(c: &mut Criterion) {
            let mut group = c.benchmark_group($group);
            group.sample_size(BUILD_SAMPLE_SIZE);
            group.warm_up_time(BUILD_WARM_UP_TIME);
            group.measurement_time(BUILD_MEASURE_TIME);
            group.sampling_mode(SamplingMode::Flat);
            let mut patterns = load_file($corpus);
            patterns.sort_unstable();
            add_build_benches_en(&mut group, &patterns);
        }
    };
}

define_build_bench_cn!(
    criterion_words_build_cn,
    "simple_matcher_build_cn",
    "../data/word_list/cn/cn_words_100000.txt"
);

define_build_bench_en!(
    criterion_words_build_en,
    "simple_matcher_build_en",
    "../data/word_list/en/en_words_100000.txt"
);

macro_rules! define_find_bench_cn {
    ( $func_name:ident, $group:literal, $corpus:literal, $haystack:literal ) => {
        fn $func_name(c: &mut Criterion) {
            let mut group = c.benchmark_group($group);
            group.sample_size(SEARCH_SAMPLE_SIZE);
            group.warm_up_time(SEARCH_WARM_UP_TIME);
            group.measurement_time(SEARCH_MEASURE_TIME);
            group.sampling_mode(SamplingMode::Flat);
            let mut patterns = load_file($corpus);
            patterns.sort_unstable();
            let haystacks = load_file($haystack);
            add_search_benches_cn(&mut group, &patterns, &haystacks);
        }
    };
}

macro_rules! define_find_bench_en {
    ( $func_name:ident, $group:literal, $corpus:literal, $haystack:literal ) => {
        fn $func_name(c: &mut Criterion) {
            let mut group = c.benchmark_group($group);
            group.sample_size(SEARCH_SAMPLE_SIZE);
            group.warm_up_time(SEARCH_WARM_UP_TIME);
            group.measurement_time(SEARCH_MEASURE_TIME);
            group.sampling_mode(SamplingMode::Flat);
            let mut patterns = load_file($corpus);
            patterns.sort_unstable();
            let haystacks = load_file($haystack);
            add_search_benches_en(&mut group, &patterns, &haystacks);
        }
    };
}

define_find_bench_cn!(
    criterion_words_search_cn,
    "simple_matcher_search_cn",
    "../data/word_list/cn/cn_words_100000.txt",
    "../data/text/cn/西游记.txt"
);

define_find_bench_en!(
    criterion_words_search_en,
    "simple_matcher_search_en",
    "../data/word_list/en/en_words_100000.txt",
    "../data/text/en/sherlock.txt"
);

criterion_group!(
    benches,
    criterion_words_build_cn,
    criterion_words_build_en,
    criterion_words_search_cn,
    criterion_words_search_en
);
criterion_main!(benches);
