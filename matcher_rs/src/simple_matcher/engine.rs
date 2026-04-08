//! Scan-engine compilation and match iteration for [`super::SimpleMatcher`].
//!
//! This module owns the Aho-Corasick automata that power Pass 1 (pattern scan)
//! of the two-pass matching pipeline. Two independent engines are compiled:
//!
//! - **Bytewise engine** ([`BytewiseMatcher`]) — scans byte-by-byte over ASCII
//!   patterns. With the `dfa` feature enabled, this uses the `aho-corasick`
//!   crate's DFA for maximum throughput. Otherwise it falls back to
//!   `daachorse`'s bytewise double-array Aho-Corasick.
//!
//! - **Charwise engine** ([`CharwiseMatcher`]) — scans character-wise using
//!   `daachorse`'s charwise automaton. Always built over the **full** pattern
//!   set so a single charwise pass covers everything on non-ASCII input. CJK
//!   characters are 3 UTF-8 bytes, so charwise does 1 state transition vs 3 for
//!   bytewise — ~1.6–1.9× faster on CJK text.
//!
//! The [`ScanPlan`] struct bundles both engines together with the
//! [`PatternIndex`] that maps raw automaton values back to rule metadata.
//!
//! # Engine selection
//!
//! [`ScanPlan::is_match`] and [`ScanPlan::for_each_match_value`] accept an
//! `is_ascii` flag (computed once per text variant by [`super::search`] via
//! `str::is_ascii()`). The charwise engine is always built, so `is_ascii`
//! determines which engine is used: bytewise for ASCII text, charwise for
//! non-ASCII text.

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

use super::{
    encoding::{DIRECT_BOUNDARY_MASK, DIRECT_BOUNDARY_SHIFT, DIRECT_RULE_MASK},
    pattern::{PatternEntry, PatternIndex},
};
use crate::MatcherError;

/// Non-ASCII byte density threshold for switching from bytewise to charwise
/// engine.
///
/// Calibrated from 8,932-point characterization sweep (4 engines × 12 sizes ×
/// 11 CJK densities). At ~40% CJK characters the non-ASCII byte fraction is
/// `0.4×3 / (0.4×3 + 0.6×1) ≈ 0.667`. Charwise overtakes DFA+Teddy at this
/// crossover, consistent across pattern sizes and both `search` and `is_match`
/// modes.
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

/// Compiled scan engines together with the pattern metadata they report into.
///
/// Immutable after construction. Shared across all threads via `Arc` or by
/// virtue of [`SimpleMatcher`](super::SimpleMatcher) being `Send + Sync`.
///
/// The charwise engine is always built (even for pure-ASCII pattern sets) so
/// that non-ASCII haystacks can be scanned at character granularity for
/// ~1.6–1.9× throughput over bytewise scanning (CJK characters are 3 UTF-8
/// bytes → 3 bytewise transitions vs 1 charwise transition).
///
/// # Performance
///
/// - **Bytewise DFA** (when `dfa` feature enabled): ~1.7–1.9× faster than DAAC
///   bytewise on ASCII text, but ~17× more memory.
/// - **Charwise DAAC**: 1 state transition per character (vs 3 bytewise for
///   CJK), yielding ~1.6–1.9× throughput on non-ASCII text.
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
    /// Used only for the `is_match` fast-return: when all patterns are ASCII
    /// and the text contains zero ASCII bytes, no match is possible.
    all_patterns_ascii: bool,
    /// Flat index mapping automaton raw values back to rule-entry metadata.
    patterns: PatternIndex,
}

/// Bytewise scan engine chosen at build time.
///
/// Bytewise scan engines. DAAC bytewise is always built (supports streaming).
/// DFA is built alongside it when the `dfa` feature is enabled (1.7–3.3× faster
/// for non-streaming scan).
#[derive(Clone)]
struct BytewiseMatcher {
    daac: DoubleArrayAhoCorasick<u32>,
    #[cfg(feature = "dfa")]
    dfa: Option<(AcEngine, Vec<u32>)>,
}

type CharwiseMatcher = CharwiseDoubleArrayAhoCorasick<u32>;

/// Construction and query helpers for compiled scan engines.
impl ScanPlan {
    /// Compiles the bytewise and charwise scan engines for the deduplicated
    /// pattern set.
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

    /// Returns whether the bytewise engine has a DFA backend available.
    ///
    /// When `true`, the caller should prefer materialized scan over streaming
    /// at low non-ASCII density — DFA+Teddy is 2–5× faster than DAAC bytewise
    /// streaming on ASCII-heavy text, outweighing the allocation cost.
    #[inline(always)]
    pub(super) fn has_dfa(&self) -> bool {
        #[cfg(feature = "dfa")]
        {
            self.bytewise_matcher
                .as_ref()
                .is_some_and(|m| m.dfa.is_some())
        }
        #[cfg(not(feature = "dfa"))]
        {
            false
        }
    }

    /// Returns the estimated heap memory in bytes owned by all scan engines.
    pub(super) fn heap_bytes(&self) -> usize {
        let bw = self.bytewise_matcher.as_ref().map_or(0, |m| m.heap_bytes());
        let cw = self.charwise_matcher.as_ref().map_or(0, |m| m.heap_bytes());
        bw + cw + self.patterns.heap_bytes()
    }

    /// Returns whether any compiled pattern matches `text`.
    ///
    /// Density-based engine dispatch (same logic as
    /// [`Self::for_each_match_value`]):
    /// - **All-ASCII patterns + 100% non-ASCII text**: `return false` — ASCII
    ///   patterns cannot match text with zero ASCII bytes (UTF-8 guarantees
    ///   bytes ≥0x80 never encode ASCII codepoints).
    /// - **Low density** (≤ [`CHARWISE_DENSITY_THRESHOLD`]): bytewise DFA+Teddy
    ///   (1.7–2.5× faster on ASCII-heavy text).
    /// - **High density** (> [`CHARWISE_DENSITY_THRESHOLD`]): charwise engine
    ///   (1.3–2.5× faster at ≥40% CJK characters).
    #[inline(always)]
    pub(super) fn is_match(&self, text: &str) -> bool {
        let density = text_non_ascii_density(text);
        if self.all_patterns_ascii && density >= 1.0 && text.bytes().all(|b| b >= 0x80) {
            return false;
        }
        if density <= CHARWISE_DENSITY_THRESHOLD {
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
    /// density-based: bytewise for low non-ASCII density (≤
    /// [`CHARWISE_DENSITY_THRESHOLD`]), charwise for high density. When
    /// `all_patterns_ascii` and the text is entirely non-ASCII, returns
    /// `false` without scanning (no ASCII pattern can match).
    #[inline(always)]
    pub(super) fn for_each_match_value(
        &self,
        text: &str,
        density: f32,
        on_value: impl FnMut(u32, usize, usize) -> bool,
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

    /// Calls `on_value` for each raw match value from a streaming byte
    /// iterator.
    ///
    /// Used by the fused delete-scan path. Always uses DAAC bytewise (DFA has
    /// no streaming API). Falls back to charwise for high-density text.
    #[inline(always)]
    pub(super) fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        density: f32,
        on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        if density <= CHARWISE_DENSITY_THRESHOLD {
            if let Some(ref matcher) = self.bytewise_matcher {
                return matcher.for_each_match_value_from_iter(iter, on_value);
            }
        } else if let Some(ref matcher) = self.charwise_matcher {
            return matcher.for_each_match_value_from_iter(iter, on_value);
        }
        false
    }

    /// AllSimple-specialized scan: yields rule indices directly.
    ///
    /// Every raw value is assumed to carry `DIRECT_RULE_BIT`; the callback
    /// receives the extracted `rule_idx` (no early exit, no indirect
    /// dispatch).
    #[inline(always)]
    pub(super) fn for_each_rule_idx_simple(
        &self,
        text: &str,
        density: f32,
        on_rule: impl FnMut(usize, u8, usize, usize),
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
///
/// Non-streaming methods prefer DFA (when available) for acceleration.
/// Streaming uses DAAC bytewise (DFA has no `_from_iter` API).
impl BytewiseMatcher {
    #[inline(always)]
    fn is_match(&self, text: &str) -> bool {
        #[cfg(feature = "dfa")]
        if let Some((ref matcher, _)) = self.dfa {
            return matcher.is_match(text);
        }
        self.daac.find_iter(text).next().is_some()
    }

    #[inline(always)]
    fn for_each_match_value(
        &self,
        text: &str,
        mut on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        #[cfg(feature = "dfa")]
        if let Some((ref matcher, ref to_value)) = self.dfa {
            for m in matcher.find_overlapping_iter(text) {
                // SAFETY: `to_value` has one entry per pattern; pattern index is always in
                // bounds.
                let value = unsafe { *to_value.get_unchecked(m.pattern().as_usize()) };
                if on_value(value, m.start(), m.end()) {
                    return true;
                }
            }
            return false;
        }
        for hit in self.daac.find_overlapping_iter(text) {
            if on_value(hit.value(), hit.start(), hit.end()) {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    fn for_each_rule_idx_simple(
        &self,
        text: &str,
        mut on_rule: impl FnMut(usize, u8, usize, usize),
    ) {
        #[cfg(feature = "dfa")]
        if let Some((ref matcher, ref to_value)) = self.dfa {
            for m in matcher.find_overlapping_iter(text) {
                // SAFETY: `to_value` has one entry per pattern; pattern index is always in
                // bounds.
                let value = unsafe { *to_value.get_unchecked(m.pattern().as_usize()) };
                let boundary = ((value & DIRECT_BOUNDARY_MASK) >> DIRECT_BOUNDARY_SHIFT) as u8;
                on_rule(
                    (value & DIRECT_RULE_MASK) as usize,
                    boundary,
                    m.start(),
                    m.end(),
                );
            }
            return;
        }
        for hit in self.daac.find_overlapping_iter(text) {
            let value = hit.value();
            let boundary = ((value & DIRECT_BOUNDARY_MASK) >> DIRECT_BOUNDARY_SHIFT) as u8;
            on_rule(
                (value & DIRECT_RULE_MASK) as usize,
                boundary,
                hit.start(),
                hit.end(),
            );
        }
    }

    /// Streaming: always uses DAAC bytewise (DFA has no streaming API).
    #[inline(always)]
    fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        mut on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        for hit in self.daac.find_overlapping_iter_from_iter(iter) {
            if on_value(hit.value(), hit.start(), hit.end()) {
                return true;
            }
        }
        false
    }

    fn heap_bytes(&self) -> usize {
        let total = self.daac.heap_bytes();
        #[cfg(feature = "dfa")]
        if let Some((ref matcher, ref to_value)) = self.dfa {
            return total + matcher.memory_usage() + to_value.capacity() * size_of::<u32>();
        }
        total
    }
}

/// Query helpers for the charwise scan engine.
trait CharwiseMatcherExt {
    fn is_match_text(&self, text: &str) -> bool;
    fn for_each_match_value(
        &self,
        text: &str,
        on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool;
    fn for_each_rule_idx_simple(&self, text: &str, on_rule: impl FnMut(usize, u8, usize, usize));
}

/// Streaming query helpers for the charwise scan engine.
trait CharwiseMatcherStreamExt {
    fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool;
}

impl CharwiseMatcherStreamExt for CharwiseMatcher {
    #[inline(always)]
    fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        mut on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        // SAFETY: The streaming iterators (DeleteFilterIterator,
        // NormalizeFilterIterator) yield valid UTF-8: delete outputs a
        // subsequence of complete codepoints; normalize outputs unmapped
        // codepoints verbatim plus valid UTF-8 replacement strings.
        for hit in unsafe { self.find_overlapping_iter_from_iter(iter) } {
            if on_value(hit.value(), hit.start(), hit.end()) {
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
    fn for_each_match_value(
        &self,
        text: &str,
        mut on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        for hit in self.find_overlapping_iter(text) {
            if on_value(hit.value(), hit.start(), hit.end()) {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    fn for_each_rule_idx_simple(
        &self,
        text: &str,
        mut on_rule: impl FnMut(usize, u8, usize, usize),
    ) {
        for hit in self.find_overlapping_iter(text) {
            let value = hit.value();
            let boundary = ((value & DIRECT_BOUNDARY_MASK) >> DIRECT_BOUNDARY_SHIFT) as u8;
            on_rule(
                (value & DIRECT_RULE_MASK) as usize,
                boundary,
                hit.start(),
                hit.end(),
            );
        }
    }
}

/// Compiles the bytewise and charwise automata from the deduplicated pattern
/// list.
///
/// - **ASCII patterns** → [`BytewiseMatcher`] (DFA or DAAC bytewise).
/// - **All patterns** → [`CharwiseMatcher`] (DAAC charwise), always built from
///   the full pattern set so that non-ASCII haystacks benefit from
///   character-granularity scanning (~1.6–1.9× faster on CJK text vs bytewise).
///
/// # Panics
///
/// Panics if the bytewise automaton build thread panics internally. This should
/// not occur under normal operation — it indicates a bug in the underlying
/// `daachorse` or `aho-corasick` builder.
///
/// # Errors
///
/// Returns [`MatcherError`] if the `daachorse` or `aho-corasick` automaton
/// builders encounter an internal error during construction.
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
/// Always builds DAAC bytewise (needed for streaming). With the `dfa` feature,
/// also builds an `aho-corasick` DFA (1.7–3.3× faster for non-streaming scan).
fn build_current_bytewise(all_patvals: Vec<(&str, u32)>) -> Result<BytewiseMatcher, MatcherError> {
    let daac = DoubleArrayAhoCorasickBuilder::new()
        .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
        .build_with_values(all_patvals.clone())
        .map_err(MatcherError::automaton_build)?;

    #[cfg(feature = "dfa")]
    let dfa = {
        let to_value: Vec<u32> = all_patvals.iter().map(|&(_, v)| v).collect();
        Some((
            AhoCorasickBuilder::new()
                .kind(Some(AhoCorasickKind::DFA))
                .match_kind(AhoCorasickMatchKind::Standard)
                .build(all_patvals.iter().map(|(p, _)| p))
                .map_err(MatcherError::automaton_build)?,
            to_value,
        ))
    };

    Ok(BytewiseMatcher {
        daac,
        #[cfg(feature = "dfa")]
        dfa,
    })
}
