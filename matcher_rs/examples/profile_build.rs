//! Profiling target: SimpleMatcher::new() construction hot loop.
//!
//! Attach Instruments / perf to this binary for flame graphs of the build
//! phase.
//!
//! ```sh
//! # Default: 10K English literal rules, 10s
//! cargo run --profile profiling --example profile_build -p matcher_rs
//!
//! # Custom:
//! cargo run --profile profiling --example profile_build -p matcher_rs -- \
//!     --dict cn --rules 50000 --pt variant_norm --seconds 15
//! ```

use std::{
    collections::HashMap,
    env,
    hint::black_box,
    time::{Duration, Instant},
};

use matcher_rs::{ProcessType, SimpleMatcher};

const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");
const EN_WORD_LIST: &str = include_str!("../../data/word/en/dictionary.txt");

fn word_list(lang: &str) -> Vec<&str> {
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

fn parse_process_type(s: &str) -> ProcessType {
    match s {
        "none" => ProcessType::None,
        "variant_norm" | "fanjian" => ProcessType::VariantNorm,
        "delete" => ProcessType::Delete,
        "norm" | "normalize" => ProcessType::Normalize,
        "dn" => ProcessType::DeleteNormalize,
        "fdn" => ProcessType::VariantNormDeleteNormalize,
        "romanize" | "pinyin" => ProcessType::Romanize,
        "pychar" | "romanize_char" => ProcessType::RomanizeChar,
        other => panic!("Unknown process type: {other}"),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let mut dict = env::var("DICT").unwrap_or_else(|_| "en".into());
    let mut rules: usize = env::var("RULES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);
    let mut pt_str = env::var("PT").unwrap_or_else(|_| "none".into());
    let mut seconds: u64 = env::var("SECONDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10);

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--dict" => {
                dict = args[i + 1].clone();
                i += 2;
            }
            "--rules" => {
                rules = args[i + 1].parse().unwrap();
                i += 2;
            }
            "--pt" => {
                pt_str = args[i + 1].clone();
                i += 2;
            }
            "--seconds" => {
                seconds = args[i + 1].parse().unwrap();
                i += 2;
            }
            other => panic!("Unknown arg: {other}. Use: --dict, --rules, --pt, --seconds"),
        }
    }

    let pt = parse_process_type(&pt_str);

    println!("profile_build: rules={rules}, dict={dict}, pt={pt}, seconds={seconds}");

    let patterns = word_list(&dict);
    let mut map = HashMap::with_capacity(rules);
    for i in 0..rules {
        let idx = (i * 997) % patterns.len();
        map.insert((i + 1) as u32, patterns[idx].to_string());
    }
    let mut table = HashMap::new();
    table.insert(pt, map);

    println!("  table ready, starting build loop...");

    let mut iterations: u64 = 0;
    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);

    while Instant::now() < deadline {
        let matcher = black_box(SimpleMatcher::new(&table).unwrap());
        black_box(&matcher);
        drop(matcher);
        iterations += 1;
    }

    let elapsed = start.elapsed();
    let per_build_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

    println!("  iterations: {iterations}");
    println!("  per-build: {per_build_ms:.2} ms");
    println!(
        "  throughput: {:.1} builds/s",
        iterations as f64 / elapsed.as_secs_f64()
    );
}
