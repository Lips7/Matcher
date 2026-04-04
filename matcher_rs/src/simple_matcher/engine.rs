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
    Anchored, MatchKind as AhoCorasickMatchKind, automaton::Automaton, dfa::DFA as AcDfaEngine,
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
/// net regression. Threshold raised from 7,000 to 15,000: DFA fits comfortably at
/// 15 K (~13 MB combined with charwise) while capturing a ~1.7× improvement over
/// daachorse for the 7 K–15 K range.
///
/// Only used when all patterns are pure ASCII; for mixed or non-ASCII pattern sets the
/// DFA state table would be much larger (3-byte UTF-8 sequences), so DaacBytewise is
/// always used in that case. Only relevant when the `dfa` feature is enabled.
#[cfg(feature = "dfa")]
const AC_DFA_PATTERN_THRESHOLD: usize = 15_000;

/// Compiled scan engines together with the pattern metadata they report into.
///
/// Immutable after construction. Shared across all threads via `Arc` or by virtue of
/// [`SimpleMatcher`](super::SimpleMatcher) being `Send + Sync`.
///
/// The charwise engine is always built (even for pure-ASCII pattern sets) so that
/// non-ASCII haystacks can be scanned at character granularity for ~1.6–1.9× throughput
/// over bytewise scanning (CJK characters are 3 UTF-8 bytes → 3 bytewise transitions
/// vs 1 charwise transition).
#[derive(Clone)]
pub(super) struct ScanPlan {
    /// Bytewise engine for ASCII patterns.
    /// `None` when no ASCII patterns exist.
    bytewise_matcher: Option<BytewiseMatcher>,
    /// Charwise engine for non-ASCII text. Always built from the **full** pattern set
    /// so a single charwise pass covers everything. `None` only when no patterns exist.
    charwise_matcher: Option<CharwiseMatcher>,
    /// `true` when every compiled pattern is pure ASCII. Gates Harry dispatch now
    /// that `charwise_matcher` is always present.
    #[cfg(feature = "harry")]
    all_patterns_ascii: bool,
    /// Harry column-vector SIMD engine for `is_match` fast path.
    ///
    /// Only built when all patterns are pure ASCII.
    /// When present and no DFA exists (pattern count > [`AC_DFA_PATTERN_THRESHOLD`]),
    /// `is_match` dispatches here instead of to the AC engines.
    /// `None` when non-ASCII patterns exist, the set is too small (< 64), or every
    /// pattern has length < 2.
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
        let all_patterns_ascii = dedup_patterns.iter().all(|p| p.is_ascii());
        #[cfg(feature = "harry")]
        let harry_matcher = if all_patterns_ascii {
            let patvals: Vec<(&str, u32)> = dedup_patterns
                .iter()
                .enumerate()
                .map(|(i, p)| (p.as_ref(), value_map[i]))
                .collect();
            HarryMatcher::build(&patvals).map(Box::new)
        } else {
            None
        };

        Ok(Self {
            bytewise_matcher,
            charwise_matcher,
            #[cfg(feature = "harry")]
            all_patterns_ascii,
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

    /// Returns the estimated heap memory in bytes owned by all scan engines.
    pub(super) fn heap_bytes(&self) -> usize {
        let bw = self.bytewise_matcher.as_ref().map_or(0, |m| m.heap_bytes());
        let cw = self.charwise_matcher.as_ref().map_or(0, |m| m.heap_bytes());
        #[cfg(feature = "harry")]
        let harry = self.harry_matcher.as_ref().map_or(0, |m| m.heap_bytes());
        #[cfg(not(feature = "harry"))]
        let harry = 0;
        bw + cw + harry + self.patterns.heap_bytes()
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
    /// 1. **Harry** — used when present (pure-ASCII patterns only, see
    ///    [`ScanPlan::compile`]) and either:
    ///    - no DFA engine exists (N > [`AC_DFA_PATTERN_THRESHOLD`]), **or**
    ///    - `text` contains non-ASCII bytes (`!text.is_ascii()`).
    ///
    ///    On non-ASCII haystacks Harry's column-0 early exit filters ~95% of
    ///    chunks, giving 3–4× throughput over AC at every pattern count.
    ///
    /// 2. **AC bytewise** — when text is ASCII.
    ///
    /// 3. **AC charwise** — when text contains multi-byte characters.
    #[inline(always)]
    pub(super) fn is_match(&self, text: &str) -> bool {
        #[cfg(feature = "harry")]
        if self
            .harry_matcher
            .as_ref()
            .is_some_and(|_| self.all_patterns_ascii && (!self.uses_dfa() || !text.is_ascii()))
        {
            return self.harry_matcher.as_ref().unwrap().is_match(text);
        }

        if text.is_ascii() {
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
    /// `true`). The `is_ascii` flag determines engine selection: bytewise for ASCII text,
    /// charwise for non-ASCII text.
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
        if is_ascii {
            if let Some(ref matcher) = self.bytewise_matcher {
                return matcher.for_each_match_value(text, on_value);
            }
        } else if let Some(ref matcher) = self.charwise_matcher {
            return matcher.for_each_match_value(text, on_value);
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
            Self::AcDfa { matcher, .. } => {
                let mut sid = matcher.start_state(Anchored::No).unwrap();
                for &byte in text.as_bytes() {
                    sid = matcher.next_state(Anchored::No, sid, byte);
                    if matcher.is_special(sid) {
                        if matcher.is_match(sid) {
                            return true;
                        }
                        if matcher.is_dead(sid) {
                            return false;
                        }
                    }
                }
                false
            }
            Self::DaacBytewise(matcher) => matcher.find_iter(text).next().is_some(),
        }
    }

    /// Calls `on_value` for each raw match value produced by the bytewise engine.
    #[inline(always)]
    fn for_each_match_value(&self, text: &str, mut on_value: impl FnMut(u32) -> bool) -> bool {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, to_value } => {
                let mut sid = matcher.start_state(Anchored::No).unwrap();
                for &byte in text.as_bytes() {
                    sid = matcher.next_state(Anchored::No, sid, byte);
                    if matcher.is_special(sid) && matcher.is_match(sid) {
                        for i in 0..matcher.match_len(sid) {
                            let pid = matcher.match_pattern(sid, i);
                            // SAFETY: `to_value` has one entry per pattern; pattern index is always in bounds.
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
                for hit in matcher.find_overlapping_iter(text) {
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

    fn heap_bytes(&self) -> usize {
        match self {
            Self::DaacCharwise(matcher) => matcher.heap_bytes(),
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

    // Always build charwise over the full pattern set so non-ASCII haystacks
    // benefit from character-granularity scanning (~1.6–1.9× on CJK text).
    let all_patvals: Vec<(&str, u32)> = dedup_patterns
        .iter()
        .enumerate()
        .map(|(i, p)| (p.as_ref(), value_map[i]))
        .collect();

    let build_bytewise = move || -> Result<BytewiseMatcher, MatcherError> {
        build_current_bytewise(ascii_patvals, value_map.len())
    };

    let build_charwise = |source: Vec<(&str, u32)>| -> Result<CharwiseMatcher, MatcherError> {
        Ok(CharwiseMatcher::DaacCharwise(
            CharwiseDoubleArrayAhoCorasickBuilder::new()
                .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                .build_with_values(source)
                .map_err(MatcherError::automaton_build)?,
        ))
    };

    let has_patterns = has_ascii || has_non_ascii;

    match (has_ascii, has_patterns) {
        (_, false) => Ok((None, None)),
        (true, true) => std::thread::scope(|s| {
            let bytewise_handle = s.spawn(build_bytewise);
            let charwise = build_charwise(all_patvals)?;
            let bytewise = bytewise_handle
                .join()
                .expect("bytewise automaton build panicked")?;
            Ok((Some(bytewise), Some(charwise)))
        }),
        (false, true) => Ok((None, Some(build_charwise(all_patvals)?))),
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
    fn harry_not_built_for_mixed_pattern_sets() {
        let ascii: Vec<String> = (0..32).map(|i| format!("token{i:02}")).collect();
        let cjk: Vec<String> = (0..32).map(|i| format!("测试{i:02}")).collect();
        let patterns: Vec<String> = ascii.into_iter().chain(cjk).collect();
        let refs: Vec<&str> = patterns.iter().map(String::as_str).collect();
        let plan = compile_from_strings(&refs);
        assert!(
            !plan.has_harry(),
            "should not build Harry for mixed ASCII+CJK patterns"
        );
        assert!(
            plan.charwise_matcher.is_some(),
            "charwise engine should exist for CJK patterns"
        );
    }
}
