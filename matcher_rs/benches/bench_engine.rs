/// Isolated head-to-head benchmark of raw automaton engines.
///
/// Measures build time, search throughput, and is_match throughput for three
/// engines across ASCII, CJK, and Mixed pattern types. Results inform the
/// `AC_DFA_PATTERN_THRESHOLD` in `simple_matcher/engine.rs`.
///
/// Run with:
///   cargo bench -p matcher_rs --bench bench_engine
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder, DoubleArrayAhoCorasick,
    DoubleArrayAhoCorasickBuilder, MatchKind as DaacMatchKind,
};
use divan::Bencher;
use divan::counter::BytesCount;
use std::collections::HashSet;
use std::env;
use std::hint::black_box;

const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/三体.txt");
const EN_WORD_LIST: &str = include_str!("../../data/word/en/dictionary.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

#[derive(Clone, Copy, Debug)]
enum Engine {
    AcDfa,
    DaacBytewise,
    DaacCharwise,
}

impl std::fmt::Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Engine::AcDfa => write!(f, "ac_dfa"),
            Engine::DaacBytewise => write!(f, "daac_byte"),
            Engine::DaacCharwise => write!(f, "daac_char"),
        }
    }
}

const ALL_ENGINES: &[Engine] = &[Engine::AcDfa, Engine::DaacBytewise, Engine::DaacCharwise];

enum BuiltEngine {
    AcDfa(AhoCorasick),
    DaacBytewise(DoubleArrayAhoCorasick<u32>),
    DaacCharwise(CharwiseDoubleArrayAhoCorasick<u32>),
}

// ── Pattern preparation ────────────────────────────────────────────────────────

fn ascii_patterns(n: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    EN_WORD_LIST
        .lines()
        .filter(|s| s.is_ascii() && !s.is_empty())
        .filter(|s| seen.insert(*s))
        .take(n)
        .map(str::to_owned)
        .collect()
}

fn cjk_patterns(n: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    CN_WORD_LIST
        .lines()
        .filter(|s| !s.is_ascii() && !s.is_empty())
        .filter(|s| seen.insert(*s))
        .take(n)
        .map(str::to_owned)
        .collect()
}

fn mixed_patterns(n: usize) -> Vec<String> {
    let half = n / 2;
    let mut v = ascii_patterns(half);
    v.extend(cjk_patterns(n - half));
    let mut seen = HashSet::new();
    v.retain(|s| seen.insert(s.clone()));
    v.truncate(n);
    v
}

// ── Engine construction ────────────────────────────────────────────────────────

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
    }
}

#[inline(always)]
fn count_overlapping(engine: &BuiltEngine, text: &str) -> usize {
    match engine {
        BuiltEngine::AcDfa(ac) => ac.find_overlapping_iter(text).count(),
        BuiltEngine::DaacBytewise(ac) => ac.find_overlapping_iter(text).count(),
        BuiltEngine::DaacCharwise(ac) => ac.find_overlapping_iter(text).count(),
    }
}

#[inline(always)]
fn engine_is_match(engine: &BuiltEngine, text: &str) -> bool {
    match engine {
        BuiltEngine::AcDfa(ac) => ac.is_match(text),
        BuiltEngine::DaacBytewise(ac) => ac.find_iter(text).next().is_some(),
        BuiltEngine::DaacCharwise(ac) => ac.find_iter(text).next().is_some(),
    }
}

// ── Memory report ──────────────────────────────────────────────────────────────

fn heap_bytes(engine: &BuiltEngine) -> usize {
    match engine {
        BuiltEngine::AcDfa(ac) => ac.memory_usage(),
        BuiltEngine::DaacBytewise(ac) => ac.heap_bytes(),
        BuiltEngine::DaacCharwise(ac) => ac.heap_bytes(),
    }
}

fn print_memory_report() {
    println!("\n=== Memory report (heap bytes) ===");
    for &kind in &["ascii", "cjk", "mixed"] {
        for &size in &[500, 1_000, 2_000, 3_000, 5_000, 10_000, 50_000] {
            let patterns = match kind {
                "ascii" => ascii_patterns(size),
                "cjk" => cjk_patterns(size),
                _ => mixed_patterns(size),
            };
            for &engine in ALL_ENGINES {
                if matches!(engine, Engine::AcDfa) && kind == "cjk" {
                    continue;
                }
                let built = build_engine(engine, &patterns);
                println!(
                    "  {engine:<12} {kind:<6} n={size:<6} -> {} bytes",
                    heap_bytes(&built)
                );
            }
        }
    }
}

// ── Build benchmarks ───────────────────────────────────────────────────────────

macro_rules! define_build_bench {
    ($mod_name:ident, $prep_fn:ident, [$($size:expr),+ $(,)?]) => {
        mod $mod_name {
            use super::*;

            #[divan::bench(args = ALL_ENGINES, consts = [$($size),+], max_time = 3, sample_count = 30)]
            fn build<const N: usize>(bencher: Bencher, engine: &Engine) {
                let patterns = $prep_fn(N);
                bencher.bench_local(|| {
                    black_box(build_engine(*engine, &patterns));
                });
            }
        }
    };
}

define_build_bench!(build_ascii, ascii_patterns, [500usize, 2000, 10000]);
define_build_bench!(build_cjk, cjk_patterns, [500usize, 2000, 10000]);
define_build_bench!(build_mixed, mixed_patterns, [500usize, 2000, 10000]);

// ── Search benchmarks ─────────────────────────────────────────────────────────

macro_rules! define_search_bench {
    ($mod_name:ident, $prep_fn:ident, $haystack:expr, [$($size:expr),+ $(,)?]) => {
        mod $mod_name {
            use super::*;

            #[divan::bench(args = ALL_ENGINES, consts = [$($size),+], max_time = 3)]
            fn search<const N: usize>(bencher: Bencher, engine: &Engine) {
                let patterns = $prep_fn(N);
                let built = build_engine(*engine, &patterns);
                let haystack = $haystack;
                let total_bytes = haystack.len();

                bencher
                    .counter(BytesCount::new(total_bytes))
                    .bench(|| {
                        for line in haystack.lines() {
                            let _ = black_box(count_overlapping(&built, line));
                        }
                    });
            }
        }
    };
}

define_search_bench!(
    search_ascii_en,
    ascii_patterns,
    EN_HAYSTACK,
    [500usize, 1000, 1500, 2000, 3000, 5000, 10000, 50000]
);
define_search_bench!(
    search_ascii_cn,
    ascii_patterns,
    CN_HAYSTACK,
    [500usize, 1000, 2000, 5000, 10000, 50000]
);
define_search_bench!(
    search_cjk_cn,
    cjk_patterns,
    CN_HAYSTACK,
    [500usize, 1000, 2000, 5000, 10000, 50000]
);
define_search_bench!(
    search_cjk_en,
    cjk_patterns,
    EN_HAYSTACK,
    [500usize, 1000, 5000, 10000, 50000]
);
define_search_bench!(
    search_mixed_en,
    mixed_patterns,
    EN_HAYSTACK,
    [500usize, 1000, 5000, 10000, 50000]
);
define_search_bench!(
    search_mixed_cn,
    mixed_patterns,
    CN_HAYSTACK,
    [500usize, 1000, 5000, 10000, 50000]
);

// ── is_match benchmarks ───────────────────────────────────────────────────────

macro_rules! define_is_match_bench {
    ($mod_name:ident, $prep_fn:ident, $haystack:expr, [$($size:expr),+ $(,)?]) => {
        mod $mod_name {
            use super::*;

            #[divan::bench(args = ALL_ENGINES, consts = [$($size),+], max_time = 3)]
            fn is_match<const N: usize>(bencher: Bencher, engine: &Engine) {
                let patterns = $prep_fn(N);
                let built = build_engine(*engine, &patterns);
                let haystack = $haystack;
                let total_bytes = haystack.len();

                bencher
                    .counter(BytesCount::new(total_bytes))
                    .bench(|| {
                        for line in haystack.lines() {
                            let _ = black_box(engine_is_match(&built, line));
                        }
                    });
            }
        }
    };
}

define_is_match_bench!(
    is_match_ascii_en,
    ascii_patterns,
    EN_HAYSTACK,
    [500usize, 1000, 1500, 2000, 3000, 5000, 10000, 50000]
);
define_is_match_bench!(
    is_match_ascii_cn,
    ascii_patterns,
    CN_HAYSTACK,
    [500usize, 1000, 2000, 5000, 10000, 50000]
);

fn main() {
    if env::args().any(|arg| arg == "--memory-report") {
        print_memory_report();
        return;
    }
    divan::main();
}
