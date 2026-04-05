//! Engine dispatch characterization: sweeps the full (engine × size × pat_cjk × text_cjk)
//! matrix and outputs CSV to stdout. Attach a visualization script for heatmaps.
//!
//! ```sh
//! cargo run --profile bench --example characterize_engines -p matcher_rs > dispatch.csv
//! ```
//!
//! Env vars (comma-separated lists override defaults):
//!   ENGINES=ac_dfa,daac_bytewise,daac_charwise,harry
//!   SIZES=10,50,100,500,1000,2000,5000,7000,10000,20000,50000,100000
//!   PAT_CJK=0,10,20,30,40,50,60,70,80,90,100
//!   TEXT_CJK=0,10,20,30,40,50,60,70,80,90,100
//!   MODES=search,is_match
//!   ITERS=5
//!   TEXT_BYTES=200000

use std::collections::HashSet;
use std::hint::black_box;
use std::time::Instant;

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use daachorse::{
    CharwiseDoubleArrayAhoCorasickBuilder, DoubleArrayAhoCorasick, DoubleArrayAhoCorasickBuilder,
    MatchKind as DaacMatchKind, charwise::CharwiseDoubleArrayAhoCorasick,
};
#[cfg(feature = "harry")]
use matcher_rs::HarryMatcher;

const EN_WORD_LIST: &str = include_str!("../../data/word/en/dictionary.txt");
const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");

// ── Engines ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum EngineKind {
    AcDfa,
    DaacBytewise,
    DaacCharwise,
    Harry,
}

impl EngineKind {
    fn name(self) -> &'static str {
        match self {
            Self::AcDfa => "ac_dfa",
            Self::DaacBytewise => "daac_bytewise",
            Self::DaacCharwise => "daac_charwise",
            Self::Harry => "harry",
        }
    }

    fn from_name(s: &str) -> Option<Self> {
        match s {
            "ac_dfa" => Some(Self::AcDfa),
            "daac_bytewise" => Some(Self::DaacBytewise),
            "daac_charwise" => Some(Self::DaacCharwise),
            "harry" => Some(Self::Harry),
            _ => None,
        }
    }
}

enum BuiltEngine {
    AcDfa(AhoCorasick),
    DaacBytewise(DoubleArrayAhoCorasick<u32>),
    DaacCharwise(CharwiseDoubleArrayAhoCorasick<u32>),
    #[cfg(feature = "harry")]
    Harry(Box<HarryMatcher>),
}

fn build_engine(kind: EngineKind, patterns: &[String]) -> Option<BuiltEngine> {
    let strs: Vec<&str> = patterns.iter().map(String::as_str).collect();
    match kind {
        EngineKind::AcDfa => Some(BuiltEngine::AcDfa(
            AhoCorasickBuilder::new()
                .kind(Some(AhoCorasickKind::DFA))
                .build(&strs)
                .ok()?,
        )),
        EngineKind::DaacBytewise => Some(BuiltEngine::DaacBytewise(
            DoubleArrayAhoCorasickBuilder::new()
                .match_kind(DaacMatchKind::Standard)
                .build(&strs)
                .ok()?,
        )),
        EngineKind::DaacCharwise => Some(BuiltEngine::DaacCharwise(
            CharwiseDoubleArrayAhoCorasickBuilder::new()
                .match_kind(DaacMatchKind::Standard)
                .build(&strs)
                .ok()?,
        )),
        #[cfg(feature = "harry")]
        EngineKind::Harry => {
            let patvals: Vec<(&str, u32)> = patterns
                .iter()
                .enumerate()
                .map(|(i, p)| (p.as_str(), i as u32))
                .collect();
            Some(BuiltEngine::Harry(Box::new(HarryMatcher::build(&patvals)?)))
        }
        #[cfg(not(feature = "harry"))]
        EngineKind::Harry => None,
    }
}

#[inline(always)]
fn count_overlapping(engine: &BuiltEngine, text: &str) -> usize {
    match engine {
        BuiltEngine::AcDfa(ac) => ac.find_overlapping_iter(text).count(),
        BuiltEngine::DaacBytewise(ac) => ac.find_overlapping_iter(text).count(),
        BuiltEngine::DaacCharwise(ac) => ac.find_overlapping_iter(text).count(),
        #[cfg(feature = "harry")]
        BuiltEngine::Harry(m) => {
            let mut c = 0usize;
            m.for_each_match_value(text, |_| {
                c += 1;
                false
            });
            c
        }
    }
}

#[inline(always)]
fn engine_is_match(engine: &BuiltEngine, text: &str) -> bool {
    match engine {
        BuiltEngine::AcDfa(ac) => ac.is_match(text),
        BuiltEngine::DaacBytewise(ac) => ac.find_iter(text).next().is_some(),
        BuiltEngine::DaacCharwise(ac) => ac.find_iter(text).next().is_some(),
        #[cfg(feature = "harry")]
        BuiltEngine::Harry(m) => m.is_match(text),
    }
}

// ── Pattern preparation ──────────────────────────────────────────────────────

fn sample_words(source: &str, n: usize, ascii_only: bool) -> Vec<String> {
    let mut words: Vec<&str> = source.lines().filter(|s| !s.is_empty()).collect();
    words.sort_unstable();
    words.dedup();
    if ascii_only {
        words.retain(|s| s.is_ascii());
    }
    let n = n.min(words.len());
    let mut out = Vec::with_capacity(n);
    let mut seen = HashSet::new();
    for i in 0.. {
        if out.len() >= n {
            break;
        }
        let idx = (i * 997) % words.len();
        if seen.insert(idx) {
            out.push(words[idx].to_owned());
        }
    }
    out
}

fn ascii_patterns(n: usize) -> Vec<String> {
    sample_words(EN_WORD_LIST, n, true)
}

fn cjk_patterns(n: usize) -> Vec<String> {
    let mut words: Vec<&str> = CN_WORD_LIST
        .lines()
        .filter(|s| !s.is_ascii() && !s.is_empty())
        .collect();
    words.sort_unstable();
    words.dedup();
    let n = n.min(words.len());
    let mut out = Vec::with_capacity(n);
    let mut seen = HashSet::new();
    for i in 0.. {
        if out.len() >= n {
            break;
        }
        let idx = (i * 997) % words.len();
        if seen.insert(idx) {
            out.push(words[idx].to_owned());
        }
    }
    out
}

fn patterns_with_cjk_pct(n: usize, cjk_pct: u8) -> Vec<String> {
    if cjk_pct == 0 {
        return ascii_patterns(n);
    }
    if cjk_pct == 100 {
        return cjk_patterns(n);
    }
    let cjk_count = (n as f64 * cjk_pct as f64 / 100.0).round() as usize;
    let mut v = ascii_patterns(n - cjk_count);
    v.extend(cjk_patterns(cjk_count));
    let mut seen = HashSet::new();
    v.retain(|s| seen.insert(s.clone()));
    v.truncate(n);
    v
}

fn synthetic_text(cjk_pct: u8, target_bytes: usize) -> String {
    let mut s = String::with_capacity(target_bytes + 3);
    let mut i = 0usize;
    while s.len() < target_bytes {
        if cjk_pct > 0 && i % 100 < cjk_pct as usize {
            s.push('中');
        } else {
            s.push('a');
        }
        i += 1;
    }
    s
}

// ── Env var parsing ──────────────────────────────────────────────────────────

fn parse_list_env<T: std::str::FromStr>(key: &str, defaults: &[T]) -> Vec<T>
where
    T: Clone,
{
    match std::env::var(key) {
        Ok(val) => val
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect(),
        Err(_) => defaults.to_vec(),
    }
}

fn parse_engine_env() -> Vec<EngineKind> {
    let defaults = vec![
        EngineKind::AcDfa,
        EngineKind::DaacBytewise,
        EngineKind::DaacCharwise,
        EngineKind::Harry,
    ];
    match std::env::var("ENGINES") {
        Ok(val) => val
            .split(',')
            .filter_map(|s| EngineKind::from_name(s.trim()))
            .collect(),
        Err(_) => defaults,
    }
}

fn parse_mode_env() -> Vec<String> {
    match std::env::var("MODES") {
        Ok(val) => val.split(',').map(|s| s.trim().to_string()).collect(),
        Err(_) => vec!["search".to_string(), "is_match".to_string()],
    }
}

// ── Measurement ──────────────────────────────────────────────────────────────

fn measure_search(engine: &BuiltEngine, text: &str, iters: usize) -> f64 {
    let mut times = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        black_box(count_overlapping(engine, text));
        times.push(start.elapsed().as_secs_f64());
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times[iters / 2]
}

fn measure_is_match(engine: &BuiltEngine, text: &str, iters: usize) -> f64 {
    let mut times = Vec::with_capacity(iters);
    for _ in 0..iters {
        let start = Instant::now();
        black_box(engine_is_match(engine, text));
        times.push(start.elapsed().as_secs_f64());
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    times[iters / 2]
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let engines = parse_engine_env();
    let sizes: Vec<usize> = parse_list_env(
        "SIZES",
        &[
            10, 50, 100, 500, 1000, 2000, 5000, 7000, 10000, 20000, 50000, 100000,
        ],
    );
    let pat_cjks: Vec<u8> =
        parse_list_env("PAT_CJK", &[0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100]);
    let text_cjks: Vec<u8> =
        parse_list_env("TEXT_CJK", &[0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100]);
    let modes = parse_mode_env();
    let iters: usize = std::env::var("ITERS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let text_bytes: usize = std::env::var("TEXT_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200_000);

    // Count total configs for progress
    let total: usize = engines.len() * modes.len() * sizes.len() * pat_cjks.len() * text_cjks.len();
    let mut done = 0usize;

    eprintln!(
        "characterize_engines: {} engines × {} modes × {} sizes × {} pat_cjk × {} text_cjk = {} configs, {} iters each",
        engines.len(),
        modes.len(),
        sizes.len(),
        pat_cjks.len(),
        text_cjks.len(),
        total,
        iters
    );

    // CSV header
    println!("engine,mode,n,pat_cjk,text_cjk,median_us,throughput_mbps,text_bytes");

    for &engine in &engines {
        for mode in &modes {
            for &n in &sizes {
                for &pat_cjk in &pat_cjks {
                    // Skip invalid Harry configs
                    if engine == EngineKind::Harry && (pat_cjk > 0 || n < 64) {
                        done += text_cjks.len();
                        continue;
                    }

                    let patterns = patterns_with_cjk_pct(n, pat_cjk);

                    let built = match build_engine(engine, &patterns) {
                        Some(e) => e,
                        None => {
                            done += text_cjks.len();
                            continue;
                        }
                    };

                    for &text_cjk in &text_cjks {
                        done += 1;
                        let text = synthetic_text(text_cjk, text_bytes);
                        let actual_bytes = text.len();

                        let median_s = match mode.as_str() {
                            "search" => measure_search(&built, &text, iters),
                            "is_match" => measure_is_match(&built, &text, iters),
                            _ => continue,
                        };

                        let median_us = median_s * 1_000_000.0;
                        let throughput_mbps = if median_s > 0.0 {
                            (actual_bytes as f64 / (1024.0 * 1024.0)) / median_s
                        } else {
                            0.0
                        };

                        println!(
                            "{},{},{},{},{},{:.2},{:.2},{}",
                            engine.name(),
                            mode,
                            n,
                            pat_cjk,
                            text_cjk,
                            median_us,
                            throughput_mbps,
                            actual_bytes
                        );

                        eprint!(
                            "\r[{done}/{total}] {} {} n={n} p{pat_cjk}_t{text_cjk} ... {median_us:.1}µs ({throughput_mbps:.0} MB/s)    ",
                            engine.name(),
                            mode,
                        );
                    }
                }
            }
        }
    }
    eprintln!("\ndone.");
}
