//! Normalize engine backed by a compiled Aho-Corasick automaton.

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

#[derive(Clone)]
enum NormalizeEngine {
    #[cfg(not(feature = "dfa"))]
    DoubleArrayAhoCorasick(CharwiseDoubleArrayAhoCorasick<u32>),
    #[cfg(feature = "dfa")]
    AhoCorasick(AhoCorasick),
}

#[derive(Clone)]
pub(crate) struct NormalizeMatcher {
    engine: NormalizeEngine,
    replace_list: Vec<&'static str>,
}

enum NormalizeFindIter<'a> {
    #[cfg(not(feature = "dfa"))]
    DoubleArrayAhoCorasick(DoubleArrayAhoCorasickFindIter<'a, &'a str, u32>),
    #[cfg(feature = "dfa")]
    AhoCorasick(AhoCorasickFindIter<'a, 'a>),
}

impl<'a> Iterator for NormalizeFindIter<'a> {
    type Item = (usize, usize, usize);

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

    pub(crate) fn replace(&self, text: &str) -> Option<String> {
        let mut iter = self.find_iter(text);
        if let Some((start, end, index)) = iter.next() {
            let mut result = crate::process::variant::get_string_from_pool(text.len());
            result.push_str(&text[..start]);
            result.push_str(self.replace_list[index]);
            let mut last_end = end;
            for (start, end, index) in iter {
                result.push_str(&text[last_end..start]);
                result.push_str(self.replace_list[index]);
                last_end = end;
            }
            result.push_str(&text[last_end..]);
            Some(result)
        } else {
            None
        }
    }

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
            }
        }
    }

    pub(crate) fn with_replacements(mut self, replace_list: Vec<&'static str>) -> Self {
        self.replace_list = replace_list;
        self
    }

    #[cfg(all(not(feature = "dfa"), not(feature = "runtime_build")))]
    pub(crate) fn deserialize(bytes: &'static [u8]) -> Self {
        Self {
            engine: NormalizeEngine::DoubleArrayAhoCorasick(unsafe {
                CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(bytes).0
            }),
            replace_list: Vec::new(),
        }
    }

    #[cfg(feature = "runtime_build")]
    pub(crate) fn from_dict(dict: HashMap<&'static str, &'static str>) -> Self {
        let mut pairs: Vec<(&'static str, &'static str)> = dict.into_iter().collect();
        pairs.sort_unstable_by_key(|&(k, _)| k);
        let replace_list: Vec<&'static str> = pairs.iter().map(|&(_, v)| v).collect();
        Self::new(pairs.into_iter().map(|(k, _)| k)).with_replacements(replace_list)
    }
}
