//! Normalize engine backed by an Aho-Corasick DFA automaton.
//!
//! [`NormalizeMatcher`] performs multi-character replacement (full-width to
//! half-width, variant forms, number normalization, etc.) using leftmost-longest
//! Aho-Corasick matching. The automaton scans the text once, finds all
//! non-overlapping matches, and rebuilds the output by interleaving unchanged
//! spans with replacements from a parallel lookup table.
//!
//! Always uses [`aho_corasick::AhoCorasick`] with DFA mode and leftmost-longest
//! match semantics. The normalization dictionary is small (~hundreds of patterns),
//! so DFA memory overhead is negligible.

#[cfg(feature = "runtime_build")]
use ahash::AHashMap;

use aho_corasick::{
    AhoCorasick, AhoCorasickBuilder, AhoCorasickKind, MatchKind as AhoCorasickMatchKind,
};

/// Multi-character normalization matcher plus its parallel replacement table.
///
/// The matcher holds a compiled [`AhoCorasick`] DFA and a `replace_list` where
/// index `i` is the replacement string for the `i`-th pattern in the automaton.
/// Pattern order is established at construction time and must be consistent
/// between the automaton and the replacement list.
#[derive(Clone)]
pub(crate) struct NormalizeMatcher {
    engine: AhoCorasick,
    /// Replacement strings parallel to the automaton's pattern indices.
    /// `replace_list[match.pattern_index]` is the output for a given match.
    replace_list: Vec<&'static str>,
    /// Pre-computed: true when every entry in `replace_list` is pure ASCII.
    /// Allows skipping per-replacement ASCII checks during the hot-path replace loop.
    pub(crate) all_replacements_ascii: bool,
}

/// Byte-by-byte iterator over normalize-transformed text.
///
/// Yields the UTF-8 bytes of `text` with all normalization matches replaced
/// by their target strings. Wraps [`aho_corasick::FindIter`] internally:
/// original bytes are yielded between match spans, and replacement string
/// bytes are yielded for each match.
pub(crate) struct NormalizeByteIter<'a> {
    find_iter: aho_corasick::FindIter<'a, 'a>,
    replace_list: &'a [&'static str],
    source: &'a [u8],
    pos: usize,
    /// Pre-fetched next match start (usize::MAX when exhausted).
    next_start: usize,
    next_end: usize,
    /// Pre-fetched replacement bytes for the next match.
    next_repl: &'a [u8],
    /// Current replacement bytes being yielded.
    repl: &'a [u8],
    repl_pos: usize,
}

impl<'a> Iterator for NormalizeByteIter<'a> {
    type Item = u8;

    #[inline(always)]
    fn next(&mut self) -> Option<u8> {
        // Drain current replacement
        if self.repl_pos < self.repl.len() {
            let b = self.repl[self.repl_pos];
            self.repl_pos += 1;
            return Some(b);
        }

        // At match start?
        if self.pos == self.next_start {
            self.pos = self.next_end;
            self.repl = self.next_repl;
            self.repl_pos = 1;
            let first = self.repl[0];
            self.advance_find_iter();
            return Some(first);
        }

        // Yield original byte
        if self.pos < self.source.len() {
            let b = self.source[self.pos];
            self.pos += 1;
            Some(b)
        } else {
            None
        }
    }
}

impl<'a> NormalizeByteIter<'a> {
    #[inline(always)]
    fn advance_find_iter(&mut self) {
        if let Some(m) = self.find_iter.next() {
            self.next_start = m.start();
            self.next_end = m.end();
            self.next_repl = self.replace_list[m.pattern().as_usize()].as_bytes();
        } else {
            self.next_start = usize::MAX;
        }
    }
}

impl NormalizeMatcher {
    /// Creates a find iterator over all leftmost-longest matches in `text`.
    #[inline(always)]
    fn find_iter<'a>(&'a self, text: &'a str) -> aho_corasick::FindIter<'a, 'a> {
        self.engine.find_iter(text)
    }

    /// Returns a byte-by-byte iterator over normalize-transformed text.
    ///
    /// Equivalent output to `replace()` followed by iterating the result's
    /// bytes, but without allocating an intermediate `String`.
    #[inline(always)]
    pub(crate) fn byte_iter<'a>(&'a self, text: &'a str) -> NormalizeByteIter<'a> {
        let mut iter = NormalizeByteIter {
            find_iter: self.find_iter(text),
            replace_list: &self.replace_list,
            source: text.as_bytes(),
            pos: 0,
            next_start: usize::MAX,
            next_end: 0,
            next_repl: &[],
            repl: &[],
            repl_pos: 0,
        };
        iter.advance_find_iter();
        iter
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
        if let Some(m) = iter.next() {
            let mut result = crate::process::variant::get_string_from_pool(text.len());
            let mut is_ascii = self.all_replacements_ascii;
            let prefix = &text[..m.start()];
            is_ascii = is_ascii && prefix.is_ascii();
            result.push_str(prefix);
            result.push_str(self.replace_list[m.pattern().as_usize()]);
            let mut last_end = m.end();
            for m in iter {
                let gap = &text[last_end..m.start()];
                is_ascii = is_ascii && gap.is_ascii();
                result.push_str(gap);
                result.push_str(self.replace_list[m.pattern().as_usize()]);
                last_end = m.end();
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
    /// Compiles the patterns into an aho_corasick DFA using leftmost-longest
    /// match semantics.
    ///
    /// # Panics
    ///
    /// Panics (via `.unwrap()`) if the Aho-Corasick builder fails.
    pub(crate) fn new<I, P>(patterns: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<str> + AsRef<[u8]>,
    {
        Self {
            engine: AhoCorasickBuilder::new()
                .kind(Some(AhoCorasickKind::DFA))
                .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                .build(patterns)
                .unwrap(),
            replace_list: Vec::new(),
            all_replacements_ascii: true,
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

    /// Builds a matcher from a runtime-parsed normalization dictionary.
    ///
    /// Sorts the dictionary entries by key for deterministic pattern ordering,
    /// builds the Aho-Corasick automaton from the sorted keys, and attaches
    /// the corresponding replacement values via [`NormalizeMatcher::with_replacements`].
    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_dict(dict: AHashMap<&'static str, &'static str>) -> Self {
        let mut pairs: Vec<(&'static str, &'static str)> = dict.into_iter().collect();
        pairs.sort_unstable_by_key(|&(k, _)| k);
        let replace_list: Vec<&'static str> = pairs.iter().map(|&(_, v)| v).collect();
        Self::new(pairs.into_iter().map(|(k, _)| k)).with_replacements(replace_list)
    }
}

#[cfg(all(test, not(feature = "runtime_build")))]
mod tests {
    use super::*;

    use super::super::constants;

    fn normalize_matcher() -> NormalizeMatcher {
        let patterns: Vec<&str> = constants::NORMALIZE_PROCESS_LIST_STR.lines().collect();
        let replace_list: Vec<&'static str> = constants::NORMALIZE_PROCESS_REPLACE_LIST_STR
            .lines()
            .collect();
        NormalizeMatcher::new(patterns.iter()).with_replacements(replace_list)
    }

    fn assert_byte_iter_eq_replace(matcher: &NormalizeMatcher, text: &str) {
        let materialized: Vec<u8> = match matcher.replace(text) {
            Some((s, _)) => s.into_bytes(),
            None => text.as_bytes().to_vec(),
        };
        let streamed: Vec<u8> = matcher.byte_iter(text).collect();
        assert_eq!(materialized, streamed, "normalize mismatch for: {:?}", text);
    }

    #[test]
    fn normalize_byte_iter_matches_replace() {
        let m = normalize_matcher();
        for text in ["", "hello", "ＡＢＣ", "abc１２３def", "①②③"] {
            assert_byte_iter_eq_replace(&m, text);
        }
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(500))]

        #[test]
        fn prop_normalize_byte_iter(text in "\\PC{0,200}") {
            let m = normalize_matcher();
            assert_byte_iter_eq_replace(&m, &text);
        }
    }
}
