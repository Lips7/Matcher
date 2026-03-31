use divan::Bencher;
use divan::counter::BytesCount;
use matcher_rs::{ProcessType, SimpleMatcher, text_process};
use std::collections::HashMap;
use std::hint::black_box;

// ── Data ────────────────────────────────────────────────────────────────────────

const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/三体.txt");

const EN_WORD_LIST: &str = include_str!("../../data/word/en/dictionary.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

// ── Parameters ──────────────────────────────────────────────────────────────────

const RULE_COUNTS: &[usize] = &[1_000, 10_000, 50_000, 100_000];
const DEFAULT_RULE_COUNT: usize = 10_000;

const BUILD_PROCESS_TYPES: &[ProcessType] = &[
    ProcessType::None,
    ProcessType::Delete,
    ProcessType::Fanjian,
    ProcessType::FanjianDeleteNormalize,
    ProcessType::PinYin,
];

// ── Helpers ─────────────────────────────────────────────────────────────────────

fn word_list(lang: &str) -> Vec<&str> {
    let mut words: Vec<&str> = if lang == "cn" {
        CN_WORD_LIST.lines().collect()
    } else {
        EN_WORD_LIST.lines().collect()
    };
    words.sort_unstable();
    words
}

fn build_literal_map(lang: &str, size: usize, match_scenario: bool) -> HashMap<u32, String> {
    let patterns = word_list(lang);
    let mut map = HashMap::with_capacity(size);
    for i in 0..size {
        let word_idx = (i * 997) % patterns.len();
        let word = if match_scenario {
            patterns[word_idx].to_string()
        } else if lang == "cn" {
            format!("{}\u{E000}{i}", patterns[word_idx])
        } else {
            format!("__impossible_{word_idx}_match_{i}__")
        };
        map.insert((i + 1) as u32, word);
    }
    map
}

fn build_shaped_map(lang: &str, size: usize, shape: &str) -> HashMap<u32, String> {
    let patterns = word_list(lang);
    let mut map = HashMap::with_capacity(size);
    for i in 0..size {
        let idx = (i * 997) % patterns.len();
        let shaped = match shape {
            "literal" => patterns[idx].to_string(),
            "and" => {
                let a = patterns[idx];
                let b = patterns[(idx + 101) % patterns.len()];
                let c = patterns[(idx + 211) % patterns.len()];
                format!("{a}&{b}&{c}")
            }
            "not" => format!("{}~__never_block_{i}__", patterns[idx]),
            _ => unreachable!("unknown rule shape: {shape}"),
        };
        map.insert((i + 1) as u32, shaped);
    }
    map
}

fn build_mixed_script_map(size: usize) -> HashMap<u32, String> {
    let mut en: Vec<&str> = EN_WORD_LIST
        .lines()
        .filter(|w| w.is_ascii() && !w.is_empty())
        .collect();
    let mut cn: Vec<&str> = CN_WORD_LIST
        .lines()
        .filter(|w| !w.is_ascii() && !w.is_empty())
        .collect();
    en.sort_unstable();
    cn.sort_unstable();

    let mut map = HashMap::with_capacity(size);
    for i in 0..size {
        let word = if i % 2 == 0 {
            en[(i * 997) % en.len()]
        } else {
            cn[(i * 991) % cn.len()]
        };
        map.insert((i + 1) as u32, word.to_string());
    }
    map
}

fn build_multi_process_table(size: usize) -> HashMap<ProcessType, HashMap<u32, String>> {
    let slice = (size / 4).max(1);
    HashMap::from([
        (ProcessType::None, build_literal_map("en", slice, true)),
        (ProcessType::Delete, build_literal_map("en", slice, true)),
        (ProcessType::Fanjian, build_literal_map("cn", slice, true)),
        (
            ProcessType::FanjianDeleteNormalize,
            build_literal_map("cn", size - slice * 3, true),
        ),
    ])
}

fn wrap_table(
    pt: ProcessType,
    map: HashMap<u32, String>,
) -> HashMap<ProcessType, HashMap<u32, String>> {
    HashMap::from([(pt, map)])
}

// ── 1. Build ────────────────────────────────────────────────────────────────────
// Question: How fast is SimpleMatcher::new(), and what drives construction cost?

mod build {
    use super::*;

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn by_size(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_literal_map("en", size, true));
        bencher.bench_local(|| {
            let _ = black_box(SimpleMatcher::new(&table).unwrap());
        });
    }

    #[divan::bench(args = BUILD_PROCESS_TYPES, max_time = 5)]
    fn by_process_type(bencher: Bencher, pt: ProcessType) {
        let table = wrap_table(pt, build_literal_map("cn", DEFAULT_RULE_COUNT, true));
        bencher.bench_local(|| {
            let _ = black_box(SimpleMatcher::new(&table).unwrap());
        });
    }

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn multi_process_type(bencher: Bencher, size: usize) {
        let table = build_multi_process_table(size);
        bencher.bench_local(|| {
            let _ = black_box(SimpleMatcher::new(&table).unwrap());
        });
    }
}

// ── 2. Search Mode ──────────────────────────────────────────────────────────────
// Question: How do the two SearchMode fast paths compare in throughput?
//
// AllSimple: PT=None, all literals  (bypasses state tracking entirely)
// General:   4 PTs via build_multi_process_table (full state machine)

mod search_mode {
    use super::*;

    mod all_simple {
        use super::*;

        #[divan::bench(max_time = 5)]
        fn is_match(bencher: Bencher) {
            let table = wrap_table(
                ProcessType::None,
                build_literal_map("en", DEFAULT_RULE_COUNT, true),
            );
            let matcher = SimpleMatcher::new(&table).unwrap();
            let haystack = EN_HAYSTACK;
            bencher.counter(BytesCount::new(haystack.len())).bench(|| {
                for line in haystack.lines() {
                    let _ = black_box(matcher.is_match(line));
                }
            });
        }

        #[divan::bench(max_time = 5)]
        fn process(bencher: Bencher) {
            let table = wrap_table(
                ProcessType::None,
                build_literal_map("en", DEFAULT_RULE_COUNT, true),
            );
            let matcher = SimpleMatcher::new(&table).unwrap();
            let haystack = EN_HAYSTACK;
            bencher.counter(BytesCount::new(haystack.len())).bench(|| {
                for line in haystack.lines() {
                    let _ = black_box(matcher.process(line));
                }
            });
        }
    }

    mod general {
        use super::*;

        #[divan::bench(max_time = 5)]
        fn is_match(bencher: Bencher) {
            let table = build_multi_process_table(DEFAULT_RULE_COUNT);
            let matcher = SimpleMatcher::new(&table).unwrap();
            let haystack = EN_HAYSTACK;
            bencher.counter(BytesCount::new(haystack.len())).bench(|| {
                for line in haystack.lines() {
                    let _ = black_box(matcher.is_match(line));
                }
            });
        }

        #[divan::bench(max_time = 5)]
        fn process(bencher: Bencher) {
            let table = build_multi_process_table(DEFAULT_RULE_COUNT);
            let matcher = SimpleMatcher::new(&table).unwrap();
            let haystack = EN_HAYSTACK;
            bencher.counter(BytesCount::new(haystack.len())).bench(|| {
                for line in haystack.lines() {
                    let _ = black_box(matcher.process(line));
                }
            });
        }
    }
}

// ── 3. Match vs No-Match ────────────────────────────────────────────────────────
// Question: What's the throughput difference when patterns match vs. don't match?

mod match_vs_nomatch {
    use super::*;

    #[divan::bench(max_time = 5)]
    fn is_match_hit(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, true),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(max_time = 5)]
    fn is_match_miss(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, false),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(max_time = 5)]
    fn process_hit(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, true),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }

    #[divan::bench(max_time = 5)]
    fn process_miss(bencher: Bencher) {
        let table = wrap_table(
            ProcessType::None,
            build_literal_map("en", DEFAULT_RULE_COUNT, false),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }
}

// ── 4. Scaling ──────────────────────────────────────────────────────────────────
// Question: How does throughput scale with rule count?

mod scaling {
    use super::*;

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn is_match_en(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_literal_map("en", size, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn is_match_cn(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_literal_map("cn", size, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = CN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn process_en(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_literal_map("en", size, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn process_cn(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_literal_map("cn", size, true));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = CN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }
}

// ── 5. Text Transform ───────────────────────────────────────────────────────────
// Question: How expensive is each text transformation step in isolation?

mod text_transform {
    use super::*;

    const CN_TRANSFORMS: &[ProcessType] = &[
        ProcessType::Fanjian,
        ProcessType::Delete,
        ProcessType::Normalize,
        ProcessType::PinYin,
        ProcessType::PinYinChar,
    ];

    const EN_TRANSFORMS: &[ProcessType] = &[ProcessType::Delete, ProcessType::Normalize];

    #[divan::bench(args = CN_TRANSFORMS, max_time = 5)]
    fn cn(bencher: Bencher, pt: ProcessType) {
        let haystack = CN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(text_process(pt, line));
            }
        });
    }

    #[divan::bench(args = EN_TRANSFORMS, max_time = 5)]
    fn en(bencher: Bencher, pt: ProcessType) {
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(text_process(pt, line));
            }
        });
    }
}

// ── 6. Rule Complexity ──────────────────────────────────────────────────────────
// Question: How do rule shape and mixed-script patterns affect throughput?

mod rule_complexity {
    use super::*;

    const SHAPES: &[&str] = &["literal", "and", "not"];

    #[divan::bench(args = SHAPES, max_time = 5)]
    fn shape_is_match(bencher: Bencher, shape: &str) {
        let table = wrap_table(
            ProcessType::None,
            build_shaped_map("en", DEFAULT_RULE_COUNT, shape),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }

    #[divan::bench(args = SHAPES, max_time = 5)]
    fn shape_process(bencher: Bencher, shape: &str) {
        let table = wrap_table(
            ProcessType::None,
            build_shaped_map("en", DEFAULT_RULE_COUNT, shape),
        );
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = EN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.process(line));
            }
        });
    }

    #[divan::bench(args = RULE_COUNTS, max_time = 5)]
    fn mixed_scripts(bencher: Bencher, size: usize) {
        let table = wrap_table(ProcessType::None, build_mixed_script_map(size));
        let matcher = SimpleMatcher::new(&table).unwrap();
        let haystack = CN_HAYSTACK;
        bencher.counter(BytesCount::new(haystack.len())).bench(|| {
            for line in haystack.lines() {
                let _ = black_box(matcher.is_match(line));
            }
        });
    }
}

fn main() {
    println!("Default rule count: {DEFAULT_RULE_COUNT}");
    divan::main()
}
