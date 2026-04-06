//! Profiling target: SimpleMatcher search hot loop.
//!
//! Attach Instruments / perf to this binary for flame graphs of the search phase:
//! ```sh
//! cargo run --profile profiling --example profile_search -p matcher_rs
//! ```
//!
//! Env vars:
//!   RULES=10000     Number of rules (default: 10000)
//!   DICT=en         Pattern script + haystack: en | cn | mixed (default: en)
//!   PT=none         ProcessType shorthand (default: none)
//!   MODE=process    Search API: is_match | process (default: process)
//!   SHAPE=literal   Rule shape: literal | and | not (default: literal)
//!   SECONDS=30      Duration of profiling loop (default: 30)

use std::collections::HashMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

use matcher_rs::{ProcessType, SimpleMatcher, SimpleResult};

const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");
const EN_WORD_LIST: &str = include_str!("../../data/word/en/dictionary.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/三体.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

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

fn build_rule_map(lang: &str, size: usize, shape: &str) -> HashMap<u32, String> {
    let patterns = word_list(lang);
    let mut map = HashMap::with_capacity(size);
    for i in 0..size {
        let idx = (i * 997) % patterns.len();
        let pattern = match shape {
            "literal" => patterns[idx].to_string(),
            "and" => {
                let a = patterns[idx];
                let b = patterns[(idx + 101) % patterns.len()];
                let c = patterns[(idx + 211) % patterns.len()];
                format!("{a}&{b}&{c}")
            }
            "not" => format!("{}~__never_block_{i}__", patterns[idx]),
            other => panic!("Unknown shape: {other}. Use: literal|and|not"),
        };
        map.insert((i + 1) as u32, pattern);
    }
    map
}

fn main() {
    let rules: usize = parse_env("RULES", 10_000);
    let lang = env_or("DICT", "en");
    let pt = parse_process_type(&env_or("PT", "none"));
    let mode = env_or("MODE", "process");
    let shape = env_or("SHAPE", "literal");
    let seconds: u64 = parse_env("SECONDS", 30);

    println!("profile_search");
    println!(
        "  rules={rules}, lang={lang}, pt={pt}, mode={mode}, shape={shape}, seconds={seconds}"
    );

    let map = build_rule_map(&lang, rules, &shape);
    let (ascii_pats, non_ascii_pats) = map.values().fold((0u32, 0u32), |(a, n), v| {
        if v.is_ascii() { (a + 1, n) } else { (a, n + 1) }
    });
    println!("  patterns: {ascii_pats} ASCII, {non_ascii_pats} non-ASCII");

    let mut table = HashMap::new();
    table.insert(pt, map);

    let matcher = SimpleMatcher::new(&table).unwrap();
    let haystack = match lang.as_str() {
        "cn" | "mixed" => CN_HAYSTACK,
        _ => EN_HAYSTACK,
    };
    let lines: Vec<&str> = haystack.lines().collect();

    println!(
        "  haystack: {} lines, {} bytes",
        lines.len(),
        haystack.len()
    );
    println!("  starting search loop...");

    let mut results: Vec<SimpleResult<'_>> = Vec::new();
    let mut total_iterations: u64 = 0;
    let mut total_matches: u64 = 0;
    let mut total_bytes: u64 = 0;

    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);

    while Instant::now() < deadline {
        for line in &lines {
            match mode.as_str() {
                "is_match" => {
                    if black_box(matcher.is_match(line)) {
                        total_matches += 1;
                    }
                }
                "process" => {
                    results.clear();
                    matcher.process_into(line, &mut results);
                    total_matches += results.len() as u64;
                    black_box(&results);
                }
                other => panic!("Unknown mode: {other}. Use: is_match|process"),
            }
            total_bytes += line.len() as u64;
            total_iterations += 1;
        }
    }

    let elapsed = start.elapsed();
    let mb = total_bytes as f64 / (1024.0 * 1024.0);
    let throughput = mb / elapsed.as_secs_f64();

    println!("  elapsed: {elapsed:?}");
    println!("  iterations: {total_iterations}");
    println!("  matches: {total_matches}");
    println!("  throughput: {throughput:.2} MB/s");
}
