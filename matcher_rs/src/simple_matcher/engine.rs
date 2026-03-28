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

use super::SearchMode;
use super::rule::{PatternEntry, PatternIndex};

#[cfg(feature = "dfa")]
const AC_DFA_PATTERN_THRESHOLD: usize = 2_000;

#[derive(Clone)]
pub(super) struct ScanPlan {
    ascii_matcher: Option<AsciiMatcher>,
    non_ascii_matcher: Option<NonAsciiMatcher>,
    patterns: PatternIndex,
}

#[derive(Clone)]
enum AsciiMatcher {
    #[cfg(feature = "dfa")]
    AcDfa {
        matcher: AhoCorasick,
        to_value: Vec<u32>,
    },
    DaacBytewise(DoubleArrayAhoCorasick<u32>),
}

#[derive(Clone)]
enum NonAsciiMatcher {
    DaacCharwise(CharwiseDoubleArrayAhoCorasick<u32>),
}

enum AsciiOverlappingIter<'a> {
    #[cfg(feature = "dfa")]
    AcDfa {
        inner: AcFindOverlappingIter<'a, 'a>,
        to_value: &'a [u32],
    },
    DaacBytewise(BytewiseOverlappingIter<'a, U8SliceIterator<&'a str>, u32>),
}

enum NonAsciiOverlappingIter<'a> {
    DaacCharwise(CharwiseOverlappingIter<'a, StrIterator<&'a str>, u32>),
}

impl ScanPlan {
    pub(super) fn compile(
        dedup_patterns: &[Cow<'_, str>],
        dedup_entries: Vec<Vec<PatternEntry>>,
        mode: SearchMode,
    ) -> Self {
        let patterns = PatternIndex::new(dedup_entries);
        let value_map = patterns.build_value_map(mode);
        let (ascii_matcher, non_ascii_matcher) = compile_automata(dedup_patterns, &value_map);

        Self {
            ascii_matcher,
            non_ascii_matcher,
            patterns,
        }
    }

    #[inline(always)]
    pub(super) fn patterns(&self) -> &PatternIndex {
        &self.patterns
    }

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

impl Iterator for AsciiOverlappingIter<'_> {
    type Item = u32;

    #[inline(always)]
    fn next(&mut self) -> Option<u32> {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { inner, to_value } => inner
                .next()
                .map(|hit| unsafe { *to_value.get_unchecked(hit.pattern().as_usize()) }),
            Self::DaacBytewise(iter) => iter.next().map(|hit| hit.value()),
        }
    }
}

impl Iterator for NonAsciiOverlappingIter<'_> {
    type Item = u32;

    #[inline(always)]
    fn next(&mut self) -> Option<u32> {
        match self {
            Self::DaacCharwise(iter) => iter.next().map(|hit| hit.value()),
        }
    }
}

impl AsciiMatcher {
    #[inline(always)]
    fn is_match(&self, text: &str) -> bool {
        match self {
            #[cfg(feature = "dfa")]
            Self::AcDfa { matcher, .. } => matcher.is_match(text),
            Self::DaacBytewise(matcher) => matcher.find_iter(text).next().is_some(),
        }
    }

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

impl NonAsciiMatcher {
    #[inline(always)]
    fn is_match(&self, text: &str) -> bool {
        match self {
            Self::DaacCharwise(matcher) => matcher.find_iter(text).next().is_some(),
        }
    }

    #[inline(always)]
    fn find_overlapping_iter<'a>(&'a self, text: &'a str) -> NonAsciiOverlappingIter<'a> {
        match self {
            Self::DaacCharwise(matcher) => {
                NonAsciiOverlappingIter::DaacCharwise(matcher.find_overlapping_iter(text))
            }
        }
    }
}

fn compile_automata(
    dedup_patterns: &[Cow<'_, str>],
    value_map: &[u32],
) -> (Option<AsciiMatcher>, Option<NonAsciiMatcher>) {
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
                    .unwrap(),
                to_value: ascii_ac_to_value,
            }
        } else {
            AsciiMatcher::DaacBytewise(
                DoubleArrayAhoCorasickBuilder::new()
                    .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                    .build_with_values(ascii_patvals)
                    .unwrap(),
            )
        };

        #[cfg(not(feature = "dfa"))]
        let engine = AsciiMatcher::DaacBytewise(
            DoubleArrayAhoCorasickBuilder::new()
                .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                .build_with_values(ascii_patvals)
                .unwrap(),
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
                .unwrap(),
        ))
    } else {
        None
    };

    (ascii_matcher, non_ascii_matcher)
}
