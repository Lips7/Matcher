//! Profiling target: SimpleMatcher search hot loop with predefined scenes.
//!
//! Attach Instruments / perf to this binary for flame graphs of the search phase.
//!
//! ```sh
//! # List available scenes:
//! cargo run --profile profiling --example profile_search -p matcher_rs -- --list
//!
//! # Run a single scene:
//! cargo run --profile profiling --example profile_search -p matcher_rs -- --scene en-search
//!
//! # Run all scenes (each for 20s by default):
//! cargo run --profile profiling --example profile_search -p matcher_rs -- --scene all
//!
//! # Override duration globally:
//! cargo run --profile profiling --example profile_search -p matcher_rs -- --scene all --seconds 5
//!
//! # Custom configuration:
//! cargo run --profile profiling --example profile_search -p matcher_rs -- \
//!     --dict en --rules 10000 --mode process --shape literal --pt none --seconds 30
//! ```

use std::collections::HashMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

use matcher_rs::{ProcessType, SimpleMatcher, SimpleResult};

const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");
const EN_WORD_LIST: &str = include_str!("../../data/word/en/dictionary.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/三体.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

// ── Scene registry ─────────────────────────────────────────────────────────────

struct Scene {
    name: &'static str,
    dict: &'static str,
    rules: usize,
    pt: &'static str,
    mode: &'static str,
    shape: &'static str,
    seconds: u64,
    description: &'static str,
}

const SCENES: &[Scene] = &[
    Scene {
        name: "en-search",
        dict: "en",
        rules: 10_000,
        pt: "none",
        mode: "process",
        shape: "literal",
        seconds: 20,
        description: "Baseline English, AllSimple fast path",
    },
    Scene {
        name: "en-is-match",
        dict: "en",
        rules: 10_000,
        pt: "none",
        mode: "is_match",
        shape: "literal",
        seconds: 20,
        description: "is_match early-exit path",
    },
    Scene {
        name: "cn-search",
        dict: "cn",
        rules: 10_000,
        pt: "none",
        mode: "process",
        shape: "literal",
        seconds: 20,
        description: "CJK charwise engine path",
    },
    Scene {
        name: "cn-transform",
        dict: "cn",
        rules: 10_000,
        pt: "fdn",
        mode: "process",
        shape: "literal",
        seconds: 20,
        description: "Full transform pipeline (VariantNorm+Delete+Normalize)",
    },
    Scene {
        name: "mixed-search",
        dict: "mixed",
        rules: 10_000,
        pt: "none",
        mode: "process",
        shape: "literal",
        seconds: 20,
        description: "Mixed-script dispatch boundary",
    },
    Scene {
        name: "en-and",
        dict: "en",
        rules: 10_000,
        pt: "none",
        mode: "process",
        shape: "and",
        seconds: 20,
        description: "AND-rule evaluation logic",
    },
    Scene {
        name: "en-not",
        dict: "en",
        rules: 10_000,
        pt: "none",
        mode: "process",
        shape: "not",
        seconds: 20,
        description: "NOT-rule veto logic",
    },
    Scene {
        name: "en-or",
        dict: "en",
        rules: 10_000,
        pt: "none",
        mode: "process",
        shape: "or",
        seconds: 20,
        description: "OR-alternative expansion",
    },
    Scene {
        name: "en-boundary",
        dict: "en",
        rules: 10_000,
        pt: "none",
        mode: "process",
        shape: "word_boundary",
        seconds: 20,
        description: "Word boundary matching",
    },
    Scene {
        name: "en-large",
        dict: "en",
        rules: 50_000,
        pt: "none",
        mode: "process",
        shape: "literal",
        seconds: 20,
        description: "Scale stress test (50K rules)",
    },
    Scene {
        name: "cn-romanize",
        dict: "cn",
        rules: 10_000,
        pt: "romanize",
        mode: "process",
        shape: "literal",
        seconds: 20,
        description: "Romanization pipeline",
    },
];

// ── Helpers ─────────────────────────────────────────────────────────────────────

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
            "or" => {
                let a = patterns[idx];
                let b = patterns[(idx + 101) % patterns.len()];
                let c = patterns[(idx + 211) % patterns.len()];
                format!("{a}|{b}|{c}")
            }
            "word_boundary" => format!("\\b{}\\b", patterns[idx]),
            other => panic!("Unknown shape: {other}. Use: literal|and|not|or|word_boundary"),
        };
        map.insert((i + 1) as u32, pattern);
    }
    map
}

// ── Scene execution ─────────────────────────────────────────────────────────────

fn run_scene(
    name: &str,
    dict: &str,
    rules: usize,
    pt_str: &str,
    mode: &str,
    shape: &str,
    seconds: u64,
) {
    let pt = parse_process_type(pt_str);

    println!("scene: {name}");
    println!(
        "  rules={rules}, dict={dict}, pt={pt}, mode={mode}, shape={shape}, seconds={seconds}"
    );

    let map = build_rule_map(dict, rules, shape);
    let (ascii_pats, non_ascii_pats) = map.values().fold((0u32, 0u32), |(a, n), v| {
        if v.is_ascii() { (a + 1, n) } else { (a, n + 1) }
    });
    println!("  patterns: {ascii_pats} ASCII, {non_ascii_pats} non-ASCII");

    let mut table = HashMap::new();
    table.insert(pt, map);

    let matcher = SimpleMatcher::new(&table).unwrap();
    let haystack = match dict {
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
            match mode {
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

// ── CLI ─────────────────────────────────────────────────────────────────────────

fn print_list() {
    println!("Available scenes:");
    println!();
    for s in SCENES {
        println!(
            "  {:<16} dict={:<5} rules={:<5} pt={:<10} mode={:<10} shape={:<15} {}",
            s.name, s.dict, s.rules, s.pt, s.mode, s.shape, s.description
        );
    }
}

fn env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn parse_env<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "--list") {
        print_list();
        return;
    }

    // Parse --seconds (global override)
    let seconds_override: Option<u64> = args
        .iter()
        .position(|a| a == "--seconds")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok());

    // Parse --scene
    let scene_arg: Option<&str> = args
        .iter()
        .position(|a| a == "--scene")
        .and_then(|i| args.get(i + 1))
        .map(String::as_str);

    if let Some(scene_spec) = scene_arg {
        let selected: Vec<&Scene> = if scene_spec == "all" {
            SCENES.iter().collect()
        } else {
            scene_spec
                .split(',')
                .map(|name| {
                    SCENES.iter().find(|s| s.name == name).unwrap_or_else(|| {
                        eprintln!("Unknown scene: {name}");
                        print_list();
                        std::process::exit(1);
                    })
                })
                .collect()
        };

        let total_seconds: u64 = selected
            .iter()
            .map(|s| seconds_override.unwrap_or(s.seconds))
            .sum();
        println!(
            "Running {} scene(s), ~{total_seconds}s total",
            selected.len()
        );
        println!();

        for (i, scene) in selected.iter().enumerate() {
            if i > 0 {
                println!();
                println!("{}", "=".repeat(60));
                println!();
            }
            run_scene(
                scene.name,
                scene.dict,
                scene.rules,
                scene.pt,
                scene.mode,
                scene.shape,
                seconds_override.unwrap_or(scene.seconds),
            );
        }
        return;
    }

    // Custom mode: check for CLI flags, fall back to env vars
    let has_custom_flags = args.iter().any(|a| {
        matches!(
            a.as_str(),
            "--dict" | "--rules" | "--mode" | "--shape" | "--pt"
        )
    });

    let get_flag = |flag: &str, env_key: &str, default: &str| -> String {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .map(String::clone)
            .unwrap_or_else(|| env_or(env_key, default))
    };

    if has_custom_flags || std::env::var("DICT").is_ok() || std::env::var("MODE").is_ok() {
        let dict = get_flag("--dict", "DICT", "en");
        let rules: usize = get_flag("--rules", "RULES", "10000")
            .parse()
            .expect("invalid --rules");
        let mode = get_flag("--mode", "MODE", "process");
        let shape = get_flag("--shape", "SHAPE", "literal");
        let pt = get_flag("--pt", "PT", "none");
        let seconds = seconds_override.unwrap_or_else(|| parse_env("SECONDS", 30));

        run_scene("custom", &dict, rules, &pt, &mode, &shape, seconds);
        return;
    }

    // Default: run en-search
    let default = &SCENES[0];
    run_scene(
        default.name,
        default.dict,
        default.rules,
        default.pt,
        default.mode,
        default.shape,
        seconds_override.unwrap_or(default.seconds),
    );
}
