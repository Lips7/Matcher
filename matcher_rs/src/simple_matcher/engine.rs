//! Scan-engine compilation and match iteration for [`super::SimpleMatcher`].
//!
//! This module owns the Aho-Corasick automata that power Pass 1 (pattern scan) of the
//! two-pass matching pipeline. Two independent engines are compiled, **both containing
//! the full pattern set**:
//!
//! - **Bytewise engine** ([`BytewiseMatcher`]) — scans byte-by-byte. With the `dfa`
//!   feature enabled and all patterns ASCII and count ≤ [`AC_DFA_PATTERN_THRESHOLD`], this
//!   uses the `aho-corasick` crate's DFA. Otherwise, it falls back to `daachorse`'s
//!   bytewise double-array Aho-Corasick. Non-ASCII patterns are stored as raw UTF-8 byte
//!   sequences; UTF-8's self-synchronizing property prevents false byte-level matches.
//!
//! - **Charwise engine** ([`CharwiseMatcher`]) — scans character-wise using `daachorse`'s
//!   charwise automaton. Preferred when the input text has high multi-byte character
//!   density (≥ [`CHARWISE_DENSITY_THRESHOLD`]), because charwise pays one AC transition
//!   per character instead of one per byte.
//!
//! The [`ScanPlan`] struct bundles both engines together with the [`PatternIndex`] that
//! maps raw automaton values back to rule metadata.
//!
//! # Engine selection
//!
//! [`ScanPlan::is_match`] and [`ScanPlan::for_each_match_value`] accept a `use_bytewise`
//! flag (computed once per text variant by [`super::search`]). This flag is derived from
//! [`crate::process::transform::simd::multibyte_density`]: if the fraction of UTF-8
//! continuation bytes is below [`CHARWISE_DENSITY_THRESHOLD`], bytewise wins; otherwise
//! charwise wins.

use std::borrow::Cow;

#[cfg(feature = "dfa")]
use aho_corasick::{
    Anchored, Input, MatchKind as AhoCorasickMatchKind, automaton::Automaton,
    dfa::DFA as AcDfaEngine,
};
use daachorse::{
    DoubleArrayAhoCorasick, DoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
    charwise::{CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder},
};

use crate::MatcherError;
use crate::process::transform::simd::multibyte_density;

use super::rule::{PatternEntry, PatternIndex};

/// Upper bound on pattern count where the `aho-corasick` DFA engine is still preferred.
///
/// Benchmarked on Apple M3 Max (`search_ascii_en`, 580 KB English haystack):
///
///   N=5,000 → AcDfa 418 MB/s vs DaacBytewise 353 MB/s (+18%)
///   N=6,000 → AcDfa 395 MB/s vs DaacBytewise 356 MB/s (+11%)
///   N=7,000 → AcDfa 386 MB/s vs DaacBytewise 340 MB/s (+14%)
///   N=8,000 → AcDfa 273 MB/s vs DaacBytewise 322 MB/s (-15%) ← cache cliff
///
/// The sharp reversal at 8,000 indicates the DFA state table crosses the per-core L2
/// cache boundary at ~7k–8k ASCII patterns. Threshold set to 7,000: AcDfa leads by
/// 11–14% at 6k–7k and DaacBytewise wins decisively at 8k+.
///
/// Only used when all patterns are pure ASCII; for mixed or non-ASCII pattern sets the
/// DFA state table would be much larger (3-byte UTF-8 sequences), so DaacBytewise is
/// always used in that case. Only relevant when the `dfa` feature is enabled.
#[cfg(feature = "dfa")]
const AC_DFA_PATTERN_THRESHOLD: usize = 7_000;

/// Multi-byte character density threshold for charwise engine selection.
///
/// When the fraction of UTF-8 continuation bytes in the input text is at or above this
/// value, the charwise engine is preferred over the bytewise engine. Below this threshold,
/// the bytewise engine wins because most bytes map 1:1 to characters and the per-byte AC
/// transition overhead is lower than the UTF-8 decode overhead in charwise.
///
/// Calibrated via the `density_dispatch` benchmark in `bench_engine.rs` (Apple M3 Max,
/// 2,000 mixed patterns, 200 KB synthetic text):
///   0% CJK  (density 0.000) → DaacBytewise  648 MB/s vs DaacCharwise  599 MB/s  (+8%)
///   10% CJK (density 0.167) → DaacBytewise  243 MB/s vs DaacCharwise  339 MB/s  (−29%)
/// The crossover lies between density 0.000 and 0.167 (interpolated: ~0.03). Threshold
/// set to 0.1 to keep pure-ASCII and very-low-density text on the bytewise engine while
/// routing any meaningful CJK content to charwise.
///
/// This threshold only applies when `BytewiseMatcher::DaacBytewise` is compiled. When
/// `BytewiseMatcher::AcDfa` is compiled, `ScanPlan::charwise_density_threshold` is
/// `f32::MAX` (charwise is never selected), because AcDfa beats DaacCharwise at every
/// density including pure CJK (~540 µs vs ~1,650 µs for ASCII patterns on Chinese text).
///
/// Run the `density_dispatch` benchmark to re-tune for a different workload.
pub(super) const CHARWISE_DENSITY_THRESHOLD: f32 = 0.1;

/// Compiled scan engines together with the pattern metadata they report into.
///
/// Immutable after construction. Shared across all threads via `Arc` or by virtue of
/// [`SimpleMatcher`](super::SimpleMatcher) being `Send + Sync`.
///
/// Both engines contain the **full** pattern set. They are `None` only when there are
/// no patterns at all; otherwise both are `Some` and engine selection is purely a
/// throughput decision made at query time via `charwise_density_threshold`.
#[derive(Clone)]
pub(super) struct ScanPlan {
    /// Bytewise engine: scans raw bytes. Contains all patterns encoded as UTF-8 byte
    /// sequences. Preferred for low-density (mostly ASCII) text.
    /// `None` when no patterns exist.
    bytewise_matcher: Option<BytewiseMatcher>,
    /// Charwise engine: scans Unicode characters. Contains all patterns. Preferred
    /// for high-density (mostly multi-byte) text.
    /// `None` when no patterns exist.
    charwise_matcher: Option<CharwiseMatcher>,
    /// Flat index mapping automaton raw values back to rule-entry metadata.
    patterns: PatternIndex,
    /// Per-plan effective charwise density threshold.
    ///
    /// Set at compile time based on which bytewise engine was selected:
    /// - `f32::MAX` when `BytewiseMatcher::AcDfa` is active: AcDfa beats charwise at all
    ///   densities (including pure CJK), so charwise is never selected.
    /// - [`CHARWISE_DENSITY_THRESHOLD`] when `BytewiseMatcher::DaacBytewise` is active:
    ///   charwise wins above ~10% CJK content.
    charwise_density_threshold: f32,
}

/// Bytewise scan engine chosen at build time.
///
/// Contains the **full** pattern set encoded as raw UTF-8 byte sequences. The variant is
/// selected by [`compile_automata`] based on the `dfa` feature flag and whether all
/// patterns happen to be ASCII:
///
/// - [`AcDfa`](Self::AcDfa) — `aho-corasick` DFA. Fastest throughput but ~10x memory
///   vs NFA. Only used when the `dfa` feature is on, all patterns are ASCII, and pattern
///   count ≤ [`AC_DFA_PATTERN_THRESHOLD`].
/// - [`DaacBytewise`](Self::DaacBytewise) — `daachorse` bytewise double-array
///   Aho-Corasick. Lower memory; used for mixed/non-ASCII pattern sets or when
///   the DFA threshold is exceeded.
#[derive(Clone)]
enum BytewiseMatcher {
    /// `aho-corasick` DFA engine.
    ///
    /// The `aho-corasick` crate uses pattern indices (not user-supplied values) in its
    /// match output, so `to_value` maps pattern index → raw value.
    #[cfg(feature = "dfa")]
    AcDfa {
        matcher: Box<AcDfaEngine>,
        to_value: Vec<u32>,
    },
    /// `daachorse` bytewise double-array engine with user-supplied `u32` values.
    DaacBytewise(DoubleArrayAhoCorasick<u32>),
}

/// Charwise scan engine chosen at build time.
///
/// Contains the **full** pattern set. Currently only one variant exists. The enum wrapper
/// allows future extension (e.g., an `aho-corasick` charwise DFA) without changing call
/// sites.
#[derive(Clone)]
enum CharwiseMatcher {
    /// `daachorse` charwise double-array engine with user-supplied `u32` values.
    DaacCharwise(CharwiseDoubleArrayAhoCorasick<u32>),
}

/// Construction and query helpers for compiled scan engines.
impl ScanPlan {
    /// Compiles the bytewise and charwise scan engines for the deduplicated pattern set.
    ///
    /// 1. Builds a [`PatternIndex`] from the raw entry buckets.
    /// 2. Builds the value map (direct-rule encoding where possible).
    /// 3. Delegates to [`compile_automata`] for actual automaton construction.
    pub(super) fn compile(
        dedup_patterns: &[Cow<'_, str>],
        dedup_entries: Vec<Vec<PatternEntry>>,
    ) -> Result<Self, MatcherError> {
        let patterns = PatternIndex::new(dedup_entries);
        let value_map = patterns.build_value_map();
        let (bytewise_matcher, charwise_matcher) = compile_automata(dedup_patterns, &value_map)?;

        // AcDfa beats DaacCharwise at all densities (including pure CJK), so when AcDfa
        // is the bytewise engine, charwise should never be selected. Use f32::MAX to
        // disable charwise for AcDfa plans; use the calibrated threshold otherwise.
        let charwise_density_threshold = match &bytewise_matcher {
            #[cfg(feature = "dfa")]
            Some(BytewiseMatcher::AcDfa { .. }) => f32::MAX,
            _ => CHARWISE_DENSITY_THRESHOLD,
        };

        Ok(Self {
            bytewise_matcher,
            charwise_matcher,
            patterns,
            charwise_density_threshold,
        })
    }

    /// Returns the pattern metadata referenced by the compiled scan engines.
    #[inline(always)]
    pub(super) fn patterns(&self) -> &PatternIndex {
        &self.patterns
    }

    /// Returns the effective charwise density threshold for this plan.
    ///
    /// `f32::MAX` when the AcDfa bytewise engine is compiled (charwise never wins);
    /// [`CHARWISE_DENSITY_THRESHOLD`] for DaacBytewise plans.
    #[inline(always)]
    pub(super) fn charwise_density_threshold(&self) -> f32 {
        self.charwise_density_threshold
    }

    /// Returns whether any compiled pattern matches `text`.
    ///
    /// Computes the multi-byte density of `text` and selects the bytewise engine when
    /// density is below the per-plan [`charwise_density_threshold`](Self::charwise_density_threshold),
    /// otherwise the charwise engine.
    #[inline(always)]
    pub(super) fn is_match(&self, text: &str) -> bool {
        let use_bytewise = multibyte_density(text.as_bytes()) < self.charwise_density_threshold;
        if use_bytewise {
            self.bytewise_matcher
                .as_ref()
                .is_some_and(|m| m.is_match(text))
        } else {
            self.charwise_matcher
                .as_ref()
                .is_some_and(|m| m.is_match(text))
        }
    }

    /// Calls `on_value` for each raw match value produced by the chosen engine.
    ///
    /// Returns `true` if the callback requests early exit (i.e., `on_value` returned
    /// `true`). The `use_bytewise` flag selects the engine: `true` → bytewise,
    /// `false` → charwise. Callers compute this flag once per text variant using
    /// [`multibyte_density`].
    #[inline(always)]
    pub(super) fn for_each_match_value(
        &self,
        text: &str,
        use_bytewise: bool,
        on_value: impl FnMut(u32) -> bool,
    ) -> bool {
        if use_bytewise {
            if let Some(ref matcher) = self.bytewise_matcher {
                return matcher.for_each_match_value(text, on_value);
            }
        } else if let Some(ref matcher) = self.charwise_matcher {
            return matcher.for_each_match_value(text, on_value);
        }
        false
    }

    /// Calls `on_value` for each raw match value produced by streaming a byte
    /// iterator through the chosen engine.
    ///
    /// Same semantics as [`for_each_match_value`](Self::for_each_match_value)
    /// but accepts an `Iterator<Item = u8>` instead of a `&str`, enabling
    /// zero-allocation transform-to-scan fusion.
    #[inline(always)]
    pub(super) fn for_each_match_value_from_iter<I: Iterator<Item = u8>>(
        &self,
        iter: I,
        use_bytewise: bool,
        on_value: impl FnMut(u32) -> bool,
    ) -> bool {
        if use_bytewise {
            if let Some(ref matcher) = self.bytewise_matcher {
                return matcher.for_each_match_value_from_iter(iter, on_value);
            }
        } else if let Some(ref matcher) = self.charwise_matcher {
            return matcher.for_each_match_value_from_iter(iter, on_value);
        }
        false
    }
}

/// Query helpers for the bytewise scan engine.
impl BytewiseMatcher {
    /// Returns whether the bytewise engine matches `text`.
    #[inline(always)]
    fn is_match(&self, text: &str) -> bool {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, .. } => matcher.try_find(&Input::new(text)).unwrap().is_some(),
            Self::DaacBytewise(matcher) => matcher.find_iter(text).next().is_some(),
        }
    }

    /// Calls `on_value` for each raw match value produced by the bytewise engine.
    #[inline(always)]
    fn for_each_match_value(&self, text: &str, mut on_value: impl FnMut(u32) -> bool) -> bool {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, to_value } => {
                for hit in matcher.try_find_overlapping_iter(Input::new(text)).unwrap() {
                    // SAFETY: `to_value` has one entry per pattern; pattern index is always in bounds.
                    let value = unsafe { *to_value.get_unchecked(hit.pattern().as_usize()) };
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

    /// Streams a byte iterator through the bytewise engine, calling `on_value` per hit.
    #[inline(always)]
    fn for_each_match_value_from_iter<I: Iterator<Item = u8>>(
        &self,
        iter: I,
        mut on_value: impl FnMut(u32) -> bool,
    ) -> bool {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, to_value } => {
                let mut sid = matcher.start_state(Anchored::No).unwrap();
                for byte in iter {
                    sid = matcher.next_state(Anchored::No, sid, byte);
                    if matcher.is_special(sid) && matcher.is_match(sid) {
                        for i in 0..matcher.match_len(sid) {
                            let pid = matcher.match_pattern(sid, i);
                            // SAFETY: `to_value` has one entry per pattern.
                            let value = unsafe { *to_value.get_unchecked(pid.as_usize()) };
                            if on_value(value) {
                                return true;
                            }
                        }
                    }
                }
                false
            }
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
}

/// Query helpers for the charwise scan engine.
impl CharwiseMatcher {
    /// Returns whether the charwise engine matches `text`.
    #[inline(always)]
    fn is_match(&self, text: &str) -> bool {
        match self {
            Self::DaacCharwise(matcher) => matcher.find_iter(text).next().is_some(),
        }
    }

    /// Calls `on_value` for each raw match value produced by the charwise engine.
    #[inline(always)]
    fn for_each_match_value(&self, text: &str, mut on_value: impl FnMut(u32) -> bool) -> bool {
        match self {
            Self::DaacCharwise(matcher) => {
                for hit in matcher.find_overlapping_iter(text) {
                    if on_value(hit.value()) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Streams a byte iterator through the charwise engine, calling `on_value` per hit.
    ///
    /// # Safety (internal)
    ///
    /// The `unsafe` call to `find_overlapping_iter_from_iter` requires that
    /// the byte iterator produces valid UTF-8. This is guaranteed because the
    /// transform byte iterators preserve UTF-8 validity of the source text.
    #[inline(always)]
    fn for_each_match_value_from_iter<I: Iterator<Item = u8>>(
        &self,
        iter: I,
        mut on_value: impl FnMut(u32) -> bool,
    ) -> bool {
        match self {
            Self::DaacCharwise(matcher) => {
                // SAFETY: byte iterator produces valid UTF-8 (transforms preserve validity).
                for hit in unsafe { matcher.find_overlapping_iter_from_iter(iter) } {
                    if on_value(hit.value()) {
                        return true;
                    }
                }
                false
            }
        }
    }
}

/// Compiles the bytewise and charwise automata from the deduplicated pattern list.
///
/// **Both engines are built from the full pattern set.** The bytewise engine stores
/// non-ASCII patterns as raw UTF-8 byte sequences; the charwise engine stores them as
/// Unicode character sequences. Engine selection at query time is controlled by
/// [`CHARWISE_DENSITY_THRESHOLD`].
///
/// The `aho-corasick` DFA is used for the bytewise engine only when all patterns happen
/// to be ASCII and the count is ≤ [`AC_DFA_PATTERN_THRESHOLD`]. Mixed or non-ASCII
/// pattern sets always use `daachorse` bytewise for the bytewise engine (DFA state tables
/// would be prohibitively large for long multi-byte byte sequences).
///
/// When the pattern list is empty, both engines are `None`.
///
/// # Errors
///
/// Returns [`MatcherError`] if any automaton builder encounters an internal error.
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

    // Decide whether the AcDfa engine is viable: only when all patterns are ASCII and
    // the count is within the threshold (mixed patterns would create an excessively large
    // DFA state table).
    #[cfg(feature = "dfa")]
    let all_ascii = dedup_patterns.iter().all(|p| p.is_ascii());
    #[cfg(feature = "dfa")]
    let ac_to_value: Vec<u32> = value_map.to_vec();

    let build_bytewise = || -> Result<BytewiseMatcher, MatcherError> {
        #[cfg(feature = "dfa")]
        if all_ascii && all_patvals.len() <= AC_DFA_PATTERN_THRESHOLD {
            return Ok(BytewiseMatcher::AcDfa {
                matcher: Box::new(
                    AcDfaEngine::builder()
                        .match_kind(AhoCorasickMatchKind::Standard)
                        .build(all_patvals.iter().map(|(p, _)| p))
                        .map_err(MatcherError::automaton_build)?,
                ),
                to_value: ac_to_value,
            });
        }
        Ok(BytewiseMatcher::DaacBytewise(
            DoubleArrayAhoCorasickBuilder::new()
                .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                .build_with_values(all_patvals.iter().copied())
                .map_err(MatcherError::automaton_build)?,
        ))
    };

    let build_charwise = || -> Result<CharwiseMatcher, MatcherError> {
        Ok(CharwiseMatcher::DaacCharwise(
            CharwiseDoubleArrayAhoCorasickBuilder::new()
                .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                .build_with_values(all_patvals.iter().copied())
                .map_err(MatcherError::automaton_build)?,
        ))
    };

    std::thread::scope(|s| {
        let bytewise_handle = s.spawn(build_bytewise);
        let charwise = build_charwise()?;
        let bytewise = bytewise_handle
            .join()
            .expect("bytewise automaton build panicked")?;
        Ok((Some(bytewise), Some(charwise)))
    })
}
