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
//!   charwise automaton. When both ASCII and non-ASCII patterns exist, this engine is built
//!   over the **full** pattern set so a single charwise pass covers everything on non-ASCII
//!   input.
//!
//! The [`ScanPlan`] struct bundles both engines together with the [`PatternIndex`] that
//! maps raw automaton values back to rule metadata.
//!
//! # Engine selection
//!
//! [`ScanPlan::is_match`] and [`ScanPlan::for_each_match_value`] accept an `is_ascii`
//! flag (computed once per text variant by [`super::search`] via `str::is_ascii()`).
//! When no charwise engine exists (all patterns are ASCII), the bytewise engine is always
//! used and the `is_ascii` check is skipped entirely.

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

#[cfg(feature = "harry")]
use super::harry::HarryMatcher;
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

/// Compiled scan engines together with the pattern metadata they report into.
///
/// Immutable after construction. Shared across all threads via `Arc` or by virtue of
/// [`SimpleMatcher`](super::SimpleMatcher) being `Send + Sync`.
///
/// Either or both engines may be `None` when the corresponding pattern class is absent.
/// For example, if all patterns are pure ASCII, `charwise_matcher` will be `None`.
#[derive(Clone)]
pub(super) struct ScanPlan {
    /// Bytewise engine for ASCII patterns.
    /// `None` when no ASCII patterns exist.
    bytewise_matcher: Option<BytewiseMatcher>,
    /// Charwise engine for text containing multi-byte characters.
    ///
    /// When both ASCII and non-ASCII patterns exist, this engine contains the full
    /// pattern set so a single pass handles everything on non-ASCII input.
    /// `None` when no non-ASCII patterns exist and no ASCII patterns need charwise
    /// coverage.
    charwise_matcher: Option<CharwiseMatcher>,
    /// Harry column-vector SIMD engine for `is_match` fast path.
    ///
    /// Built from the full pattern set (ASCII + non-ASCII). When present, `is_match`
    /// dispatches here instead of to the AC engines — Harry has no state table and
    /// is faster at large pattern counts on both ASCII and CJK haystacks.
    /// `None` when the pattern set is too small (< 64) or has no length-≥2 pattern.
    #[cfg(feature = "harry")]
    harry_matcher: Option<Box<HarryMatcher>>,
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
/// Currently only one variant exists. The enum wrapper allows future extension (e.g.,
/// an `aho-corasick` charwise DFA) without changing call sites.
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
    /// 3. Delegates to [`compile_automata`] for AC automaton construction.
    /// 4. Attempts to build a [`HarryMatcher`] from the full pattern set for `is_match`.
    pub(super) fn compile(
        dedup_patterns: &[Cow<'_, str>],
        dedup_entries: Vec<Vec<PatternEntry>>,
    ) -> Result<Self, MatcherError> {
        let patterns = PatternIndex::new(dedup_entries);
        let value_map = patterns.build_value_map();
        let (bytewise_matcher, charwise_matcher) = compile_automata(dedup_patterns, &value_map)?;

        #[cfg(feature = "harry")]
        let harry_matcher = {
            let patvals: Vec<(&str, u32)> = dedup_patterns
                .iter()
                .enumerate()
                .map(|(i, p)| (p.as_ref(), value_map[i]))
                .collect();
            HarryMatcher::build(&patvals).map(Box::new)
        };

        Ok(Self {
            bytewise_matcher,
            charwise_matcher,
            #[cfg(feature = "harry")]
            harry_matcher,
            patterns,
        })
    }

    /// Returns the pattern metadata referenced by the compiled scan engines.
    #[inline(always)]
    pub(super) fn patterns(&self) -> &PatternIndex {
        &self.patterns
    }

    /// Returns whether the bytewise engine is a DFA.
    #[cfg(feature = "harry")]
    #[inline(always)]
    fn uses_dfa(&self) -> bool {
        #[cfg(feature = "dfa")]
        if let Some(BytewiseMatcher::AcDfa { .. }) = &self.bytewise_matcher {
            return true;
        }
        false
    }

    /// Returns whether any compiled pattern matches `text`.
    ///
    /// Engine selection for `is_match`:
    ///
    /// 1. **Harry** — used when available, all patterns are ASCII (`charwise_matcher`
    ///    is `None`), and no DFA engine exists (pattern count > [`AC_DFA_PATTERN_THRESHOLD`]).
    ///    Harry's dual-index encoding covers all 7 ASCII bits (zero encoding false
    ///    positives). With non-ASCII patterns bit 7 is lost, causing 1.5–2.7× more
    ///    false positives than AC. Below the DFA threshold the DFA's state table fits
    ///    in L2 cache and outperforms Harry by 10–20%.
    ///
    /// 2. **AC bytewise** — when text is ASCII or no charwise engine exists.
    ///
    /// 3. **AC charwise** — when text contains multi-byte characters and a charwise
    ///    engine was compiled.
    #[inline(always)]
    pub(super) fn is_match(&self, text: &str) -> bool {
        #[cfg(feature = "harry")]
        if self
            .harry_matcher
            .as_ref()
            .is_some_and(|_| self.charwise_matcher.is_none() && !self.uses_dfa())
        {
            return self.harry_matcher.as_ref().unwrap().is_match(text);
        }

        if self.charwise_matcher.is_none() || text.is_ascii() {
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
    /// `true`). The `is_ascii` flag determines engine selection: when `true` or when no
    /// charwise engine exists, the bytewise engine is used; otherwise the charwise engine
    /// handles the full scan.
    ///
    /// # Engine selection invariant
    ///
    /// When `is_ascii` is `true` and `bytewise_matcher` is `None` (all patterns are
    /// non-ASCII), the function returns `false` without scanning — this is correct because
    /// non-ASCII patterns cannot match in a pure-ASCII text (UTF-8 guarantees that
    /// multi-byte continuation bytes are always ≥ 0x80, so ASCII bytes never appear inside
    /// non-ASCII codepoints).
    #[inline(always)]
    pub(super) fn for_each_match_value(
        &self,
        text: &str,
        is_ascii: bool,
        on_value: impl FnMut(u32) -> bool,
    ) -> bool {
        let use_bytewise = self.charwise_matcher.is_none() || is_ascii;
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
        is_ascii: bool,
        on_value: impl FnMut(u32) -> bool,
    ) -> bool {
        let use_bytewise = self.charwise_matcher.is_none() || is_ascii;
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
/// Patterns are partitioned by `is_ascii()`:
///
/// - **ASCII-only patterns** → [`BytewiseMatcher`] (DFA or DAAC bytewise).
/// - **Non-ASCII patterns** → [`CharwiseMatcher`] (DAAC charwise).
///
/// When both classes are present, the charwise engine is built over the **full** pattern
/// set (ASCII + non-ASCII) so that a single charwise scan on non-ASCII input covers
/// everything without needing the bytewise engine.
///
/// # Errors
///
/// Returns [`MatcherError`] if the `daachorse` or `aho-corasick` automaton builders
/// encounter an internal error during construction.
fn compile_automata(
    dedup_patterns: &[Cow<'_, str>],
    value_map: &[u32],
) -> Result<(Option<BytewiseMatcher>, Option<CharwiseMatcher>), MatcherError> {
    let cap = dedup_patterns.len();
    let mut ascii_patvals: Vec<(&str, u32)> = Vec::with_capacity(cap);
    let mut non_ascii_patvals: Vec<(&str, u32)> = Vec::with_capacity(cap);

    for (dedup_idx, pattern) in dedup_patterns.iter().enumerate() {
        let value = value_map[dedup_idx];
        if pattern.as_ref().is_ascii() {
            ascii_patvals.push((pattern.as_ref(), value));
        } else {
            non_ascii_patvals.push((pattern.as_ref(), value));
        }
    }

    let has_ascii = !ascii_patvals.is_empty();
    let has_non_ascii = !non_ascii_patvals.is_empty();

    // When both classes are present, build the charwise engine over the full set.
    let full_charwise_patvals: Option<Vec<(&str, u32)>> = if has_ascii && has_non_ascii {
        Some(
            dedup_patterns
                .iter()
                .enumerate()
                .map(|(i, p)| (p.as_ref(), value_map[i]))
                .collect(),
        )
    } else {
        None
    };
    let charwise_source = full_charwise_patvals
        .as_deref()
        .unwrap_or(non_ascii_patvals.as_slice());

    let build_bytewise = move || -> Result<BytewiseMatcher, MatcherError> {
        build_current_bytewise(ascii_patvals, value_map.len())
    };

    let build_charwise = || -> Result<CharwiseMatcher, MatcherError> {
        Ok(CharwiseMatcher::DaacCharwise(
            CharwiseDoubleArrayAhoCorasickBuilder::new()
                .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                .build_with_values(charwise_source.iter().copied())
                .map_err(MatcherError::automaton_build)?,
        ))
    };

    match (has_ascii, has_non_ascii) {
        (false, false) => Ok((None, None)),
        (true, false) => Ok((Some(build_bytewise()?), None)),
        (false, true) => Ok((None, Some(build_charwise()?))),
        (true, true) => std::thread::scope(|s| {
            let bytewise_handle = s.spawn(build_bytewise);
            let charwise = build_charwise()?;
            let bytewise = bytewise_handle
                .join()
                .expect("bytewise automaton build panicked")?;
            Ok((Some(bytewise), Some(charwise)))
        }),
    }
}

fn build_current_bytewise(
    ascii_patvals: Vec<(&str, u32)>,
    _value_map_len: usize,
) -> Result<BytewiseMatcher, MatcherError> {
    #[cfg(feature = "dfa")]
    let mut ascii_ac_to_value: Vec<u32> = Vec::with_capacity(ascii_patvals.len());

    #[cfg(feature = "dfa")]
    for &(_, value) in &ascii_patvals {
        ascii_ac_to_value.push(value);
    }

    #[cfg(feature = "dfa")]
    if ascii_patvals.len() <= AC_DFA_PATTERN_THRESHOLD {
        return Ok(BytewiseMatcher::AcDfa {
            matcher: Box::new(
                AcDfaEngine::builder()
                    .match_kind(AhoCorasickMatchKind::Standard)
                    .build(ascii_patvals.iter().map(|(p, _)| p))
                    .map_err(MatcherError::automaton_build)?,
            ),
            to_value: ascii_ac_to_value,
        });
    }

    Ok(BytewiseMatcher::DaacBytewise(
        DoubleArrayAhoCorasickBuilder::new()
            .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
            .build_with_values(ascii_patvals)
            .map_err(MatcherError::automaton_build)?,
    ))
}

#[cfg(all(test, feature = "harry"))]
impl ScanPlan {
    /// Returns whether a Harry matcher was compiled for this plan.
    pub(super) fn has_harry(&self) -> bool {
        self.harry_matcher.is_some()
    }
}

#[cfg(all(test, feature = "harry"))]
mod tests {
    use super::*;

    fn compile_from_strings(patterns: &[&str]) -> ScanPlan {
        let dedup_patterns: Vec<Cow<'_, str>> =
            patterns.iter().map(|&p| Cow::Borrowed(p)).collect();
        let dedup_entries: Vec<Vec<PatternEntry>> = patterns.iter().map(|_| vec![]).collect();
        ScanPlan::compile(&dedup_patterns, dedup_entries).expect("compile should succeed")
    }

    #[test]
    fn harry_built_for_large_ascii_sets() {
        let patterns: Vec<String> = (0..64).map(|i| format!("token{i:02}")).collect();
        let refs: Vec<&str> = patterns.iter().map(String::as_str).collect();
        let plan = compile_from_strings(&refs);
        assert!(plan.has_harry(), "should build Harry for 64 ASCII patterns");
    }

    #[test]
    fn harry_not_built_for_small_sets() {
        let patterns: Vec<String> = (0..8).map(|i| format!("token{i:02}")).collect();
        let refs: Vec<&str> = patterns.iter().map(String::as_str).collect();
        let plan = compile_from_strings(&refs);
        assert!(
            !plan.has_harry(),
            "should not build Harry for < 64 patterns"
        );
    }

    #[test]
    fn harry_built_for_mixed_pattern_sets() {
        let ascii: Vec<String> = (0..32).map(|i| format!("token{i:02}")).collect();
        let cjk: Vec<String> = (0..32).map(|i| format!("测试{i:02}")).collect();
        let patterns: Vec<String> = ascii.into_iter().chain(cjk).collect();
        let refs: Vec<&str> = patterns.iter().map(String::as_str).collect();
        let plan = compile_from_strings(&refs);
        assert!(plan.has_harry(), "should build Harry for 64 mixed patterns");
        // AC engines are also built for for_each_match_value
        assert!(
            plan.charwise_matcher.is_some(),
            "charwise engine should exist for CJK patterns"
        );
    }
}
