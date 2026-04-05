/// Regression benchmarks for raw automaton engines at key dispatch operating points.
///
/// Focused set (~280 configs) covering the dispatch thresholds identified by
/// the characterization tool (`characterize_engines` example). Run regularly
/// to catch regressions; use the characterization tool for full-matrix sweeps.
///
/// Run with:
///   cargo bench -p matcher_rs --bench bench_engine
///   cargo bench -p matcher_rs --bench bench_engine -- dispatch_search
///   cargo bench -p matcher_rs --bench bench_engine -- dispatch_is_match
///   cargo bench -p matcher_rs --bench bench_engine -- build_
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use daachorse::{
    CharwiseDoubleArrayAhoCorasickBuilder, DoubleArrayAhoCorasick, DoubleArrayAhoCorasickBuilder,
    MatchKind as DaacMatchKind, charwise::CharwiseDoubleArrayAhoCorasick,
};
use divan::Bencher;
use divan::counter::BytesCount;
#[cfg(feature = "harry")]
use matcher_rs::HarryMatcher;
use std::collections::HashSet;
use std::env;
use std::hint::black_box;

const EN_WORD_LIST: &str = include_str!("../../data/word/en/dictionary.txt");
const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");

// ── Engine types ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
enum Engine {
    AcDfa,
    DaacBytewise,
    DaacCharwise,
    #[cfg(feature = "harry")]
    Harry,
}

impl std::fmt::Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Engine::AcDfa => write!(f, "ac_dfa"),
            Engine::DaacBytewise => write!(f, "daac_byte"),
            Engine::DaacCharwise => write!(f, "daac_char"),
            #[cfg(feature = "harry")]
            Engine::Harry => write!(f, "harry"),
        }
    }
}

#[cfg(feature = "harry")]
const ALL_ENGINES: &[Engine] = &[
    Engine::AcDfa,
    Engine::DaacBytewise,
    Engine::DaacCharwise,
    Engine::Harry,
];
#[cfg(not(feature = "harry"))]
const ALL_ENGINES: &[Engine] = &[Engine::AcDfa, Engine::DaacBytewise, Engine::DaacCharwise];

enum BuiltEngine {
    AcDfa(AhoCorasick),
    DaacBytewise(DoubleArrayAhoCorasick<u32>),
    DaacCharwise(CharwiseDoubleArrayAhoCorasick<u32>),
    #[cfg(feature = "harry")]
    Harry(Box<HarryMatcher>),
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

// ── Engine construction & measurement ────────────────────────────────────────

fn build_engine(engine: Engine, patterns: &[String]) -> BuiltEngine {
    let strs: Vec<&str> = patterns.iter().map(String::as_str).collect();
    match engine {
        Engine::AcDfa => BuiltEngine::AcDfa(
            AhoCorasickBuilder::new()
                .kind(Some(AhoCorasickKind::DFA))
                .build(&strs)
                .unwrap(),
        ),
        Engine::DaacBytewise => BuiltEngine::DaacBytewise(
            DoubleArrayAhoCorasickBuilder::new()
                .match_kind(DaacMatchKind::Standard)
                .build(&strs)
                .unwrap(),
        ),
        Engine::DaacCharwise => BuiltEngine::DaacCharwise(
            CharwiseDoubleArrayAhoCorasickBuilder::new()
                .match_kind(DaacMatchKind::Standard)
                .build(&strs)
                .unwrap(),
        ),
        #[cfg(feature = "harry")]
        Engine::Harry => {
            let patvals: Vec<(&str, u32)> = patterns
                .iter()
                .enumerate()
                .map(|(i, p)| (p.as_str(), i as u32))
                .collect();
            BuiltEngine::Harry(Box::new(HarryMatcher::build(&patvals).expect(
                "harry build requires ≥64 patterns with at least one length-≥2 pattern",
            )))
        }
    }
}

#[inline(always)]
fn count_overlapping(engine: &BuiltEngine, text: &str) -> usize {
    match engine {
        BuiltEngine::AcDfa(ac) => ac.find_overlapping_iter(text).count(),
        BuiltEngine::DaacBytewise(ac) => ac.find_overlapping_iter(text).count(),
        BuiltEngine::DaacCharwise(ac) => ac.find_overlapping_iter(text).count(),
        #[cfg(feature = "harry")]
        BuiltEngine::Harry(matcher) => {
            let mut count = 0usize;
            matcher.for_each_match_value(text, |_| {
                count += 1;
                false
            });
            count
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
        BuiltEngine::Harry(matcher) => matcher.is_match(text),
    }
}

// ── Memory report ────────────────────────────────────────────────────────────

fn heap_bytes(engine: &BuiltEngine) -> usize {
    match engine {
        BuiltEngine::AcDfa(ac) => ac.memory_usage(),
        BuiltEngine::DaacBytewise(ac) => ac.heap_bytes(),
        BuiltEngine::DaacCharwise(ac) => ac.heap_bytes(),
        #[cfg(feature = "harry")]
        BuiltEngine::Harry(m) => m.heap_bytes(),
    }
}

fn print_memory_report() {
    println!("\n=== Memory report (heap bytes) ===");
    for &cjk_pct in &[0u8, 50, 100] {
        for &size in &[500usize, 2_000, 10_000, 25_000, 50_000] {
            let patterns = patterns_with_cjk_pct(size, cjk_pct);
            for &engine in ALL_ENGINES {
                #[cfg(feature = "harry")]
                if matches!(engine, Engine::Harry) && (cjk_pct > 0 || size < 64) {
                    continue;
                }
                let built = build_engine(engine, &patterns);
                println!(
                    "  {engine:<12} cjk={cjk_pct:>3}% n={size:<6} -> {} bytes",
                    heap_bytes(&built)
                );
            }
        }
    }
}

// ── Synthetic text ───────────────────────────────────────────────────────────

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

const TARGET_BYTES: usize = 200_000;

// ── Dispatch regression config ───────────────────────────────────────────────
//
// Key operating points covering dispatch thresholds:
// - Pure ASCII text (DFA territory)
// - Crossover zone (~30% CJK)
// - CJK-dominant (charwise territory)
// - Pure CJK (Harry territory on is_match)
// - Mixed patterns (no DFA in production)

#[derive(Clone, Copy)]
struct DispatchConfig {
    pat_cjk_pct: u8,
    text_cjk_pct: u8,
}

impl std::fmt::Display for DispatchConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "p{}_t{}", self.pat_cjk_pct, self.text_cjk_pct)
    }
}

const DISPATCH_CONFIGS: &[DispatchConfig] = &[
    DispatchConfig {
        pat_cjk_pct: 0,
        text_cjk_pct: 0,
    },
    DispatchConfig {
        pat_cjk_pct: 0,
        text_cjk_pct: 30,
    },
    DispatchConfig {
        pat_cjk_pct: 0,
        text_cjk_pct: 70,
    },
    DispatchConfig {
        pat_cjk_pct: 0,
        text_cjk_pct: 100,
    },
    DispatchConfig {
        pat_cjk_pct: 50,
        text_cjk_pct: 0,
    },
    DispatchConfig {
        pat_cjk_pct: 50,
        text_cjk_pct: 50,
    },
    DispatchConfig {
        pat_cjk_pct: 50,
        text_cjk_pct: 100,
    },
];

// ── Build benchmarks ─────────────────────────────────────────────────────────

macro_rules! define_build_bench {
    ($mod_name:ident, $prep_fn:ident, $engines:expr, [$($size:expr),+ $(,)?]) => {
        mod $mod_name {
            use super::*;

            #[divan::bench(args = $engines, consts = [$($size),+], max_time = 3, sample_count = 30)]
            fn build<const N: usize>(bencher: Bencher, engine: &Engine) {
                let patterns = $prep_fn(N);
                bencher.bench_local(|| {
                    black_box(build_engine(*engine, &patterns));
                });
            }
        }
    };
}

define_build_bench!(
    build_ascii,
    ascii_patterns,
    ALL_ENGINES,
    [500usize, 2000, 10000]
);
define_build_bench!(
    build_cjk,
    cjk_patterns,
    ALL_ENGINES,
    [500usize, 2000, 10000]
);

// ── Dispatch regression benchmarks ───────────────────────────────────────────

macro_rules! define_dispatch_bench {
    ($mod_name:ident, $measure_fn:ident) => {
        mod $mod_name {
            use super::*;

            #[divan::bench(
                                                args = DISPATCH_CONFIGS,
                                                consts = [500usize, 2000, 10000, 25000, 50000],
                                                max_time = 3,
                                            )]
            fn ac_dfa<const N: usize>(bencher: Bencher, cfg: &DispatchConfig) {
                let patterns = patterns_with_cjk_pct(N, cfg.pat_cjk_pct);
                let engine = build_engine(Engine::AcDfa, &patterns);
                let text = synthetic_text(cfg.text_cjk_pct, TARGET_BYTES);
                bencher
                    .counter(BytesCount::new(text.len()))
                    .bench(|| black_box($measure_fn(&engine, &text)));
            }

            #[divan::bench(
                                                args = DISPATCH_CONFIGS,
                                                consts = [500usize, 2000, 10000, 25000, 50000],
                                                max_time = 3,
                                            )]
            fn daac_bytewise<const N: usize>(bencher: Bencher, cfg: &DispatchConfig) {
                let patterns = patterns_with_cjk_pct(N, cfg.pat_cjk_pct);
                let engine = build_engine(Engine::DaacBytewise, &patterns);
                let text = synthetic_text(cfg.text_cjk_pct, TARGET_BYTES);
                bencher
                    .counter(BytesCount::new(text.len()))
                    .bench(|| black_box($measure_fn(&engine, &text)));
            }

            #[divan::bench(
                                                args = DISPATCH_CONFIGS,
                                                consts = [500usize, 2000, 10000, 25000, 50000],
                                                max_time = 3,
                                            )]
            fn daac_charwise<const N: usize>(bencher: Bencher, cfg: &DispatchConfig) {
                let patterns = patterns_with_cjk_pct(N, cfg.pat_cjk_pct);
                let engine = build_engine(Engine::DaacCharwise, &patterns);
                let text = synthetic_text(cfg.text_cjk_pct, TARGET_BYTES);
                bencher
                    .counter(BytesCount::new(text.len()))
                    .bench(|| black_box($measure_fn(&engine, &text)));
            }

            #[cfg(feature = "harry")]
            #[divan::bench(
                                                args = DISPATCH_CONFIGS,
                                                consts = [500usize, 2000, 10000, 25000, 50000],
                                                max_time = 3,
                                            )]
            fn harry<const N: usize>(bencher: Bencher, cfg: &DispatchConfig) {
                if cfg.pat_cjk_pct > 0 || N < 64 {
                    return;
                }
                let patterns = patterns_with_cjk_pct(N, cfg.pat_cjk_pct);
                let engine = build_engine(Engine::Harry, &patterns);
                let text = synthetic_text(cfg.text_cjk_pct, TARGET_BYTES);
                bencher
                    .counter(BytesCount::new(text.len()))
                    .bench(|| black_box($measure_fn(&engine, &text)));
            }
        }
    };
}

define_dispatch_bench!(dispatch_search, count_overlapping);
define_dispatch_bench!(dispatch_is_match, engine_is_match);

fn main() {
    if env::args().any(|arg| arg == "--memory-report") {
        print_memory_report();
        return;
    }
    divan::main();
}
