use aho_corasick::{AhoCorasick, FindIter as AhoCorasickFindIter};
#[cfg(feature = "dfa")]
use aho_corasick::{AhoCorasickBuilder, AhoCorasickKind, MatchKind as AhoCorasickMatchKind};
#[cfg(not(feature = "dfa"))]
use daachorse::{
    CharwiseDoubleArrayAhoCorasick,
    charwise::iter::LestmostFindIterator as DoubleArrayAhoCorasickFindIter,
};
#[cfg(all(not(feature = "dfa"), feature = "runtime_build"))]
use daachorse::{
    CharwiseDoubleArrayAhoCorasickBuilder, MatchKind as DoubleArrayAhoCorasickMatchKind,
};
#[cfg(feature = "runtime_build")]
use std::collections::HashMap;

/// Underlying automaton variants for [`MultiCharMatcher`].
#[derive(Clone)]
enum MultiCharEngine {
    /// Charwise double-array Aho-Corasick (non-`dfa` builds only).
    #[cfg(not(feature = "dfa"))]
    DoubleArrayAhoCorasick(CharwiseDoubleArrayAhoCorasick<u32>),
    /// Standard Aho-Corasick automaton.
    AhoCorasick(AhoCorasick),
}

/// Multi-character pattern matching engine backed by a compiled automaton.
///
/// Non-`dfa` builds use a [`daachorse`] charwise double-array Aho-Corasick for
/// leftmost-longest matching. With the `dfa` feature a standard
/// [`aho_corasick::AhoCorasick`] DFA is used instead. The `AC` engine also
/// serves as a zero-pattern no-op sentinel for `ProcessType::None`.
///
/// `replace_list` maps each pattern index to its replacement string and is
/// populated only for `ProcessType::Normalize`; it is empty for all other types.
#[derive(Clone)]
pub(crate) struct MultiCharMatcher {
    engine: MultiCharEngine,
    replace_list: Vec<&'static str>,
}

/// An iterator over multi-character pattern matches in a text string.
///
/// Yields `(start_byte, end_byte, pattern_idx)` triples for each match.
pub(crate) enum MultiCharFindIter<'a> {
    /// DAAC leftmost-longest iterator.
    #[cfg(not(feature = "dfa"))]
    DoubleArrayAhoCorasick(DoubleArrayAhoCorasickFindIter<'a, &'a str, u32>),
    /// Standard Aho-Corasick iterator.
    AhoCorasick(AhoCorasickFindIter<'a, 'a>),
}

impl<'a> Iterator for MultiCharFindIter<'a> {
    type Item = (usize, usize, usize);

    #[inline(always)]
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            #[cfg(not(feature = "dfa"))]
            MultiCharFindIter::DoubleArrayAhoCorasick(iter) => iter
                .next()
                .map(|m| (m.start(), m.end(), m.value() as usize)),
            MultiCharFindIter::AhoCorasick(iter) => iter
                .next()
                .map(|m| (m.start(), m.end(), m.pattern().as_usize())),
        }
    }
}

impl MultiCharMatcher {
    /// Returns the replacement strings parallel to automaton pattern indices.
    ///
    /// Only normalization matchers populate this; empty/no-op matchers leave it empty.
    #[inline(always)]
    pub(crate) fn replace_list(&self) -> &[&'static str] {
        &self.replace_list
    }

    /// Returns an iterator over all pattern matches in `text`.
    ///
    /// Each item is `(start_byte, end_byte, pattern_idx)`.
    #[inline(always)]
    pub(crate) fn find_iter<'a>(&'a self, text: &'a str) -> MultiCharFindIter<'a> {
        match &self.engine {
            #[cfg(not(feature = "dfa"))]
            MultiCharEngine::DoubleArrayAhoCorasick(ac) => {
                MultiCharFindIter::DoubleArrayAhoCorasick(ac.leftmost_find_iter(text))
            }
            MultiCharEngine::AhoCorasick(ac) => MultiCharFindIter::AhoCorasick(ac.find_iter(text)),
        }
    }

    /// Creates an empty no-op matcher (used for `ProcessType::None`).
    pub(crate) fn new_empty() -> Self {
        Self {
            engine: MultiCharEngine::AhoCorasick(AhoCorasick::new(Vec::<&str>::new()).unwrap()),
            replace_list: Vec::new(),
        }
    }

    /// Builds a leftmost-longest matcher from `patterns` with an empty replace list.
    ///
    /// Non-`dfa` builds produce a DAAC automaton; `dfa` builds produce a DFA.
    /// Available when `runtime_build` or `dfa` is active.
    /// Use [`with_replace_list`](Self::with_replace_list) to populate the replace list.
    #[cfg(any(feature = "runtime_build", feature = "dfa"))]
    pub(crate) fn new<I, P>(patterns: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: AsRef<str> + AsRef<[u8]>,
    {
        #[cfg(not(feature = "dfa"))]
        {
            Self {
                engine: MultiCharEngine::DoubleArrayAhoCorasick(
                    CharwiseDoubleArrayAhoCorasickBuilder::new()
                        .match_kind(DoubleArrayAhoCorasickMatchKind::LeftmostLongest)
                        .build(patterns)
                        .unwrap(),
                ),
                replace_list: Vec::new(),
            }
        }
        #[cfg(feature = "dfa")]
        {
            Self {
                engine: MultiCharEngine::AhoCorasick(
                    AhoCorasickBuilder::new()
                        .kind(Some(AhoCorasickKind::DFA))
                        .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                        .build(patterns)
                        .unwrap(),
                ),
                replace_list: Vec::new(),
            }
        }
    }

    /// Attaches a replacement list, consuming and returning `self`.
    ///
    /// `replace_list[i]` must correspond to pattern `i` in the compiled automaton.
    pub(crate) fn with_replace_list(mut self, replace_list: Vec<&'static str>) -> Self {
        self.replace_list = replace_list;
        self
    }

    /// Deserializes a precompiled DAAC automaton from static bytes.
    ///
    /// Only available without the `dfa` feature (non-`runtime_build` path).
    #[cfg(not(feature = "dfa"))]
    pub(crate) fn deserialize_from(bytes: &'static [u8]) -> Self {
        Self {
            // SAFETY: `bytes` is produced by build.rs `serialize()` in the same build,
            // so the format, alignment, and endianness match the current daachorse version.
            engine: MultiCharEngine::DoubleArrayAhoCorasick(unsafe {
                CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(bytes).0
            }),
            replace_list: Vec::new(),
        }
    }

    /// Builds a matcher from a pattern→replacement dict.
    ///
    /// Pairs are sorted by key; the automaton is built from the sorted keys and
    /// `replace_list` is populated from the corresponding sorted values so pattern
    /// indices stay aligned with replacements.
    #[cfg(feature = "runtime_build")]
    pub(crate) fn new_from_dict(dict: HashMap<&'static str, &'static str>) -> Self {
        let mut pairs: Vec<(&'static str, &'static str)> = dict.into_iter().collect();
        pairs.sort_unstable_by_key(|&(k, _)| k);
        let replace_list: Vec<&'static str> = pairs.iter().map(|&(_, v)| v).collect();
        Self::new(pairs.into_iter().map(|(k, _)| k)).with_replace_list(replace_list)
    }
}
