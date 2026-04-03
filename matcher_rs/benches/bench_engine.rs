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
#[cfg(feature = "harry")]
use matcher_rs::HarryMatcher;
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

const ALL_ENGINES: &[Engine] = &[Engine::AcDfa, Engine::DaacBytewise, Engine::DaacCharwise];
#[cfg(feature = "harry")]
const ALL_ENGINES_WITH_HARRY: &[Engine] = &[
    Engine::AcDfa,
    Engine::DaacBytewise,
    Engine::DaacCharwise,
    Engine::Harry,
];
#[cfg(not(feature = "harry"))]
const ALL_ENGINES_WITH_HARRY: &[Engine] = ALL_ENGINES;

enum BuiltEngine {
    AcDfa(AhoCorasick),
    DaacBytewise(DoubleArrayAhoCorasick<u32>),
    DaacCharwise(CharwiseDoubleArrayAhoCorasick<u32>),
    #[cfg(feature = "harry")]
    Harry(Box<HarryMatcher>),
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

/// Generates `n` patterns where `cjk_pct`% are CJK and the rest are ASCII.
fn patterns_with_cjk_pct(n: usize, cjk_pct: u8) -> Vec<String> {
    let cjk_count = (n as f64 * cjk_pct as f64 / 100.0).round() as usize;
    let mut v = ascii_patterns(n - cjk_count);
    v.extend(cjk_patterns(cjk_count));
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

// ── Memory report ──────────────────────────────────────────────────────────────

fn heap_bytes(engine: &BuiltEngine) -> usize {
    match engine {
        BuiltEngine::AcDfa(ac) => ac.memory_usage(),
        BuiltEngine::DaacBytewise(ac) => ac.heap_bytes(),
        BuiltEngine::DaacCharwise(ac) => ac.heap_bytes(),
        #[cfg(feature = "harry")]
        BuiltEngine::Harry(_) => 0, // no introspection API yet
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
    ALL_ENGINES_WITH_HARRY,
    [500usize, 2000, 10000]
);
define_build_bench!(
    build_cjk,
    cjk_patterns,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 2000, 10000]
);
define_build_bench!(
    build_mixed,
    mixed_patterns,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 2000, 10000]
);

// ── Search benchmarks ─────────────────────────────────────────────────────────

macro_rules! define_search_bench {
    ($mod_name:ident, $prep_fn:ident, $haystack:expr, $engines:expr, [$($size:expr),+ $(,)?]) => {
        mod $mod_name {
            use super::*;

            #[divan::bench(args = $engines, consts = [$($size),+], max_time = 3)]
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
    ALL_ENGINES_WITH_HARRY,
    [
        500usize, 1000, 1500, 2000, 3000, 5000, 6000, 7000, 8000, 10000, 50000
    ]
);
define_search_bench!(
    search_ascii_cn,
    ascii_patterns,
    CN_HAYSTACK,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 1000, 2000, 5000, 10000, 50000]
);
define_search_bench!(
    search_cjk_cn,
    cjk_patterns,
    CN_HAYSTACK,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 1000, 2000, 5000, 10000, 50000]
);
define_search_bench!(
    search_cjk_en,
    cjk_patterns,
    EN_HAYSTACK,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 1000, 5000, 10000, 50000]
);
define_search_bench!(
    search_mixed_en,
    mixed_patterns,
    EN_HAYSTACK,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 1000, 5000, 10000, 50000]
);
define_search_bench!(
    search_mixed_cn,
    mixed_patterns,
    CN_HAYSTACK,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 1000, 5000, 10000, 50000]
);

// ── is_match benchmarks ───────────────────────────────────────────────────────

macro_rules! define_is_match_bench {
    ($mod_name:ident, $prep_fn:ident, $haystack:expr, $engines:expr, [$($size:expr),+ $(,)?]) => {
        mod $mod_name {
            use super::*;

            #[divan::bench(args = $engines, consts = [$($size),+], max_time = 3)]
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
    ALL_ENGINES_WITH_HARRY,
    [500usize, 1000, 1500, 2000, 3000, 5000, 10000, 50000]
);
define_is_match_bench!(
    is_match_ascii_cn,
    ascii_patterns,
    CN_HAYSTACK,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 1000, 2000, 5000, 10000, 50000]
);
define_is_match_bench!(
    is_match_cjk_cn,
    cjk_patterns,
    CN_HAYSTACK,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 1000, 2000, 5000, 10000, 50000]
);
define_is_match_bench!(
    is_match_cjk_en,
    cjk_patterns,
    EN_HAYSTACK,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 2000, 10000, 50000]
);
define_is_match_bench!(
    is_match_mixed_cn,
    mixed_patterns,
    CN_HAYSTACK,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 2000, 10000, 50000]
);
define_is_match_bench!(
    is_match_mixed_en,
    mixed_patterns,
    EN_HAYSTACK,
    ALL_ENGINES_WITH_HARRY,
    [500usize, 2000, 10000, 50000]
);

// ── Pattern-mix benchmark ─────────────────────────────────────────────────────
//
// Measures how all three engines respond as the fraction of CJK patterns grows.
// N is fixed at DENSITY_PATTERN_COUNT (2,000); only the pattern mix varies.
//
// In production, the `all_ascii` guard selects AcDfa only when cjk_pct == 0.
// This benchmark deliberately builds AcDfa from mixed patterns to measure the
// raw degradation curve and confirm where DaacBytewise takes over.
//
// Run with:
//   cargo bench -p matcher_rs --bench bench_engine -- pattern_mix_

/// CJK pattern percentages to sweep for the pattern-mix benchmark.
const PATTERN_MIX_CJK_PCTS: &[u8] = &[0, 10, 20, 50, 60, 80, 100];

mod pattern_mix_en {
    use super::*;

    #[divan::bench(args = PATTERN_MIX_CJK_PCTS, max_time = 3)]
    fn ac_dfa(bencher: Bencher, cjk_pct: &u8) {
        let patterns = patterns_with_cjk_pct(DENSITY_PATTERN_COUNT, *cjk_pct);
        let engine = build_engine(Engine::AcDfa, &patterns);
        bencher
            .counter(BytesCount::new(EN_HAYSTACK.len()))
            .bench(|| {
                for line in EN_HAYSTACK.lines() {
                    let _ = black_box(count_overlapping(&engine, line));
                }
            });
    }

    #[divan::bench(args = PATTERN_MIX_CJK_PCTS, max_time = 3)]
    fn bytewise(bencher: Bencher, cjk_pct: &u8) {
        let patterns = patterns_with_cjk_pct(DENSITY_PATTERN_COUNT, *cjk_pct);
        let engine = build_engine(Engine::DaacBytewise, &patterns);
        bencher
            .counter(BytesCount::new(EN_HAYSTACK.len()))
            .bench(|| {
                for line in EN_HAYSTACK.lines() {
                    let _ = black_box(count_overlapping(&engine, line));
                }
            });
    }

    #[divan::bench(args = PATTERN_MIX_CJK_PCTS, max_time = 3)]
    fn charwise(bencher: Bencher, cjk_pct: &u8) {
        let patterns = patterns_with_cjk_pct(DENSITY_PATTERN_COUNT, *cjk_pct);
        let engine = build_engine(Engine::DaacCharwise, &patterns);
        bencher
            .counter(BytesCount::new(EN_HAYSTACK.len()))
            .bench(|| {
                for line in EN_HAYSTACK.lines() {
                    let _ = black_box(count_overlapping(&engine, line));
                }
            });
    }
}

mod pattern_mix_cn {
    use super::*;

    #[divan::bench(args = PATTERN_MIX_CJK_PCTS, max_time = 3)]
    fn ac_dfa(bencher: Bencher, cjk_pct: &u8) {
        let patterns = patterns_with_cjk_pct(DENSITY_PATTERN_COUNT, *cjk_pct);
        let engine = build_engine(Engine::AcDfa, &patterns);
        bencher
            .counter(BytesCount::new(CN_HAYSTACK.len()))
            .bench(|| {
                for line in CN_HAYSTACK.lines() {
                    let _ = black_box(count_overlapping(&engine, line));
                }
            });
    }

    #[divan::bench(args = PATTERN_MIX_CJK_PCTS, max_time = 3)]
    fn bytewise(bencher: Bencher, cjk_pct: &u8) {
        let patterns = patterns_with_cjk_pct(DENSITY_PATTERN_COUNT, *cjk_pct);
        let engine = build_engine(Engine::DaacBytewise, &patterns);
        bencher
            .counter(BytesCount::new(CN_HAYSTACK.len()))
            .bench(|| {
                for line in CN_HAYSTACK.lines() {
                    let _ = black_box(count_overlapping(&engine, line));
                }
            });
    }

    #[divan::bench(args = PATTERN_MIX_CJK_PCTS, max_time = 3)]
    fn charwise(bencher: Bencher, cjk_pct: &u8) {
        let patterns = patterns_with_cjk_pct(DENSITY_PATTERN_COUNT, *cjk_pct);
        let engine = build_engine(Engine::DaacCharwise, &patterns);
        bencher
            .counter(BytesCount::new(CN_HAYSTACK.len()))
            .bench(|| {
                for line in CN_HAYSTACK.lines() {
                    let _ = black_box(count_overlapping(&engine, line));
                }
            });
    }
}

// ── Density dispatch benchmark ────────────────────────────────────────────────
//
// Measures bytewise vs charwise throughput across synthetic text at varying
// multi-byte density. Use the crossover point to calibrate CHARWISE_DENSITY_THRESHOLD
// in `simple_matcher/engine.rs`.
//
// CJK char fraction → approximate byte density (cont_bytes / total_bytes):
//   0%  →  0.000    10% → 0.167    20% → 0.286    30% → 0.375
//  40%  →  0.444    50% → 0.500    60% → 0.545    75% → 0.600   100% → 0.667
//
// Run with:
//   cargo bench -p matcher_rs --bench bench_engine -- density_

/// Generates synthetic text with `cjk_pct`% CJK characters (each 3-byte UTF-8),
/// the rest ASCII 'a', evenly interleaved over approximately `target_bytes` total.
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

const DENSITY_TARGET_BYTES: usize = 200_000;
const DENSITY_PATTERN_COUNT: usize = 2_000;
/// CJK character percentages to sweep — maps to byte densities shown above.
const DENSITY_CJK_PCTS: &[u8] = &[0, 10, 20, 30, 40, 50, 60, 75, 100];

mod density_dispatch {
    use super::*;

    #[divan::bench(args = DENSITY_CJK_PCTS, max_time = 3)]
    fn ac_dfa(bencher: Bencher, cjk_pct: &u8) {
        let patterns = mixed_patterns(DENSITY_PATTERN_COUNT);
        let engine = build_engine(Engine::AcDfa, &patterns);
        let text = synthetic_text(*cjk_pct, DENSITY_TARGET_BYTES);
        bencher
            .counter(BytesCount::new(text.len()))
            .bench(|| black_box(count_overlapping(&engine, &text)));
    }

    #[divan::bench(args = DENSITY_CJK_PCTS, max_time = 3)]
    fn bytewise(bencher: Bencher, cjk_pct: &u8) {
        let patterns = mixed_patterns(DENSITY_PATTERN_COUNT);
        let engine = build_engine(Engine::DaacBytewise, &patterns);
        let text = synthetic_text(*cjk_pct, DENSITY_TARGET_BYTES);
        bencher
            .counter(BytesCount::new(text.len()))
            .bench(|| black_box(count_overlapping(&engine, &text)));
    }

    #[divan::bench(args = DENSITY_CJK_PCTS, max_time = 3)]
    fn charwise(bencher: Bencher, cjk_pct: &u8) {
        let patterns = mixed_patterns(DENSITY_PATTERN_COUNT);
        let engine = build_engine(Engine::DaacCharwise, &patterns);
        let text = synthetic_text(*cjk_pct, DENSITY_TARGET_BYTES);
        bencher
            .counter(BytesCount::new(text.len()))
            .bench(|| black_box(count_overlapping(&engine, &text)));
    }
}

fn main() {
    if env::args().any(|arg| arg == "--memory-report") {
        print_memory_report();
        return;
    }
    divan::main();
}
