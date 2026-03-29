//! Scan-engine compilation and match iteration for [`super::SimpleMatcher`].
//!
//! This module owns the Aho-Corasick automata that power Pass 1 (pattern scan) of the
//! two-pass matching pipeline. Two independent engines may be built:
//!
//! - **ASCII engine** ([`AsciiMatcher`]) — scans byte-wise over ASCII-only text. With the
//!   `dfa` feature enabled and pattern count ≤ [`AC_DFA_PATTERN_THRESHOLD`], this uses the
//!   `aho-corasick` crate's DFA for maximum throughput. Otherwise, it falls back to
//!   `daachorse`'s bytewise double-array Aho-Corasick.
//!
//! - **Non-ASCII engine** ([`NonAsciiMatcher`]) — scans character-wise using `daachorse`'s
//!   charwise automaton. When both ASCII and non-ASCII patterns exist, this engine is built
//!   over the **full** pattern set (not just the non-ASCII ones) so a single charwise pass
//!   can cover all patterns when the input text contains multi-byte characters.
//!
//! The [`ScanPlan`] struct bundles both engines together with the [`PatternIndex`] that
//! maps raw automaton values back to rule metadata.

use std::borrow::Cow;

#[cfg(feature = "dfa")]
use aho_corasick::{
    AhoCorasick, AhoCorasickBuilder, AhoCorasickKind, FindOverlappingIter as AcFindOverlappingIter,
    MatchKind as AhoCorasickMatchKind,
};
use daachorse::{
    DoubleArrayAhoCorasick, DoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
    bytewise::iter::{FindOverlappingIterator as BytewiseOverlappingIter, U8SliceIterator},
    charwise::{
        CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
        iter::{FindOverlappingIterator as CharwiseOverlappingIter, StrIterator},
    },
};

use crate::MatcherError;

use super::SearchMode;
use super::rule::{PatternEntry, PatternIndex};

/// Upper bound on pattern count where the `aho-corasick` DFA engine is still preferred.
///
/// Benchmarked on Apple M3 Max: AcDfa beats DaacBytewise by 14-22% up to ~5,000 ASCII
/// patterns on English text (both `search` overlapping iteration and `is_match` early
/// exit). Above ~5,000 the DFA's cache footprint grows enough that DaacBytewise wins.
/// Only relevant when the `dfa` feature is enabled.
#[cfg(feature = "dfa")]
const AC_DFA_PATTERN_THRESHOLD: usize = 5_000;

/// Compiled scan engines together with the pattern metadata they report into.
///
/// Immutable after construction. Shared across all threads via `Arc` or by virtue of
/// [`SimpleMatcher`](super::SimpleMatcher) being `Send + Sync`.
///
/// Either or both engines may be `None` when the corresponding pattern class is absent.
/// For example, if all patterns are pure ASCII, `non_ascii_matcher` will be `None`.
#[derive(Clone)]
pub(super) struct ScanPlan {
    /// Bytewise engine for ASCII-only text. `None` when no ASCII patterns exist.
    ascii_matcher: Option<AsciiMatcher>,
    /// Charwise engine for text containing multi-byte characters.
    ///
    /// When both ASCII and non-ASCII patterns exist, this engine contains the full
    /// pattern set so a single pass handles everything on non-ASCII input.
    /// `None` when no non-ASCII patterns exist **and** no ASCII patterns need charwise
    /// coverage.
    non_ascii_matcher: Option<NonAsciiMatcher>,
    /// Flat index mapping automaton raw values back to rule-entry metadata.
    patterns: PatternIndex,
}

/// ASCII-specific scan engine chosen at build time.
///
/// The variant is selected by [`compile_automata`] based on the `dfa` feature flag and
/// the number of ASCII patterns:
///
/// - [`AcDfa`](Self::AcDfa) — `aho-corasick` DFA. Fastest throughput but ~10x memory
///   vs NFA. Only used when the `dfa` feature is on and pattern count ≤
///   [`AC_DFA_PATTERN_THRESHOLD`].
/// - [`DaacBytewise`](Self::DaacBytewise) — `daachorse` bytewise double-array
///   Aho-Corasick. Lower memory, used as fallback.
#[derive(Clone)]
enum AsciiMatcher {
    /// `aho-corasick` DFA engine.
    ///
    /// The `aho-corasick` crate uses pattern indices (not user-supplied values) in its
    /// match output, so `to_value` maps pattern index → raw value.
    #[cfg(feature = "dfa")]
    AcDfa {
        matcher: AhoCorasick,
        to_value: Vec<u32>,
    },
    /// `daachorse` bytewise double-array engine with user-supplied `u32` values.
    DaacBytewise(DoubleArrayAhoCorasick<u32>),
}

/// Non-ASCII scan engine chosen at build time.
///
/// Currently only one variant exists. The enum wrapper allows future extension (e.g., an
/// `aho-corasick` charwise DFA) without changing call sites.
#[derive(Clone)]
enum NonAsciiMatcher {
    /// `daachorse` charwise double-array engine with user-supplied `u32` values.
    DaacCharwise(CharwiseDoubleArrayAhoCorasick<u32>),
}

/// Overlapping-iterator wrapper for ASCII scan engines.
///
/// Implements [`Iterator<Item = u32>`] yielding raw match values. The wrapper exists to
/// present a uniform interface across the two possible ASCII engine backends.
enum AsciiOverlappingIter<'a> {
    /// Wraps [`aho_corasick::FindOverlappingIter`] and translates pattern indices to values.
    #[cfg(feature = "dfa")]
    AcDfa {
        inner: AcFindOverlappingIter<'a, 'a>,
        to_value: &'a [u32],
    },
    /// Wraps [`daachorse`]'s bytewise overlapping iterator (values are already `u32`).
    DaacBytewise(BytewiseOverlappingIter<'a, U8SliceIterator<&'a str>, u32>),
}

/// Overlapping-iterator wrapper for the non-ASCII scan engine.
///
/// Implements [`Iterator<Item = u32>`] yielding raw match values.
enum NonAsciiOverlappingIter<'a> {
    /// Wraps [`daachorse`]'s charwise overlapping iterator (values are already `u32`).
    DaacCharwise(CharwiseOverlappingIter<'a, StrIterator<&'a str>, u32>),
}

/// Construction and query helpers for compiled scan engines.
impl ScanPlan {
    /// Compiles the ASCII and non-ASCII scan engines for the deduplicated pattern set.
    ///
    /// 1. Builds a [`PatternIndex`] from the raw entry buckets.
    /// 2. Builds the value map (direct-rule encoding where possible).
    /// 3. Delegates to [`compile_automata`] for actual automaton construction.
    pub(super) fn compile(
        dedup_patterns: &[Cow<'_, str>],
        dedup_entries: Vec<Vec<PatternEntry>>,
        mode: SearchMode,
    ) -> Result<Self, MatcherError> {
        let patterns = PatternIndex::new(dedup_entries);
        let value_map = patterns.build_value_map(mode);
        let (ascii_matcher, non_ascii_matcher) = compile_automata(dedup_patterns, &value_map)?;

        Ok(Self {
            ascii_matcher,
            non_ascii_matcher,
            patterns,
        })
    }

    /// Returns the pattern metadata referenced by the compiled scan engines.
    #[inline(always)]
    pub(super) fn patterns(&self) -> &PatternIndex {
        &self.patterns
    }

    /// Returns whether any compiled pattern matches `text`.
    ///
    /// Selects the engine based on whether a non-ASCII engine exists and whether `text`
    /// is pure ASCII. When no non-ASCII engine is present, always uses the ASCII engine
    /// (skipping the `text.is_ascii()` scan).
    #[inline(always)]
    pub(super) fn is_match(&self, text: &str) -> bool {
        if self.non_ascii_matcher.is_none() {
            return self
                .ascii_matcher
                .as_ref()
                .is_some_and(|matcher| matcher.is_match(text));
        }

        if text.is_ascii() {
            self.ascii_matcher
                .as_ref()
                .is_some_and(|matcher| matcher.is_match(text))
        } else {
            self.non_ascii_matcher
                .as_ref()
                .is_some_and(|matcher| matcher.is_match(text))
        }
    }

    /// Calls `on_value` for each raw match value produced by the chosen engine.
    ///
    /// Returns `true` if the callback requests early exit (i.e., `on_value` returned
    /// `true`). The `is_ascii` flag determines engine selection: when `true` or when no
    /// non-ASCII engine exists, the ASCII engine is used; otherwise the charwise engine
    /// handles the full scan.
    #[inline(always)]
    pub(super) fn for_each_match_value(
        &self,
        text: &str,
        is_ascii: bool,
        mut on_value: impl FnMut(u32) -> bool,
    ) -> bool {
        let use_ascii = self.non_ascii_matcher.is_none() || is_ascii;
        if use_ascii {
            if let Some(ref matcher) = self.ascii_matcher {
                for value in matcher.find_overlapping_iter(text) {
                    if on_value(value) {
                        return true;
                    }
                }
            }
        } else if let Some(ref matcher) = self.non_ascii_matcher {
            for value in matcher.find_overlapping_iter(text) {
                if on_value(value) {
                    return true;
                }
            }
        }

        false
    }
}

/// Iterator implementation for ASCII match streams.
impl Iterator for AsciiOverlappingIter<'_> {
    type Item = u32;

    /// Returns the next raw match value produced by the ASCII engine.
    ///
    /// # Safety (AcDfa variant)
    ///
    /// Uses `get_unchecked` on `to_value` with the pattern index from the `aho-corasick`
    /// match. This is safe because `to_value` was constructed with one entry per pattern
    /// in [`compile_automata`], so the pattern index is always in bounds.
    #[inline(always)]
    fn next(&mut self) -> Option<u32> {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { inner, to_value } => inner
                .next()
                // SAFETY: `to_value` has one entry per pattern; pattern index is always in bounds.
                .map(|hit| unsafe { *to_value.get_unchecked(hit.pattern().as_usize()) }),
            Self::DaacBytewise(iter) => iter.next().map(|hit| hit.value()),
        }
    }
}

/// Iterator implementation for non-ASCII match streams.
impl Iterator for NonAsciiOverlappingIter<'_> {
    type Item = u32;

    /// Returns the next raw match value produced by the non-ASCII engine.
    #[inline(always)]
    fn next(&mut self) -> Option<u32> {
        match self {
            Self::DaacCharwise(iter) => iter.next().map(|hit| hit.value()),
        }
    }
}

/// Query helpers for the chosen ASCII scan engine.
impl AsciiMatcher {
    /// Returns whether the ASCII engine matches `text`.
    #[inline(always)]
    fn is_match(&self, text: &str) -> bool {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, .. } => matcher.is_match(text),
            Self::DaacBytewise(matcher) => matcher.find_iter(text).next().is_some(),
        }
    }

    /// Creates the overlapping iterator for the ASCII engine.
    #[inline(always)]
    fn find_overlapping_iter<'a>(&'a self, text: &'a str) -> AsciiOverlappingIter<'a> {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, to_value } => AsciiOverlappingIter::AcDfa {
                inner: matcher.find_overlapping_iter(text),
                to_value,
            },
            Self::DaacBytewise(matcher) => {
                AsciiOverlappingIter::DaacBytewise(matcher.find_overlapping_iter(text))
            }
        }
    }
}

/// Query helpers for the chosen non-ASCII scan engine.
impl NonAsciiMatcher {
    /// Returns whether the non-ASCII engine matches `text`.
    #[inline(always)]
    fn is_match(&self, text: &str) -> bool {
        match self {
            Self::DaacCharwise(matcher) => matcher.find_iter(text).next().is_some(),
        }
    }

    /// Creates the overlapping iterator for the non-ASCII engine.
    #[inline(always)]
    fn find_overlapping_iter<'a>(&'a self, text: &'a str) -> NonAsciiOverlappingIter<'a> {
        match self {
            Self::DaacCharwise(matcher) => {
                NonAsciiOverlappingIter::DaacCharwise(matcher.find_overlapping_iter(text))
            }
        }
    }
}

/// Compiles the ASCII and non-ASCII automata from the deduplicated pattern list.
///
/// Patterns are partitioned by `is_ascii()`:
///
/// - **ASCII-only patterns** → [`AsciiMatcher`] (DFA or DAAC bytewise).
/// - **Non-ASCII patterns** → [`NonAsciiMatcher`] (DAAC charwise).
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
) -> Result<(Option<AsciiMatcher>, Option<NonAsciiMatcher>), MatcherError> {
    let mut ascii_patvals: Vec<(&str, u32)> = Vec::new();
    let mut non_ascii_patvals: Vec<(&str, u32)> = Vec::new();
    #[cfg(feature = "dfa")]
    let mut ascii_ac_to_value: Vec<u32> = Vec::new();

    for (dedup_idx, pattern) in dedup_patterns.iter().enumerate() {
        let value = value_map[dedup_idx];
        if pattern.as_ref().is_ascii() {
            #[cfg(feature = "dfa")]
            ascii_ac_to_value.push(value);
            ascii_patvals.push((pattern.as_ref(), value));
        } else {
            non_ascii_patvals.push((pattern.as_ref(), value));
        }
    }

    let full_charwise_patvals = if ascii_patvals.is_empty() || non_ascii_patvals.is_empty() {
        None
    } else {
        Some(
            dedup_patterns
                .iter()
                .enumerate()
                .map(|(dedup_idx, pattern)| (pattern.as_ref(), value_map[dedup_idx]))
                .collect::<Vec<_>>(),
        )
    };

    let ascii_matcher = if !ascii_patvals.is_empty() {
        #[cfg(feature = "dfa")]
        let engine = if ascii_patvals.len() <= AC_DFA_PATTERN_THRESHOLD {
            AsciiMatcher::AcDfa {
                matcher: AhoCorasickBuilder::new()
                    .kind(Some(AhoCorasickKind::DFA))
                    .match_kind(AhoCorasickMatchKind::Standard)
                    .build(ascii_patvals.iter().map(|(pattern, _)| pattern))
                    .map_err(MatcherError::automaton_build)?,
                to_value: ascii_ac_to_value,
            }
        } else {
            AsciiMatcher::DaacBytewise(
                DoubleArrayAhoCorasickBuilder::new()
                    .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                    .build_with_values(ascii_patvals)
                    .map_err(MatcherError::automaton_build)?,
            )
        };

        #[cfg(not(feature = "dfa"))]
        let engine = AsciiMatcher::DaacBytewise(
            DoubleArrayAhoCorasickBuilder::new()
                .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                .build_with_values(ascii_patvals)
                .map_err(MatcherError::automaton_build)?,
        );

        Some(engine)
    } else {
        None
    };

    let non_ascii_patvals = full_charwise_patvals
        .as_deref()
        .unwrap_or(non_ascii_patvals.as_slice());
    let non_ascii_matcher = if !non_ascii_patvals.is_empty() {
        Some(NonAsciiMatcher::DaacCharwise(
            CharwiseDoubleArrayAhoCorasickBuilder::new()
                .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                .build_with_values(non_ascii_patvals.iter().copied())
                .map_err(MatcherError::automaton_build)?,
        ))
    } else {
        None
    };

    Ok((ascii_matcher, non_ascii_matcher))
}
