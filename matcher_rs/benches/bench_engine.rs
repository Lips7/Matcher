/// Isolated head-to-head benchmark of raw automaton engines.
///
/// Measures build time and search throughput for four engines across
/// three pattern types (ASCII, CJK, Mixed) and four pattern counts.
/// Results inform the threshold used for automatic engine selection in
/// `simple_matcher.rs`.
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
use std::hint::black_box;

const CN_WORD_LIST: &str = include_str!("../../data/word/cn/jieba.txt");
const CN_HAYSTACK: &str = include_str!("../../data/text/cn/三体.txt");
const EN_WORD_LIST: &str = include_str!("../../data/word/en/dictionary.txt");
const EN_HAYSTACK: &str = include_str!("../../data/text/en/sherlock.txt");

const PATTERN_SIZES: &[usize] = &[100, 1_000, 10_000, 50_000];

#[derive(Clone, Copy, Debug)]
enum Engine {
    AcDfa,
    AcContiguousNfa,
    DaacBytewise,
    DaacCharwise,
}

impl std::fmt::Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Engine::AcDfa => write!(f, "ac_dfa"),
            Engine::AcContiguousNfa => write!(f, "ac_cnfa"),
            Engine::DaacBytewise => write!(f, "daac_byte"),
            Engine::DaacCharwise => write!(f, "daac_char"),
        }
    }
}

const ALL_ENGINES: &[Engine] = &[
    Engine::AcDfa,
    Engine::AcContiguousNfa,
    Engine::DaacBytewise,
    Engine::DaacCharwise,
];

enum BuiltEngine {
    AcDfa(AhoCorasick),
    AcContiguousNfa(AhoCorasick),
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

/// Interleaves ASCII and CJK words at a ~50/50 ratio.
fn mixed_patterns(n: usize) -> Vec<String> {
    let half = n / 2;
    let mut v = ascii_patterns(half);
    v.extend(cjk_patterns(n - half));
    // Dedup across the combined set.
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
        Engine::AcContiguousNfa => BuiltEngine::AcContiguousNfa(
            AhoCorasickBuilder::new()
                .kind(Some(AhoCorasickKind::ContiguousNFA))
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

/// Counts overlapping matches, used to drive the iterator in search benches.
#[inline(always)]
fn count_overlapping(engine: &BuiltEngine, text: &str) -> usize {
    match engine {
        BuiltEngine::AcDfa(ac) | BuiltEngine::AcContiguousNfa(ac) => {
            ac.find_overlapping_iter(text).count()
        }
        BuiltEngine::DaacBytewise(ac) => ac.find_overlapping_iter(text).count(),
        BuiltEngine::DaacCharwise(ac) => ac.find_overlapping_iter(text).count(),
    }
}

// ── Memory report ──────────────────────────────────────────────────────────────

fn heap_bytes(engine: &BuiltEngine) -> usize {
    match engine {
        BuiltEngine::AcDfa(ac) | BuiltEngine::AcContiguousNfa(ac) => ac.memory_usage(),
        BuiltEngine::DaacBytewise(ac) => ac.heap_bytes(),
        BuiltEngine::DaacCharwise(ac) => ac.heap_bytes(),
    }
}

fn print_memory_report() {
    println!("\n=== Memory report (heap bytes) ===");
    for &kind in &["ascii", "cjk", "mixed"] {
        for &size in PATTERN_SIZES {
            let patterns = match kind {
                "ascii" => ascii_patterns(size),
                "cjk" => cjk_patterns(size),
                _ => mixed_patterns(size),
            };
            for &engine in ALL_ENGINES {
                // DaacCharwise on ASCII-only is fine but slow; skip for brevity.
                if matches!(engine, Engine::AcDfa | Engine::AcContiguousNfa) && kind == "cjk" {
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
//
// For each pattern kind and engine, measure automaton construction time at
// each of the four pattern counts.

macro_rules! define_build_bench {
    ($mod_name:ident, $prep_fn:ident) => {
        mod $mod_name {
            use super::*;

            #[divan::bench(args = ALL_ENGINES, consts = [100usize, 1000, 10000, 50000], max_time = 10)]
            fn build<const N: usize>(bencher: Bencher, engine: &Engine) {
                let patterns = $prep_fn(N);
                bencher.bench_local(|| {
                    black_box(build_engine(*engine, &patterns));
                });
            }
        }
    };
}

define_build_bench!(build_ascii, ascii_patterns);
define_build_bench!(build_cjk, cjk_patterns);
define_build_bench!(build_mixed, mixed_patterns);

// ── Search benchmarks ─────────────────────────────────────────────────────────
//
// For each pattern kind, engine, and haystack, measure overlapping-match
// throughput (bytes/sec) at each pattern count.
//
// Automaton is built once in setup; only the search loop is timed.

macro_rules! define_search_bench {
    ($mod_name:ident, $prep_fn:ident, $haystack:expr) => {
        mod $mod_name {
            use super::*;

            #[divan::bench(args = ALL_ENGINES, consts = [100usize, 1000, 10000, 50000], max_time = 10)]
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

// ASCII patterns on English text (most natural pairing)
define_search_bench!(search_ascii_en, ascii_patterns, EN_HAYSTACK);
// ASCII patterns on Chinese text (cross-type: few matches expected)
define_search_bench!(search_ascii_cn, ascii_patterns, CN_HAYSTACK);
// CJK patterns on Chinese text
define_search_bench!(search_cjk_cn, cjk_patterns, CN_HAYSTACK);
// CJK patterns on English text (cross-type: near-zero matches)
define_search_bench!(search_cjk_en, cjk_patterns, EN_HAYSTACK);
// Mixed patterns on English text
define_search_bench!(search_mixed_en, mixed_patterns, EN_HAYSTACK);
// Mixed patterns on Chinese text
define_search_bench!(search_mixed_cn, mixed_patterns, CN_HAYSTACK);

fn main() {
    print_memory_report();
    divan::main();
}
