//! Scan-engine compilation and match iteration for [`super::SimpleMatcher`].
//!
//! This module owns the Aho-Corasick automata that power Pass 1 (pattern scan) of the
//! two-pass matching pipeline. Two independent engines are compiled:
//!
//! - **Bytewise engine** ([`BytewiseMatcher`]) — scans byte-by-byte over ASCII patterns.
//!   With the `dfa` feature enabled and pattern count ≤ [`AC_DFA_PATTERN_THRESHOLD`], this
//!   uses the `aho-corasick` crate's DFA for maximum throughput. Otherwise it falls back to
//!   `daachorse`'s bytewise double-array Aho-Corasick.
//!
//! - **Charwise engine** ([`CharwiseMatcher`]) — scans character-wise using `daachorse`'s
//!   charwise automaton. Always built over the **full** pattern set so a single charwise
//!   pass covers everything on non-ASCII input. CJK characters are 3 UTF-8 bytes, so
//!   charwise does 1 state transition vs 3 for bytewise — ~1.6–1.9× faster on CJK text.
//!
//! The [`ScanPlan`] struct bundles both engines together with the [`PatternIndex`] that
//! maps raw automaton values back to rule metadata.
//!
//! # Engine selection
//!
//! [`ScanPlan::is_match`] and [`ScanPlan::for_each_match_value`] accept an `is_ascii`
//! flag (computed once per text variant by [`super::search`] via `str::is_ascii()`).
//! The charwise engine is always built, so `is_ascii` determines which engine is used:
//! bytewise for ASCII text, charwise for non-ASCII text.

use std::borrow::Cow;

#[cfg(feature = "dfa")]
use aho_corasick::{
    AhoCorasick as AcEngine, AhoCorasickBuilder, AhoCorasickKind, MatchKind as AhoCorasickMatchKind,
};
use daachorse::{
    DoubleArrayAhoCorasick, DoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
    charwise::{CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder},
};

use crate::MatcherError;

use super::rule::{DIRECT_RULE_MASK, PatternEntry, PatternIndex};

/// Non-ASCII byte density threshold for switching from bytewise to charwise engine.
///
/// Calibrated from 8,932-point characterization sweep (4 engines × 12 sizes × 11 CJK
/// densities). At ~40% CJK characters the non-ASCII byte fraction is
/// `0.4×3 / (0.4×3 + 0.6×1) ≈ 0.667`. Charwise overtakes DFA+Teddy at this crossover,
/// consistent across pattern sizes and both `search` and `is_match` modes.
pub(super) const CHARWISE_DENSITY_THRESHOLD: f32 = 0.67;

/// Computes the non-ASCII byte fraction of the full text using SIMD.
///
/// Returns a value in `[0.0, 1.0]`: 0.0 = pure ASCII, 1.0 = all non-ASCII.
/// Uses platform-specific SIMD (NEON / AVX2 / portable `std::simd`) via
/// [`super::simd::count_non_ascii_simd`]. ~2 µs for 200 KB.
#[inline(always)]
pub(super) fn text_non_ascii_density(text: &str) -> f32 {
    let bytes = text.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return 0.0;
    }
    super::simd::count_non_ascii_simd(bytes) as f32 / len as f32
}

/// Upper bound on pattern count where the `aho-corasick` DFA engine is still preferred.
///
/// Benchmarked on Apple M3 Max (`search_ascii_en`, 580 KB English haystack,
/// `find_overlapping_iter` — full-text scan counting all hits):
///
///   N=7,000  → AcDfa 2.536 ms vs DaacBytewise 4.818 ms (DFA 1.9× faster)
///   N=8,000  → AcDfa 2.588 ms vs DaacBytewise 4.586 ms (DFA 1.8× faster)
///   N=10,000 → AcDfa 2.699 ms vs DaacBytewise 4.541 ms (DFA 1.7× faster)
///   N=50,000 → AcDfa 5.473 ms vs DaacBytewise 5.827 ms (DFA 1.07× faster)
///
/// Memory (heap bytes, ASCII patterns only):
///   N=10,000 → AcDfa 9.7 MB vs DaacBytewise 553 KB (DFA 17× larger)
///   N=50,000 → AcDfa 35 MB  vs DaacBytewise 2.1 MB (DFA 17× larger)
///
/// The DFA is faster at all measured pattern counts (up to 50 K). However, at 50 K
/// the DFA + charwise combined footprint (~38 MB) causes L3 cache pressure and a
/// net regression. Threshold raised from 7,000 → 15,000 → 25,000: at 20 K patterns
/// the DFA weighs ~19 MB and with charwise (~1.6 MB) totals ~21 MB. This exceeds
/// L2 on M-series (16 MB), causing a ~4-7% regression on scan-dominated workloads
/// with few matches (e.g. AND patterns that rarely fire). However, for workloads
/// with frequent matches (NOT shapes, large rule sets) the DFA's 1.7× faster scan
/// offsets the cache pressure, yielding 15-20% net improvement. Since most
/// real-world matchers produce matches, the higher threshold is net positive.
///
/// Only used when all patterns are pure ASCII; for mixed or non-ASCII pattern sets the
/// DFA state table would be much larger (3-byte UTF-8 sequences), so DaacBytewise is
/// always used in that case. Only relevant when the `dfa` feature is enabled.
#[cfg(feature = "dfa")]
const AC_DFA_PATTERN_THRESHOLD: usize = 25_000;

/// Compiled scan engines together with the pattern metadata they report into.
///
/// Immutable after construction. Shared across all threads via `Arc` or by virtue of
/// [`SimpleMatcher`](super::SimpleMatcher) being `Send + Sync`.
///
/// The charwise engine is always built (even for pure-ASCII pattern sets) so that
/// non-ASCII haystacks can be scanned at character granularity for ~1.6–1.9× throughput
/// over bytewise scanning (CJK characters are 3 UTF-8 bytes → 3 bytewise transitions
/// vs 1 charwise transition).
///
/// # Performance
///
/// - **Bytewise DFA** (when `dfa` feature + ≤[`AC_DFA_PATTERN_THRESHOLD`] patterns):
///   ~1.7–1.9× faster than DAAC bytewise on ASCII text, but ~17× more memory.
/// - **Charwise DAAC**: 1 state transition per character (vs 3 bytewise for CJK),
///   yielding ~1.6–1.9× throughput on non-ASCII text.
///
/// Engine selection is density-based: bytewise for low non-ASCII density
/// (≤ [`CHARWISE_DENSITY_THRESHOLD`]), charwise for high density.
#[derive(Clone)]
pub(super) struct ScanPlan {
    /// Bytewise engine built from ALL patterns.
    bytewise_matcher: Option<BytewiseMatcher>,
    /// Charwise engine built from ALL patterns.
    charwise_matcher: Option<CharwiseMatcher>,
    /// `true` when every compiled pattern is pure ASCII.
    ///
    /// Used only for the `is_match` fast-return: when all patterns are ASCII and
    /// the text contains zero ASCII bytes, no match is possible.
    all_patterns_ascii: bool,
    /// Flat index mapping automaton raw values back to rule-entry metadata.
    patterns: PatternIndex,
}

/// Bytewise scan engine chosen at build time.
///
/// Contains the ASCII pattern subset. The variant is selected by [`compile_automata`]
/// based on the `dfa` feature flag and the number of ASCII patterns:
///
/// - [`AcDfa`](Self::AcDfa) — `aho-corasick` DFA. Fastest throughput but ~10x memory
///   vs NFA. Only used when the `dfa` feature is on, all patterns are ASCII, and pattern
///   count ≤ [`AC_DFA_PATTERN_THRESHOLD`].
/// - [`DaacBytewise`](Self::DaacBytewise) — `daachorse` bytewise double-array
///   Aho-Corasick. Lower memory; used for mixed/non-ASCII pattern sets or when
///   the DFA threshold is exceeded.
#[derive(Clone)]
enum BytewiseMatcher {
    /// `aho-corasick` DFA engine with prefilter acceleration.
    ///
    /// Uses the high-level [`AhoCorasick`](AcEngine) wrapper (forced to DFA kind) rather
    /// than the low-level `dfa::DFA`, because `AhoCorasick` integrates Teddy/memchr
    /// prefilter logic that the raw DFA doesn't expose. The prefilter SIMD-skips
    /// non-matching regions in `is_match`, giving 2–4× on sparse haystacks.
    ///
    /// The crate uses pattern indices (not user-supplied values) in its match output,
    /// so `to_value` maps pattern index → raw value.
    #[cfg(feature = "dfa")]
    AcDfa {
        matcher: AcEngine,
        to_value: Vec<u32>,
    },
    /// `daachorse` bytewise double-array engine with user-supplied `u32` values.
    DaacBytewise(DoubleArrayAhoCorasick<u32>),
}

type CharwiseMatcher = CharwiseDoubleArrayAhoCorasick<u32>;

/// Construction and query helpers for compiled scan engines.
impl ScanPlan {
    /// Compiles the bytewise and charwise scan engines for the deduplicated pattern set.
    pub(super) fn compile(
        dedup_patterns: &[Cow<'_, str>],
        dedup_entries: Vec<Vec<PatternEntry>>,
    ) -> Result<Self, MatcherError> {
        let patterns = PatternIndex::new(dedup_entries);
        let value_map = patterns.build_value_map();
        let (bytewise_matcher, charwise_matcher) = compile_automata(dedup_patterns, &value_map)?;
        let all_patterns_ascii = dedup_patterns.iter().all(|p| p.is_ascii());

        Ok(Self {
            bytewise_matcher,
            charwise_matcher,
            all_patterns_ascii,
            patterns,
        })
    }

    /// Returns the pattern metadata referenced by the compiled scan engines.
    pub(super) fn patterns(&self) -> &PatternIndex {
        &self.patterns
    }

    /// Returns the estimated heap memory in bytes owned by all scan engines.
    pub(super) fn heap_bytes(&self) -> usize {
        let bw = self.bytewise_matcher.as_ref().map_or(0, |m| m.heap_bytes());
        let cw = self.charwise_matcher.as_ref().map_or(0, |m| m.heap_bytes());
        bw + cw + self.patterns.heap_bytes()
    }

    /// Returns whether any compiled pattern matches `text`.
    ///
    /// Engine selection uses non-ASCII byte density from a 512-byte prefix sample:
    ///
    /// 1. **All-ASCII patterns** — density-based dispatch:
    ///    - **100% non-ASCII** (verified full text): `return false` — ASCII patterns
    ///      cannot match text with zero ASCII bytes (UTF-8 guarantees bytes ≥0x80
    ///      never encode ASCII codepoints).
    ///    - **High density** (> [`CHARWISE_DENSITY_THRESHOLD`]): charwise engine
    ///      (1.3–2.5× faster than DFA at ≥40% CJK characters).
    ///    - **Low density**: bytewise DFA+Teddy (1.7–2.5× faster on ASCII-heavy text).
    ///
    /// 2. **Mixed patterns** — exact `is_ascii()` dispatch (bytewise only has ASCII
    ///    patterns, so CJK text must use charwise for correctness).
    #[inline(always)]
    pub(super) fn is_match(&self, text: &str) -> bool {
        if self.all_patterns_ascii {
            let density = text_non_ascii_density(text);
            // ASCII patterns can never match text with zero ASCII bytes.
            if density >= 1.0 && text.bytes().all(|b| b >= 0x80) {
                return false;
            }
            if density > CHARWISE_DENSITY_THRESHOLD {
                return self
                    .charwise_matcher
                    .as_ref()
                    .is_some_and(|m| m.is_match_text(text));
            }
            return self
                .bytewise_matcher
                .as_ref()
                .is_some_and(|m| m.is_match(text));
        }

        // Mixed patterns — exact is_ascii dispatch (bytewise only has ASCII patterns).
        if text.is_ascii() {
            self.bytewise_matcher
                .as_ref()
                .is_some_and(|m| m.is_match(text))
        } else {
            self.charwise_matcher
                .as_ref()
                .is_some_and(|m| m.is_match_text(text))
        }
    }

    /// Calls `on_value` for each raw match value produced by the chosen engine.
    ///
    /// Returns `true` if the callback requests early exit. Engine selection is
    /// density-based: bytewise for low non-ASCII density (≤ [`CHARWISE_DENSITY_THRESHOLD`]),
    /// charwise for high density. When `all_patterns_ascii` and the text is entirely
    /// non-ASCII, returns `false` without scanning (no ASCII pattern can match).
    #[inline(always)]
    pub(super) fn for_each_match_value(
        &self,
        text: &str,
        density: f32,
        on_value: impl FnMut(u32) -> bool,
    ) -> bool {
        if self.all_patterns_ascii && density >= 1.0 && text.bytes().all(|b| b >= 0x80) {
            return false;
        }
        if density <= CHARWISE_DENSITY_THRESHOLD {
            if let Some(ref matcher) = self.bytewise_matcher {
                return matcher.for_each_match_value(text, on_value);
            }
        } else if let Some(ref matcher) = self.charwise_matcher {
            return matcher.for_each_match_value(text, on_value);
        }
        false
    }

    /// Calls `on_value` for each raw match value from a streaming byte iterator.
    ///
    /// Used by the fused delete-scan path. Returns `Some(bool)` when streaming
    /// succeeded, `None` when DFA would be selected (caller must fall back to
    /// materialized scan).
    #[inline(always)]
    pub(super) fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        density: f32,
        on_value: impl FnMut(u32) -> bool,
    ) -> Option<bool> {
        if density <= CHARWISE_DENSITY_THRESHOLD {
            match self.bytewise_matcher {
                #[cfg(feature = "dfa")]
                Some(BytewiseMatcher::AcDfa { .. }) => return None,
                Some(ref matcher) => {
                    return Some(matcher.for_each_match_value_from_iter(iter, on_value));
                }
                None => return Some(false),
            }
        }
        if let Some(ref matcher) = self.charwise_matcher {
            Some(matcher.for_each_match_value_from_iter(iter, on_value))
        } else {
            Some(false)
        }
    }

    /// Returns whether the streaming `_from_iter` scan path is available for the
    /// given density. `false` when the DFA engine would be selected (no streaming API).
    #[inline(always)]
    pub(super) fn can_stream(&self, density: f32) -> bool {
        if density <= CHARWISE_DENSITY_THRESHOLD {
            #[cfg(feature = "dfa")]
            if matches!(self.bytewise_matcher, Some(BytewiseMatcher::AcDfa { .. })) {
                return false;
            }
        }
        true
    }

    /// AllSimple-specialized scan: yields rule indices directly.
    ///
    /// Every raw value is assumed to carry `DIRECT_RULE_BIT`; the callback receives
    /// the extracted `rule_idx` (no early exit, no indirect dispatch).
    #[inline(always)]
    pub(super) fn for_each_rule_idx_simple(
        &self,
        text: &str,
        density: f32,
        on_rule: impl FnMut(usize),
    ) {
        if density <= CHARWISE_DENSITY_THRESHOLD {
            if let Some(ref matcher) = self.bytewise_matcher {
                matcher.for_each_rule_idx_simple(text, on_rule);
            }
        } else if let Some(ref matcher) = self.charwise_matcher {
            matcher.for_each_rule_idx_simple(text, on_rule);
        }
    }
}

/// Query helpers for the bytewise scan engine.
impl BytewiseMatcher {
    /// Returns whether the bytewise engine matches `text`.
    #[inline(always)]
    fn is_match(&self, text: &str) -> bool {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, .. } => matcher.is_match(text),
            Self::DaacBytewise(matcher) => matcher.find_iter(text).next().is_some(),
        }
    }

    /// Calls `on_value` for each raw match value produced by the bytewise engine.
    #[inline(always)]
    fn for_each_match_value(&self, text: &str, mut on_value: impl FnMut(u32) -> bool) -> bool {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, to_value } => {
                for m in matcher.find_overlapping_iter(text) {
                    // SAFETY: `to_value` has one entry per pattern; pattern index is always in bounds.
                    let value = unsafe { *to_value.get_unchecked(m.pattern().as_usize()) };
                    if on_value(value) {
                        return true;
                    }
                }
                false
            }
            Self::DaacBytewise(matcher) => {
                for hit in matcher.find_overlapping_iter(text) {
                    if on_value(hit.value()) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// AllSimple-specialized: yields rule indices directly, no early exit, no
    /// DIRECT_RULE_BIT check. Every value is assumed to have DIRECT_RULE_BIT set.
    #[inline(always)]
    fn for_each_rule_idx_simple(&self, text: &str, mut on_rule: impl FnMut(usize)) {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, to_value } => {
                for m in matcher.find_overlapping_iter(text) {
                    // SAFETY: `to_value` has one entry per pattern.
                    let value = unsafe { *to_value.get_unchecked(m.pattern().as_usize()) };
                    on_rule((value & DIRECT_RULE_MASK) as usize);
                }
            }
            Self::DaacBytewise(matcher) => {
                for hit in matcher.find_overlapping_iter(text) {
                    on_rule((hit.value() & DIRECT_RULE_MASK) as usize);
                }
            }
        }
    }

    /// Streaming variant: accepts a byte iterator instead of a complete `&str`.
    /// Only available for DaacBytewise (DFA has no streaming API).
    #[inline(always)]
    fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        mut on_value: impl FnMut(u32) -> bool,
    ) -> bool {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { .. } => unreachable!("DFA has no streaming API"),
            Self::DaacBytewise(matcher) => {
                for hit in matcher.find_overlapping_iter_from_iter(iter) {
                    if on_value(hit.value()) {
                        return true;
                    }
                }
                false
            }
        }
    }

    fn heap_bytes(&self) -> usize {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, to_value } => {
                matcher.memory_usage() + to_value.capacity() * size_of::<u32>()
            }
            Self::DaacBytewise(matcher) => matcher.heap_bytes(),
        }
    }
}

/// Query helpers for the charwise scan engine.
trait CharwiseMatcherExt {
    fn is_match_text(&self, text: &str) -> bool;
    fn for_each_match_value(&self, text: &str, on_value: impl FnMut(u32) -> bool) -> bool;
    fn for_each_rule_idx_simple(&self, text: &str, on_rule: impl FnMut(usize));
}

/// Streaming query helpers for the charwise scan engine.
trait CharwiseMatcherStreamExt {
    fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        on_value: impl FnMut(u32) -> bool,
    ) -> bool;
}

impl CharwiseMatcherStreamExt for CharwiseMatcher {
    #[inline(always)]
    fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        mut on_value: impl FnMut(u32) -> bool,
    ) -> bool {
        // SAFETY: The streaming iterators (DeleteFilterIterator, NormalizeFilterIterator)
        // yield valid UTF-8: delete outputs a subsequence of complete codepoints;
        // normalize outputs unmapped codepoints verbatim plus valid UTF-8 replacement strings.
        for hit in unsafe { self.find_overlapping_iter_from_iter(iter) } {
            if on_value(hit.value()) {
                return true;
            }
        }
        false
    }
}

impl CharwiseMatcherExt for CharwiseMatcher {
    fn is_match_text(&self, text: &str) -> bool {
        self.find_iter(text).next().is_some()
    }

    #[inline(always)]
    fn for_each_match_value(&self, text: &str, mut on_value: impl FnMut(u32) -> bool) -> bool {
        for hit in self.find_overlapping_iter(text) {
            if on_value(hit.value()) {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    fn for_each_rule_idx_simple(&self, text: &str, mut on_rule: impl FnMut(usize)) {
        for hit in self.find_overlapping_iter(text) {
            on_rule((hit.value() & DIRECT_RULE_MASK) as usize);
        }
    }
}

/// Compiles the bytewise and charwise automata from the deduplicated pattern list.
///
/// - **ASCII patterns** → [`BytewiseMatcher`] (DFA or DAAC bytewise).
/// - **All patterns** → [`CharwiseMatcher`] (DAAC charwise), always built from the
///   full pattern set so that non-ASCII haystacks benefit from character-granularity
///   scanning (~1.6–1.9× faster on CJK text vs bytewise).
///
/// # Errors
///
/// Returns [`MatcherError`] if the `daachorse` or `aho-corasick` automaton builders
/// encounter an internal error during construction.
fn compile_automata(
    dedup_patterns: &[Cow<'_, str>],
    value_map: &[u32],
) -> Result<(Option<BytewiseMatcher>, Option<CharwiseMatcher>), MatcherError> {
    if dedup_patterns.is_empty() {
        return Ok((None, None));
    }

    let all_patvals: Vec<(&str, u32)> = dedup_patterns
        .iter()
        .enumerate()
        .map(|(i, p)| (p.as_ref(), value_map[i]))
        .collect();

    // Both engines are built from the FULL pattern set. Bytewise handles any
    // UTF-8 text via byte-level matching; charwise gives 1.6–1.9× throughput
    // on CJK text via character-granularity transitions. The density-based
    // dispatch in ScanPlan selects the faster engine at runtime.
    let all_patvals_clone = all_patvals.clone();
    let build_bytewise = move || -> Result<BytewiseMatcher, MatcherError> {
        build_current_bytewise(all_patvals_clone)
    };

    let build_charwise = |source: Vec<(&str, u32)>| -> Result<CharwiseMatcher, MatcherError> {
        CharwiseDoubleArrayAhoCorasickBuilder::new()
            .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
            .build_with_values(source)
            .map_err(MatcherError::automaton_build)
    };

    std::thread::scope(|s| {
        let bytewise_handle = s.spawn(build_bytewise);
        let charwise = build_charwise(all_patvals)?;
        let bytewise = bytewise_handle
            .join()
            .expect("bytewise automaton build panicked")?;
        Ok((Some(bytewise), Some(charwise)))
    })
}

/// Builds the bytewise engine from the full pattern set.
///
/// Uses DFA (with Teddy prefilter) when pattern count ≤ [`AC_DFA_PATTERN_THRESHOLD`],
/// regardless of pattern encoding. Falls back to DAAC bytewise otherwise.
fn build_current_bytewise(all_patvals: Vec<(&str, u32)>) -> Result<BytewiseMatcher, MatcherError> {
    #[cfg(feature = "dfa")]
    if all_patvals.len() <= AC_DFA_PATTERN_THRESHOLD {
        let to_value: Vec<u32> = all_patvals.iter().map(|&(_, v)| v).collect();
        return Ok(BytewiseMatcher::AcDfa {
            matcher: AhoCorasickBuilder::new()
                .kind(Some(AhoCorasickKind::DFA))
                .match_kind(AhoCorasickMatchKind::Standard)
                .build(all_patvals.iter().map(|(p, _)| p))
                .map_err(MatcherError::automaton_build)?,
            to_value,
        });
    }

    Ok(BytewiseMatcher::DaacBytewise(
        DoubleArrayAhoCorasickBuilder::new()
            .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
            .build_with_values(all_patvals)
            .map_err(MatcherError::automaton_build)?,
    ))
}
