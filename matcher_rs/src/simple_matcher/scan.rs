//! Scan-engine compilation and match iteration for [`super::SimpleMatcher`].
//!
//! This module owns the Aho-Corasick automata that power Pass 1 (pattern scan)
//! of the two-pass matching pipeline. Two independent engines are compiled:
//!
//! - **Bytewise engine** ([`BytewiseMatcher`]) — scans byte-by-byte over the
//!   full pattern set. With the `dfa` feature enabled, this builds an
//!   `aho-corasick` `dfa::DFA` and uses its low-level `Automaton` API for
//!   maximum throughput. When no Teddy SIMD prefilter is active (>100
//!   patterns), both materialized and fused streaming paths use a custom
//!   `next_state` loop, eliminating iterator overhead and enabling DFA use on
//!   the fused path. When a prefilter is active, `try_find_overlapping` is used
//!   instead so Teddy can skip non-matching regions. Without `dfa`, falls back
//!   to `daachorse`'s bytewise double-array Aho-Corasick.
//!
//! - **Charwise engine** ([`CharwiseMatcher`]) — scans character-wise using
//!   `daachorse`'s charwise automaton. Also built over the **full** pattern
//!   set. CJK characters are 3 UTF-8 bytes, so charwise does 1 state transition
//!   vs 3 for bytewise — ~1.6–1.9× faster on CJK-heavy text.
//!
//! The [`ScanPlan`] struct bundles both engines together with the
//! [`PatternIndex`] that maps raw automaton values back to rule metadata.
//!
//! # Engine selection
//!
//! [`ScanPlan::is_match`] and [`ScanPlan::for_each_match_value`] use a SIMD
//! character density scan ([`text_char_density`]) to select the engine. When
//! the character density (chars/bytes) is ≥ [`CHARWISE_DENSITY_THRESHOLD`]
//! (0.55, ~40% CJK characters) the bytewise engine is used; below the
//! threshold the charwise engine is selected.

use std::borrow::Cow;

#[cfg(feature = "dfa")]
use aho_corasick::{
    Anchored, Input, MatchKind as AcMatchKind,
    automaton::{Automaton as _, OverlappingState},
    dfa::{Builder as AcDfaBuilder, DFA as AcDfa},
};
use daachorse::{
    DoubleArrayAhoCorasick as BytewiseDAACEngine,
    DoubleArrayAhoCorasickBuilder as BytewiseDAACBuilder, MatchKind as DAACMatchKind,
    charwise::{
        CharwiseDoubleArrayAhoCorasick as CharwiseDAACEngine,
        CharwiseDoubleArrayAhoCorasickBuilder as CharwiseDAACBuilder,
    },
};

use super::pattern::{PatternEntry, PatternIndex};
use crate::MatcherError;

/// Character density threshold for switching from bytewise to charwise engine.
///
/// Calibrated from 8,932-point characterization sweep (4 engines × 12 sizes ×
/// 11 CJK densities). At ~40% CJK characters the character density (chars per
/// byte) is `1 / (0.4×3 + 0.6×1) ≈ 0.556`. Charwise overtakes DFA at
/// this crossover, consistent across pattern sizes and both `search` and
/// `is_match` modes.
pub(super) const CHARWISE_DENSITY_THRESHOLD: f32 = 0.55;

/// Computes character density (codepoints / bytes) using SIMD via `bytecount`.
///
/// Returns a value in `(0.0, 1.0]` for non-empty text: 1.0 = pure ASCII,
/// lower values indicate more multi-byte characters (e.g. 0.33 for pure
/// 3-byte CJK). Returns 1.0 for empty text.
#[inline(always)]
pub(super) fn text_char_density(text: &str) -> f32 {
    let bytes = text.as_bytes();
    let len = bytes.len();
    if len == 0 {
        return 1.0;
    }
    bytecount::num_chars(bytes) as f32 / len as f32
}

// ── Unified scan trait ──────────────────────────────────────────────────

/// Common query interface implemented by both bytewise and charwise engines.
trait ScanEngine {
    /// Returns whether any compiled pattern matches `text`.
    fn is_match(&self, text: &str) -> bool;

    /// Calls `on_value(raw_value, start, end)` for each overlapping match in
    /// `text`. Returns `true` on early exit.
    fn for_each_match_value(
        &self,
        text: &str,
        on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool;

    /// Streaming variant of
    /// [`for_each_match_value`](Self::for_each_match_value) from a byte
    /// iterator. With `dfa` and no prefilter, uses the DFA's `next_state` loop
    /// directly. Otherwise falls back to DAAC.
    fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool;

    /// Returns the estimated heap memory in bytes owned by this engine.
    fn heap_bytes(&self) -> usize;
}

// ── Bytewise DFA engine ─────────────────────────────────────────────────

/// DFA component of the bytewise scan engine.
///
/// Owns the `aho-corasick` DFA and its value map. Accessed via the low-level
/// `Automaton` API (`next_state`, `is_special`, `is_match`, etc.) for maximum
/// throughput. All DFA logic lives here; `BytewiseMatcher` delegates to it.
#[cfg(feature = "dfa")]
#[derive(Clone)]
struct BytewiseDFAEngine {
    dfa: AcDfa,
    /// Maps DFA pattern index → raw match value (bridges `aho-corasick` pattern
    /// ids to our encoding).
    dfa_to_value: Vec<u32>,
    /// Whether the DFA has a Teddy SIMD prefilter. When true, Teddy can skip
    /// non-matching regions — the custom `next_state` loop cannot replicate
    /// this, so materialized text paths use `try_find_overlapping` instead.
    has_prefilter: bool,
}

#[cfg(feature = "dfa")]
impl BytewiseDFAEngine {
    fn is_match(&self, text: &str) -> bool {
        self.dfa
            .try_find(&Input::new(text))
            .is_ok_and(|m| m.is_some())
    }

    /// Prefilter-aware overlapping scan over materialized text.
    ///
    /// With Teddy prefilter: drives `try_find_overlapping` (Teddy skips
    /// non-matching regions). Without: custom `next_state` loop.
    #[inline(always)]
    fn for_each_match_value(
        &self,
        text: &str,
        mut on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        if self.has_prefilter {
            let input = Input::new(text);
            let mut state = OverlappingState::start();
            loop {
                if self.dfa.try_find_overlapping(&input, &mut state).is_err() {
                    break;
                }
                match state.get_match() {
                    None => break,
                    Some(m) => {
                        let pid = m.pattern().as_usize();
                        // SAFETY: `pid` is a pattern id from the DFA; bounded by construction.
                        unsafe { core::hint::assert_unchecked(pid < self.dfa_to_value.len()) };
                        let value = self.dfa_to_value[pid];
                        if on_value(value, m.start(), m.end()) {
                            return true;
                        }
                    }
                }
            }
            false
        } else {
            // No prefilter: is_special fires only for dead/match states (never
            // start states), so the loop is both correct and branch-minimal.
            self.scan(text.as_bytes(), on_value)
        }
    }

    /// Custom `next_state` scan from a streaming byte iterator.
    ///
    /// Only called when `!has_prefilter` — the caller checks before delegating.
    #[cfg_attr(feature = "_profile_boundaries", inline(never))]
    #[cfg_attr(not(feature = "_profile_boundaries"), inline(always))]
    fn scan_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        mut on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        let anchored = Anchored::No;
        let mut sid = match self.dfa.start_state(anchored) {
            Ok(s) => s,
            Err(_) => return false,
        };
        for (pos, byte) in iter.enumerate() {
            sid = self.dfa.next_state(anchored, sid, byte);
            if self.dfa.is_special(sid) {
                if self.dfa.is_dead(sid) {
                    break;
                }
                if self.dfa.is_match(sid) {
                    let end = pos + 1;
                    for i in 0..self.dfa.match_len(sid) {
                        let pid = self.dfa.match_pattern(sid, i);
                        let start = end - self.dfa.pattern_len(pid);
                        // SAFETY: pid is a DFA pattern id; bounded by construction.
                        let value = unsafe { *self.dfa_to_value.get_unchecked(pid.as_usize()) };
                        if on_value(value, start, end) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn heap_bytes(&self) -> usize {
        self.dfa.memory_usage() + self.dfa_to_value.capacity() * size_of::<u32>()
    }

    /// Custom `next_state` scan over materialized bytes.
    ///
    /// In the DFA, match states encode all overlapping hits (failure-link
    /// matches baked in during construction), so `0..match_len(sid)` yields
    /// the complete overlapping set.
    #[cfg_attr(feature = "_profile_boundaries", inline(never))]
    #[cfg_attr(not(feature = "_profile_boundaries"), inline(always))]
    fn scan(&self, text: &[u8], mut on_value: impl FnMut(u32, usize, usize) -> bool) -> bool {
        let anchored = Anchored::No;
        let mut sid = match self.dfa.start_state(anchored) {
            Ok(s) => s,
            Err(_) => return false,
        };
        for (pos, &byte) in text.iter().enumerate() {
            sid = self.dfa.next_state(anchored, sid, byte);
            if self.dfa.is_special(sid) {
                if self.dfa.is_dead(sid) {
                    break;
                }
                if self.dfa.is_match(sid) {
                    let end = pos + 1;
                    for i in 0..self.dfa.match_len(sid) {
                        let pid = self.dfa.match_pattern(sid, i);
                        let start = end - self.dfa.pattern_len(pid);
                        // SAFETY: pid is a DFA pattern id; bounded by construction.
                        let value = unsafe { *self.dfa_to_value.get_unchecked(pid.as_usize()) };
                        if on_value(value, start, end) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

// ── Bytewise engine ─────────────────────────────────────────────────────

/// Bytewise scan engine. DAAC bytewise is always built (supports streaming).
/// With the `dfa` feature, a [`BytewiseDFAEngine`] is built alongside it
/// (1.7–3.3× faster) and takes over all non-DAAC paths.
#[derive(Clone)]
struct BytewiseMatcher {
    /// DAAC bytewise automaton. Always built — used for streaming when the DFA
    /// has a Teddy prefilter (Teddy needs materialized text).
    daac: BytewiseDAACEngine<u32>,
    #[cfg(feature = "dfa")]
    dfa_engine: BytewiseDFAEngine,
}

impl ScanEngine for BytewiseMatcher {
    #[inline(always)]
    fn is_match(&self, text: &str) -> bool {
        #[cfg(feature = "dfa")]
        {
            self.dfa_engine.is_match(text)
        }
        #[cfg(not(feature = "dfa"))]
        {
            self.daac.find_iter(text).next().is_some()
        }
    }

    #[inline(always)]
    fn for_each_match_value(
        &self,
        text: &str,
        on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        #[cfg(feature = "dfa")]
        {
            self.dfa_engine.for_each_match_value(text, on_value)
        }
        #[cfg(not(feature = "dfa"))]
        {
            let mut on_value = on_value;
            for hit in self.daac.find_overlapping_iter(text) {
                if on_value(hit.value(), hit.start(), hit.end()) {
                    return true;
                }
            }
            false
        }
    }

    #[inline(always)]
    fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        mut on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        // DFA + no prefilter: stream bytes through custom next_state loop,
        // avoiding materialization cost.
        #[cfg(feature = "dfa")]
        if !self.dfa_engine.has_prefilter {
            return self.dfa_engine.scan_from_iter(iter, on_value);
        }
        for hit in self.daac.find_overlapping_iter_from_iter(iter) {
            if on_value(hit.value(), hit.start(), hit.end()) {
                return true;
            }
        }
        false
    }

    fn heap_bytes(&self) -> usize {
        let daac = self.daac.heap_bytes();
        #[cfg(feature = "dfa")]
        {
            daac + self.dfa_engine.heap_bytes()
        }
        #[cfg(not(feature = "dfa"))]
        daac
    }
}

// ── Charwise engine ─────────────────────────────────────────────────────

type CharwiseMatcher = CharwiseDAACEngine<u32>;

impl ScanEngine for CharwiseMatcher {
    fn is_match(&self, text: &str) -> bool {
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

    fn heap_bytes(&self) -> usize {
        CharwiseDAACEngine::heap_bytes(self)
    }
}

// ── Engines bundle ──────────────────────────────────────────────────────

/// Both compiled scan engines. Always built together from the full pattern set.
#[derive(Clone)]
struct Engines {
    bytewise: BytewiseMatcher,
    charwise: CharwiseMatcher,
}

impl Engines {
    /// Returns whether the bytewise DFA has a Teddy SIMD prefilter.
    fn has_dfa_prefilter(&self) -> bool {
        #[cfg(feature = "dfa")]
        {
            self.bytewise.dfa_engine.has_prefilter
        }
        #[cfg(not(feature = "dfa"))]
        {
            false
        }
    }
}

/// Dispatches to the bytewise or charwise engine based on character density.
///
/// Expands to: `if density >= threshold { bytewise.$method } else {
/// charwise.$method }`. Higher density = more ASCII-like = bytewise.
/// Avoids `dyn ScanEngine` (methods have `impl Trait` params → not
/// object-safe).
macro_rules! dispatch {
    ($engines:expr, $density:expr, $method:ident ($($arg:expr),*)) => {
        if $density >= CHARWISE_DENSITY_THRESHOLD {
            ScanEngine::$method(&$engines.bytewise, $($arg),*)
        } else {
            ScanEngine::$method(&$engines.charwise, $($arg),*)
        }
    };
}

// ── ScanPlan ────────────────────────────────────────────────────────────

/// Compiled scan engines together with the pattern metadata they report into.
///
/// Immutable after construction. Shared across all threads via `Arc` or by
/// virtue of [`SimpleMatcher`](super::SimpleMatcher) being `Send + Sync`.
///
/// Both engines are always built from the full pattern set. The charwise
/// engine gives ~1.6–1.9× throughput over bytewise on CJK-heavy text (3 UTF-8
/// bytes → 1 charwise transition). Engine selection is density-based at
/// runtime: bytewise for ≤ [`CHARWISE_DENSITY_THRESHOLD`], charwise above.
#[derive(Clone)]
pub(super) struct ScanPlan {
    engines: Engines,
    /// Flat index mapping automaton raw values back to rule-entry metadata.
    patterns: PatternIndex,
}

impl ScanPlan {
    /// Compiles the bytewise and charwise scan engines for the deduplicated
    /// pattern set.
    ///
    /// # Panics
    ///
    /// Panics if `dedup_patterns` is empty. The caller must reject empty
    /// pattern sets before calling this function.
    pub(super) fn compile(
        dedup_patterns: &[Cow<'_, str>],
        dedup_entries: Vec<Vec<PatternEntry>>,
        rule_info: &[super::rule::RuleInfo],
    ) -> Result<Self, MatcherError> {
        debug_assert!(
            !dedup_patterns.is_empty(),
            "ScanPlan::compile called with zero patterns"
        );

        let patterns = PatternIndex::new(dedup_entries);
        let value_map = patterns.build_value_map(rule_info);
        let engines = compile_automata(dedup_patterns, &value_map)?;

        Ok(Self { engines, patterns })
    }

    /// Returns the pattern metadata referenced by the compiled scan engines.
    pub(super) fn patterns(&self) -> &PatternIndex {
        &self.patterns
    }

    /// Returns the estimated heap memory in bytes owned by all scan engines.
    pub(super) fn heap_bytes(&self) -> usize {
        self.engines.bytewise.heap_bytes()
            + self.engines.charwise.heap_bytes()
            + self.patterns.heap_bytes()
    }

    /// Returns whether any compiled pattern matches `text`.
    ///
    /// Density-based engine dispatch: bytewise for high character density
    /// (≥ [`CHARWISE_DENSITY_THRESHOLD`]), charwise below.
    /// Skips TLS state entirely — used as a fast path for
    /// `SimpleMatcher::is_match` when no text transforms are needed.
    #[inline(always)]
    pub(super) fn is_match(&self, text: &str) -> bool {
        let density = text_char_density(text);
        dispatch!(self.engines, density, is_match(text))
    }

    /// Calls `on_value` for each raw match value produced by the chosen engine.
    ///
    /// Returns `true` if the callback requests early exit. Engine selection is
    /// density-based: bytewise for high character density
    /// (≥ [`CHARWISE_DENSITY_THRESHOLD`]), charwise below.
    #[inline(always)]
    pub(super) fn for_each_match_value(
        &self,
        text: &str,
        density: f32,
        on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        dispatch!(self.engines, density, for_each_match_value(text, on_value))
    }

    /// Returns whether the bytewise DFA has a Teddy SIMD prefilter active.
    ///
    /// When true, `find_overlapping_iter` on materialized text uses Teddy to
    /// skip non-matching regions — the fused streaming path cannot replicate
    /// this and should fall back to materialization.
    pub(super) fn has_dfa_prefilter(&self) -> bool {
        self.engines.has_dfa_prefilter()
    }

    /// Calls `on_value` for each raw match value from a streaming byte
    /// iterator.
    ///
    /// Used by the fused transform-scan path. With `dfa` feature and no Teddy
    /// prefilter, uses the DFA's low-level `next_state` loop (avoids
    /// materialization). Otherwise falls back to DAAC bytewise or charwise.
    #[inline(always)]
    pub(super) fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        density: f32,
        on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        dispatch!(
            self.engines,
            density,
            for_each_match_value_from_iter(iter, on_value)
        )
    }
}

// ── Automaton compilation ───────────────────────────────────────────────

/// Compiles the bytewise and charwise automata from the deduplicated pattern
/// list.
///
/// Both engines are built from the FULL pattern set. Bytewise handles any
/// UTF-8 text via byte-level matching; charwise gives 1.6–1.9× throughput
/// on CJK text via character-granularity transitions.
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
#[optimize(speed)]
fn compile_automata(
    dedup_patterns: &[Cow<'_, str>],
    value_map: &[u32],
) -> Result<Engines, MatcherError> {
    let all_patvals: Vec<(&str, u32)> = dedup_patterns
        .iter()
        .enumerate()
        .map(|(i, p)| (p.as_ref(), value_map[i]))
        .collect();

    let all_patvals_clone = all_patvals.clone();
    let build_bytewise = move || -> Result<BytewiseMatcher, MatcherError> {
        build_current_bytewise(all_patvals_clone)
    };

    let build_charwise = |source: Vec<(&str, u32)>| -> Result<CharwiseMatcher, MatcherError> {
        CharwiseDAACBuilder::new()
            .match_kind(DAACMatchKind::Standard)
            .build_with_values(source)
            .map_err(MatcherError::automaton_build)
    };

    std::thread::scope(|s| {
        let bytewise_handle = s.spawn(build_bytewise);
        let charwise = build_charwise(all_patvals)?;
        let bytewise = bytewise_handle
            .join()
            .expect("bytewise automaton build panicked")?;
        Ok(Engines { bytewise, charwise })
    })
}

/// Builds the bytewise engine from the full pattern set.
///
/// Always builds DAAC bytewise (needed for streaming). With the `dfa` feature,
/// also builds a [`BytewiseDFAEngine`] (1.7–3.3× faster for non-streaming
/// scan).
fn build_current_bytewise(all_patvals: Vec<(&str, u32)>) -> Result<BytewiseMatcher, MatcherError> {
    // Build DFA first (reads via iterator), then DAAC last (consumes the vec),
    // so we avoid cloning all_patvals.
    #[cfg(feature = "dfa")]
    let dfa_to_value: Vec<u32> = all_patvals.iter().map(|&(_, v)| v).collect();
    #[cfg(feature = "dfa")]
    let dfa = AcDfaBuilder::new()
        .match_kind(AcMatchKind::Standard)
        .build(all_patvals.iter().map(|(p, _)| p))
        .map_err(MatcherError::automaton_build)?;

    let daac = BytewiseDAACBuilder::new()
        .match_kind(DAACMatchKind::Standard)
        .build_with_values(all_patvals)
        .map_err(MatcherError::automaton_build)?;

    Ok(BytewiseMatcher {
        daac,
        #[cfg(feature = "dfa")]
        dfa_engine: BytewiseDFAEngine {
            has_prefilter: dfa.prefilter().is_some(),
            dfa,
            dfa_to_value,
        },
    })
}
