//! Profiling target: SimpleMatcher construction.
//!
//! Attach Instruments / perf to this binary for flame graphs of the build phase:
//! ```sh
//! cargo run --profile profiling --example profile_build -p matcher_rs
//! ```
//!
//! Env vars:
//!   RULES=10000   Number of rules (default: 10000)
//!   DICT=en       Pattern script: en | cn | mixed (default: en)
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
        "variant_norm" => ProcessType::VariantNorm,
        "delete" => ProcessType::Delete,
        "norm" => ProcessType::Normalize,
        "dn" => ProcessType::DeleteNormalize,
        "fdn" => ProcessType::VariantNormDeleteNormalize,
        "romanize" => ProcessType::Romanize,
        "pychar" => ProcessType::RomanizeChar,
        other => panic!(
            "Unknown PT shorthand: {other}. Use: none|variant_norm|delete|norm|dn|fdn|romanize|pychar"
        ),
    }
}

/// Returns a filtered, sorted word list.
///
/// - `"en"`:    pure ASCII words from the English dictionary
/// - `"cn"`:    pure non-ASCII words from the Chinese dictionary
/// - `"mixed"`: alternating ASCII and CJK words (guaranteed ~50/50 mix)
fn word_list(lang: &str) -> Vec<&'static str> {
    match lang {
        "cn" => {
            let mut w: Vec<&str> = CN_WORD_LIST
                .lines()
                .filter(|s| !s.is_ascii() && !s.is_empty())
                .collect();
            w.sort_unstable();
            w
        }
        "mixed" => {
            let mut en: Vec<&str> = EN_WORD_LIST
                .lines()
                .filter(|s| s.is_ascii() && !s.is_empty())
                .collect();
            let mut cn: Vec<&str> = CN_WORD_LIST
                .lines()
                .filter(|s| !s.is_ascii() && !s.is_empty())
                .collect();
            en.sort_unstable();
            cn.sort_unstable();
            let cap = en.len() + cn.len();
            let mut words = Vec::with_capacity(cap);
            let (mut ei, mut ci) = (0, 0);
            while ei < en.len() || ci < cn.len() {
                if ei < en.len() {
                    words.push(en[ei]);
                    ei += 1;
                }
                if ci < cn.len() {
                    words.push(cn[ci]);
                    ci += 1;
                }
            }
            words
        }
        _ => {
            let mut w: Vec<&str> = EN_WORD_LIST
                .lines()
                .filter(|s| s.is_ascii() && !s.is_empty())
                .collect();
            w.sort_unstable();
            w
        }
    }
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
    let (ascii_pats, non_ascii_pats) = map.values().fold((0u32, 0u32), |(a, n), v| {
        if v.is_ascii() { (a + 1, n) } else { (a, n + 1) }
    });
    println!("  patterns: {ascii_pats} ASCII, {non_ascii_pats} non-ASCII");

    let mut table = HashMap::new();
    table.insert(pt, map);

    // Warmup: initialize OnceLock transform step caches
    let _ = black_box(SimpleMatcher::new(&table).unwrap());

    let start = Instant::now();
    for _ in 0..iters {
        let _ = black_box(SimpleMatcher::new(&table).unwrap());
    }
    let elapsed = start.elapsed();

    println!("  total: {elapsed:?}");
    println!("  per-build: {:?}", elapsed / iters as u32);
}
