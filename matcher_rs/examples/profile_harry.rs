//! Profiling target: HarryMatcher hot loop in isolation.
//!
//! Drives `HarryMatcher::is_match` and `HarryMatcher::for_each_match_value`
//! directly — bypassing `SimpleMatcher` and `ScanPlan` — so Instruments / samply
//! profiles are dominated by the Harry SIMD kernels and verification paths.
//!
//! ```sh
//! cargo run --profile profiling --example profile_harry -p matcher_rs
//! ```
//!
//! Env vars:
//!   RULES=10000     Number of patterns (default: 10000, min 64 for Harry to build)
//!   DICT=en         Pattern dictionary: en | cn (default: en)
//!   HAYSTACK=cn     Haystack text: en | cn (default: cn)
//!   MODE=is_match   API: is_match | for_each (default: is_match)
//!   SECONDS=30      Duration of profiling loop (default: 30)

use std::hint::black_box;
use std::time::{Duration, Instant};

use matcher_rs::HarryMatcher;

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

fn build_patterns(dict: &str, count: usize) -> Vec<(&'static str, u32)> {
    let words = word_list(dict);
    assert!(!words.is_empty(), "No words found for dict={dict}");
    (0..count)
        .map(|i| {
            let idx = (i * 997) % words.len();
            (words[idx], i as u32)
        })
        .collect()
}

fn main() {
    let rules: usize = parse_env("RULES", 10_000);
    let dict = env_or("DICT", "en");
    let haystack_lang = env_or("HAYSTACK", "cn");
    let mode = env_or("MODE", "is_match");
    let seconds: u64 = parse_env("SECONDS", 30);

    println!("profile_harry");
    println!(
        "  rules={rules}, dict={dict}, haystack={haystack_lang}, mode={mode}, seconds={seconds}"
    );

    let patterns = build_patterns(&dict, rules);
    let harry = HarryMatcher::build(&patterns.iter().map(|(s, v)| (*s, *v)).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            panic!("HarryMatcher::build failed (need >= 64 patterns with len >= 2)")
        });

    let haystack = match haystack_lang.as_str() {
        "en" => EN_HAYSTACK,
        _ => CN_HAYSTACK,
    };
    let lines: Vec<&str> = haystack.lines().collect();

    let ascii_pats = patterns.iter().filter(|(s, _)| s.is_ascii()).count();
    println!(
        "  patterns: {ascii_pats} ASCII, {} non-ASCII",
        patterns.len() - ascii_pats
    );
    println!(
        "  haystack: {} lines, {} bytes",
        lines.len(),
        haystack.len()
    );
    println!("  heap_bytes: {}", harry.heap_bytes());
    println!("  starting profiling loop...");

    let mut total_iterations: u64 = 0;
    let mut total_matches: u64 = 0;
    let mut total_bytes: u64 = 0;

    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);

    while Instant::now() < deadline {
        for line in &lines {
            match mode.as_str() {
                "is_match" => {
                    if black_box(harry.is_match(line)) {
                        total_matches += 1;
                    }
                }
                "for_each" => {
                    harry.for_each_match_value(line, |v| {
                        black_box(v);
                        false // don't early-exit, exercise full scan
                    });
                    total_matches += 1;
                }
                other => panic!("Unknown mode: {other}. Use: is_match|for_each"),
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
