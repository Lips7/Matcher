//! Profiling target: SimpleMatcher construction.
//!
//! Attach Instruments / perf to this binary for flame graphs of the build phase:
//! ```sh
//! cargo run --profile profiling --example profile_build -p matcher_rs
//! ```
//!
//! Env vars:
//!   RULES=10000   Number of rules (default: 10000)
//!   DICT=en       Word list language: cn | en (default: en)
//!   PT=fdn        ProcessType shorthand (default: fdn)
//!   ITERS=50      Number of build iterations (default: 50)

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Instant;

use matcher_rs::{ProcessType, SimpleMatcher};

const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");
const EN_WORD_LIST: &str = include_str!("../../data/word/en/dictionary.txt");

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn parse_process_type(s: &str) -> ProcessType {
    match s {
        "none" => ProcessType::None,
        "fanjian" => ProcessType::Fanjian,
        "delete" => ProcessType::Delete,
        "norm" => ProcessType::Normalize,
        "dn" => ProcessType::DeleteNormalize,
        "fdn" => ProcessType::FanjianDeleteNormalize,
        "pinyin" => ProcessType::PinYin,
        "pychar" => ProcessType::PinYinChar,
        other => panic!(
            "Unknown PT shorthand: {other}. Use: none|fanjian|delete|norm|dn|fdn|pinyin|pychar"
        ),
    }
}

fn word_list(lang: &str) -> Vec<&'static str> {
    let raw = if lang == "cn" {
        CN_WORD_LIST
    } else {
        EN_WORD_LIST
    };
    let mut words: Vec<&str> = raw.lines().collect();
    words.sort_unstable();
    words
}

fn build_rule_map(lang: &str, size: usize) -> HashMap<u32, String> {
    let patterns = word_list(lang);
    let mut map = HashMap::with_capacity(size);
    for i in 0..size {
        let idx = (i * 997) % patterns.len();
        map.insert((i + 1) as u32, patterns[idx].to_string());
    }
    map
}

fn main() {
    let rules: usize = parse_env("RULES", 10_000);
    let lang = env_or("DICT", "en");
    let pt = parse_process_type(&env_or("PT", "fdn"));
    let iters: usize = parse_env("ITERS", 50);

    println!("profile_build");
    println!("  rules={rules}, lang={lang}, pt={pt}, iters={iters}");

    let map = build_rule_map(&lang, rules);
    let mut table = HashMap::new();
    table.insert(pt, map);

    // Warmup: initialize OnceLock transform step caches
    let _ = black_box(SimpleMatcher::new(&table));

    let start = Instant::now();
    for _ in 0..iters {
        black_box(SimpleMatcher::new(&table));
    }
    let elapsed = start.elapsed();

    println!("  total: {elapsed:?}");
    println!("  per-build: {:?}", elapsed / iters as u32);
}
