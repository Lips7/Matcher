//! Normalize engine backed by a compiled Aho-Corasick automaton.
//!
//! [`NormalizeMatcher`] performs multi-character replacement (full-width to
//! half-width, variant forms, number normalization, etc.) using leftmost-longest
//! Aho-Corasick matching. The automaton scans the text once, finds all
//! non-overlapping matches, and rebuilds the output by interleaving unchanged
//! spans with replacements from a parallel lookup table.
//!
//! Two backends are supported, selected by feature flags:
//!
//! - **`dfa`** (default): [`aho_corasick::AhoCorasick`] with DFA mode --
//!   faster matching at the cost of higher memory usage (~10x vs NFA).
//! - **`not(dfa)`**: [`daachorse::CharwiseDoubleArrayAhoCorasick`] -- compact
//!   double-array trie that can be pre-serialized at build time and
//!   deserialized without recompilation.
//!
//! Construction is also feature-gated:
//! - **Default (not `runtime_build`)**: Pre-compiled automaton bytes
//!   (`constants::NORMALIZE_PROCESS_MATCHER_BYTES`) are deserialized, or
//!   patterns (`constants::NORMALIZE_PROCESS_LIST_STR`) are compiled into a
//!   DFA, depending on the `dfa` flag.
//! - **`runtime_build`**: The automaton is built from a `HashMap` dictionary
//!   parsed at startup.

#[cfg(feature = "runtime_build")]
use std::collections::HashMap;

#[cfg(feature = "dfa")]
use aho_corasick::{
    AhoCorasick, AhoCorasickBuilder, AhoCorasickKind, FindIter as AhoCorasickFindIter,
    MatchKind as AhoCorasickMatchKind,
};
#[cfg(not(feature = "dfa"))]
use daachorse::{
    CharwiseDoubleArrayAhoCorasick,
    charwise::iter::LestmostFindIterator as DoubleArrayAhoCorasickFindIter,
};
#[cfg(all(not(feature = "dfa"), feature = "runtime_build"))]
use daachorse::{
    CharwiseDoubleArrayAhoCorasickBuilder, MatchKind as DoubleArrayAhoCorasickMatchKind,
};

/// Concrete automaton backend used by [`NormalizeMatcher`].
///
/// Exactly one variant is compiled, determined by the `dfa` feature flag.
/// This enum exists to provide a uniform interface over the two backends
/// without runtime dispatch overhead (only one variant is ever present).
#[derive(Clone)]
enum NormalizeEngine {
    /// Compact double-array Aho-Corasick trie (non-DFA path).
    #[cfg(not(feature = "dfa"))]
    DoubleArrayAhoCorasick(CharwiseDoubleArrayAhoCorasick<u32>),
    /// Full DFA-backed Aho-Corasick automaton.
    #[cfg(feature = "dfa")]
    AhoCorasick(AhoCorasick),
}

/// Multi-character normalization matcher plus its parallel replacement table.
///
/// The matcher holds a compiled [`NormalizeEngine`] and a `replace_list` where
/// index `i` is the replacement string for the `i`-th pattern in the automaton.
/// Pattern order is established at construction time and must be consistent
/// between the automaton and the replacement list.
#[derive(Clone)]
pub(crate) struct NormalizeMatcher {
    /// Compiled Aho-Corasick automaton (DFA or double-array, depending on features).
    engine: NormalizeEngine,
    /// Replacement strings parallel to the automaton's pattern indices.
    /// `replace_list[match.pattern_index]` is the output for a given match.
    replace_list: Vec<&'static str>,
    /// Pre-computed: true when every entry in `replace_list` is pure ASCII.
    /// Allows skipping per-replacement ASCII checks during the hot-path replace loop.
    all_replacements_ascii: bool,
}

/// Iterator adapter over normalization matches, yielding `(start, end, pattern_index)` tuples.
///
/// Wraps the backend-specific find iterator and normalizes the output format
/// so that [`NormalizeMatcher::replace`] can consume matches uniformly
/// regardless of the active backend.
enum NormalizeFindIter<'a> {
    /// Double-array backend iterator (non-DFA path).
    #[cfg(not(feature = "dfa"))]
    DoubleArrayAhoCorasick(DoubleArrayAhoCorasickFindIter<'a, &'a str, u32>),
    /// DFA backend iterator.
    #[cfg(feature = "dfa")]
    AhoCorasick(AhoCorasickFindIter<'a, 'a>),
}

impl<'a> Iterator for NormalizeFindIter<'a> {
    type Item = (usize, usize, usize);

    /// Advances to the next normalization match.
    ///
    /// Returns `(start_byte, end_byte, pattern_index)` where `pattern_index`
    /// is the zero-based index into the replacement list. The two backends
    /// expose this value differently (`m.value()` vs `m.pattern().as_usize()`),
    /// which this adapter normalizes.
    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            #[cfg(not(feature = "dfa"))]
            NormalizeFindIter::DoubleArrayAhoCorasick(iter) => iter
                .next()
                .map(|m| (m.start(), m.end(), m.value() as usize)),
            #[cfg(feature = "dfa")]
            NormalizeFindIter::AhoCorasick(iter) => iter
                .next()
                .map(|m| (m.start(), m.end(), m.pattern().as_usize())),
        }
    }
}

impl NormalizeMatcher {
    /// Creates a [`NormalizeFindIter`] over all leftmost-longest matches in `text`.
    #[inline(always)]
    fn find_iter<'a>(&'a self, text: &'a str) -> NormalizeFindIter<'a> {
        match &self.engine {
            #[cfg(not(feature = "dfa"))]
            NormalizeEngine::DoubleArrayAhoCorasick(ac) => {
                NormalizeFindIter::DoubleArrayAhoCorasick(ac.leftmost_find_iter(text))
            }
            #[cfg(feature = "dfa")]
            NormalizeEngine::AhoCorasick(ac) => NormalizeFindIter::AhoCorasick(ac.find_iter(text)),
        }
    }

    /// Replaces every normalization match in `text`.
    ///
    /// Scans `text` with the Aho-Corasick automaton in leftmost-longest mode.
    /// For each match, copies the unchanged text since the last match, then
    /// appends the replacement string from `replace_list[pattern_index]`.
    ///
    /// Returns `None` when no pattern matched, so callers can preserve
    /// borrowed input without allocation. The `bool` indicates whether the
    /// output is pure ASCII, tracked incrementally to avoid a redundant scan.
    pub(crate) fn replace(&self, text: &str) -> Option<(String, bool)> {
        let mut iter = self.find_iter(text);
        if let Some((start, end, index)) = iter.next() {
            let mut result = crate::process::variant::get_string_from_pool(text.len());
            let mut is_ascii = self.all_replacements_ascii;
            let prefix = &text[..start];
            is_ascii = is_ascii && prefix.is_ascii();
            result.push_str(prefix);
            result.push_str(self.replace_list[index]);
            let mut last_end = end;
            for (start, end, index) in iter {
                let gap = &text[last_end..start];
                is_ascii = is_ascii && gap.is_ascii();
                result.push_str(gap);
                result.push_str(self.replace_list[index]);
                last_end = end;
            }
            let suffix = &text[last_end..];
            is_ascii = is_ascii && suffix.is_ascii();
            result.push_str(suffix);
            Some((result, is_ascii))
        } else {
            None
        }
    }

    /// Builds a matcher from an ordered pattern list.
    ///
    /// Compiles the patterns into the active Aho-Corasick backend using
    /// leftmost-longest match semantics. The replacement list is initially
    /// empty and must be attached via [`NormalizeMatcher::with_replacements`].
    ///
    /// # Panics
    ///
    /// Panics (via `.unwrap()`) if the Aho-Corasick builder fails, which can
    /// happen if patterns are empty or the backend encounters an internal limit.
    #[cfg(any(feature = "runtime_build", feature = "dfa"))]
    pub(crate) fn new<I, P>(patterns: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<str> + AsRef<[u8]>,
    {
        #[cfg(not(feature = "dfa"))]
        {
            Self {
                engine: NormalizeEngine::DoubleArrayAhoCorasick(
                    CharwiseDoubleArrayAhoCorasickBuilder::new()
                        .match_kind(DoubleArrayAhoCorasickMatchKind::LeftmostLongest)
                        .build(patterns)
                        .unwrap(),
                ),
                replace_list: Vec::new(),
                all_replacements_ascii: true,
            }
        }
        #[cfg(feature = "dfa")]
        {
            Self {
                engine: NormalizeEngine::AhoCorasick(
                    AhoCorasickBuilder::new()
                        .kind(Some(AhoCorasickKind::DFA))
                        .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                        .build(patterns)
                        .unwrap(),
                ),
                replace_list: Vec::new(),
                all_replacements_ascii: true,
            }
        }
    }

    /// Attaches the replacement list parallel to the compiled pattern order.
    ///
    /// `replace_list[i]` must be the replacement for pattern `i` in the
    /// automaton. Consumes and returns `self` for builder-style chaining.
    pub(crate) fn with_replacements(mut self, replace_list: Vec<&'static str>) -> Self {
        self.all_replacements_ascii = replace_list.iter().all(|s| s.is_ascii());
        self.replace_list = replace_list;
        self
    }

    /// Reconstructs the precompiled non-DFA matcher from build-time bytes.
    ///
    /// Deserializes a [`CharwiseDoubleArrayAhoCorasick<u32>`] from the raw
    /// bytes in `constants::NORMALIZE_PROCESS_MATCHER_BYTES`, which were
    /// serialized by `build.rs`.
    ///
    /// # Safety
    ///
    /// Uses `deserialize_unchecked` which trusts that the byte layout matches
    /// the expected format. This is safe because the bytes are produced by the
    /// same version of `daachorse` at build time and embedded as a static
    /// constant -- they cannot be corrupted at runtime.
    #[cfg(all(not(feature = "dfa"), not(feature = "runtime_build")))]
    pub(crate) fn deserialize(bytes: &'static [u8]) -> Self {
        Self {
            engine: NormalizeEngine::DoubleArrayAhoCorasick(unsafe {
                CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(bytes).0
            }),
            replace_list: Vec::new(),
            all_replacements_ascii: true,
        }
    }

    /// Builds a matcher from a runtime-parsed normalization dictionary.
    ///
    /// Sorts the dictionary entries by key for deterministic pattern ordering,
    /// builds the Aho-Corasick automaton from the sorted keys, and attaches
    /// the corresponding replacement values via [`NormalizeMatcher::with_replacements`].
    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_dict(dict: HashMap<&'static str, &'static str>) -> Self {
        let mut pairs: Vec<(&'static str, &'static str)> = dict.into_iter().collect();
        pairs.sort_unstable_by_key(|&(k, _)| k);
        let replace_list: Vec<&'static str> = pairs.iter().map(|&(_, v)| v).collect();
        Self::new(pairs.into_iter().map(|(k, _)| k)).with_replacements(replace_list)
    }
}
