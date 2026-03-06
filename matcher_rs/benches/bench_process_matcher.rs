use std::hint::black_box;
use divan::Bencher;
use matcher_rs::{ProcessType, get_process_matcher};

const CN_HAYSTACK: &str = include_str!("../../data/text/cn/西游记.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

const CN_SINGLE_BIT_PROCESS_TYPES: &[ProcessType] = &[
    ProcessType::None,
    ProcessType::Fanjian,
    ProcessType::Delete,
    ProcessType::Normalize,
    ProcessType::PinYin,
    ProcessType::PinYinChar,
];

const EN_SINGLE_BIT_PROCESS_TYPES: &[ProcessType] = &[
    ProcessType::None,
    ProcessType::Delete,
    ProcessType::Normalize,
];

#[divan::bench(args = CN_SINGLE_BIT_PROCESS_TYPES, max_time = 5)]
fn cn_process_matcher(bencher: Bencher, process_type: ProcessType) {
    let cached_result = get_process_matcher(process_type);
    let (process_replace_list, process_matcher) = cached_result.as_ref();

    bencher.bench(|| {
        for line in CN_HAYSTACK.lines() {
            match process_type {
                ProcessType::Delete => {
                    let _ = black_box(process_matcher.delete_all(line));
                }
                _ => {
                    let _ = black_box(process_matcher.replace_all(line, process_replace_list));
                }
            }
        }
    });
}

#[divan::bench(args = EN_SINGLE_BIT_PROCESS_TYPES, max_time = 5)]
fn en_process_matcher(bencher: Bencher, process_type: ProcessType) {
    let cached_result = get_process_matcher(process_type);
    let (process_replace_list, process_matcher) = cached_result.as_ref();

    bencher.bench(|| {
        for line in EN_HAYSTACK.lines() {
            match process_type {
                ProcessType::Delete => {
                    let _ = black_box(process_matcher.delete_all(line));
                }
                _ => {
                    let _ = black_box(process_matcher.replace_all(line, process_replace_list));
                }
            }
        }
    });
}

fn main() {
    divan::main()
}
