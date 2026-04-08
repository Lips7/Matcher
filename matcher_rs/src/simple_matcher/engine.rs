//! Scan-engine compilation and match iteration for [`super::SimpleMatcher`].
//!
//! This module owns the Aho-Corasick automata that power Pass 1 (pattern scan)
//! of the two-pass matching pipeline. Two independent engines are compiled:
//!
//! - **Bytewise engine** ([`BytewiseMatcher`]) — scans byte-by-byte over the
//!   full pattern set. With the `dfa` feature enabled, this uses the
//!   `aho-corasick` crate's DFA for maximum throughput. Otherwise it falls back
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
//! density scan ([`text_non_ascii_density`]) to select the engine. When the
//! non-ASCII byte fraction is ≤ [`CHARWISE_DENSITY_THRESHOLD`] (0.67, ~40%
//! CJK characters) the bytewise engine is used; above the threshold the
//! charwise engine is selected.

use std::borrow::Cow;

#[cfg(feature = "dfa")]
use aho_corasick::{
    AhoCorasick as AcEngine, AhoCorasickBuilder as AcBuilder, AhoCorasickKind as AcKind,
    MatchKind as AcMatchKind,
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
    /// iterator. Always uses DAAC (no DFA streaming API).
    fn for_each_match_value_from_iter(
        &self,
        iter: impl Iterator<Item = u8>,
        on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool;

    /// Returns the estimated heap memory in bytes owned by this engine.
    fn heap_bytes(&self) -> usize;
}

// ── Bytewise engine ─────────────────────────────────────────────────────

/// Bytewise scan engine. DAAC bytewise is always built (supports streaming).
/// DFA is built alongside it when the `dfa` feature is enabled (1.7–3.3× faster
/// for non-streaming scan).
#[derive(Clone)]
struct BytewiseMatcher {
    /// DAAC bytewise automaton. Always built — needed for streaming iteration.
    daac: BytewiseDAACEngine<u32>,
    /// Aho-Corasick DFA. 1.7–3.3× faster than DAAC for non-streaming scan.
    #[cfg(feature = "dfa")]
    dfa: AcEngine,
    /// Maps DFA pattern index → raw match value (bridges `aho-corasick` pattern
    /// ids to our encoding).
    #[cfg(feature = "dfa")]
    dfa_to_value: Vec<u32>,
}

impl ScanEngine for BytewiseMatcher {
    #[inline(always)]
    fn is_match(&self, text: &str) -> bool {
        #[cfg(feature = "dfa")]
        {
            self.dfa.is_match(text)
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
        mut on_value: impl FnMut(u32, usize, usize) -> bool,
    ) -> bool {
        #[cfg(feature = "dfa")]
        {
            for m in self.dfa.find_overlapping_iter(text) {
                // SAFETY: `dfa_to_value` has one entry per pattern; pattern index is always
                // in bounds.
                let value = unsafe { *self.dfa_to_value.get_unchecked(m.pattern().as_usize()) };
                if on_value(value, m.start(), m.end()) {
                    return true;
                }
            }
            false
        }
        #[cfg(not(feature = "dfa"))]
        {
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
            daac + self.dfa.memory_usage() + self.dfa_to_value.capacity() * size_of::<u32>()
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

/// Dispatches to the bytewise or charwise engine based on density.
///
/// Expands to: `if density <= threshold { bytewise.$method } else {
/// charwise.$method }`. Avoids `dyn ScanEngine` (methods have `impl Trait`
/// params → not object-safe).
macro_rules! dispatch {
    ($engines:expr, $density:expr, $method:ident ($($arg:expr),*)) => {
        if $density <= CHARWISE_DENSITY_THRESHOLD {
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
    /// `true` when every compiled pattern is pure ASCII.
    ///
    /// Used for a fast-return: when all patterns are ASCII and the text
    /// contains zero ASCII bytes, no match is possible.
    all_patterns_ascii: bool,
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
    ) -> Result<Self, MatcherError> {
        debug_assert!(
            !dedup_patterns.is_empty(),
            "ScanPlan::compile called with zero patterns"
        );

        let patterns = PatternIndex::new(dedup_entries);
        let value_map = patterns.build_value_map();
        let engines = compile_automata(dedup_patterns, &value_map)?;
        let all_patterns_ascii = dedup_patterns.iter().all(|p| p.is_ascii());

        Ok(Self {
            engines,
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
        cfg!(feature = "dfa")
    }

    /// Returns the estimated heap memory in bytes owned by all scan engines.
    pub(super) fn heap_bytes(&self) -> usize {
        self.engines.bytewise.heap_bytes()
            + self.engines.charwise.heap_bytes()
            + self.patterns.heap_bytes()
    }

    /// Returns whether any compiled pattern matches `text`.
    ///
    /// Density-based engine dispatch: bytewise for low non-ASCII density
    /// (≤ [`CHARWISE_DENSITY_THRESHOLD`]), charwise for high density.
    /// Skips TLS state entirely — used as a fast path for
    /// `SimpleMatcher::is_match` when no text transforms are needed.
    #[inline(always)]
    pub(super) fn is_match(&self, text: &str) -> bool {
        let density = text_non_ascii_density(text);
        if self.all_patterns_ascii && density >= 1.0 && text.bytes().all(|b| b >= 0x80) {
            return false;
        }
        dispatch!(self.engines, density, is_match(text))
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
        dispatch!(self.engines, density, for_each_match_value(text, on_value))
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
/// also builds an `aho-corasick` DFA (1.7–3.3× faster for non-streaming scan).
fn build_current_bytewise(all_patvals: Vec<(&str, u32)>) -> Result<BytewiseMatcher, MatcherError> {
    let daac = BytewiseDAACBuilder::new()
        .match_kind(DAACMatchKind::Standard)
        .build_with_values(all_patvals.clone())
        .map_err(MatcherError::automaton_build)?;

    #[cfg(feature = "dfa")]
    let dfa_to_value: Vec<u32> = all_patvals.iter().map(|&(_, v)| v).collect();
    #[cfg(feature = "dfa")]
    let dfa = AcBuilder::new()
        .kind(Some(AcKind::DFA))
        .match_kind(AcMatchKind::Standard)
        .build(all_patvals.iter().map(|(p, _)| p))
        .map_err(MatcherError::automaton_build)?;

    Ok(BytewiseMatcher {
        daac,
        #[cfg(feature = "dfa")]
        dfa,
        #[cfg(feature = "dfa")]
        dfa_to_value,
    })
}
