//! Cached single-step transformation engines and public text-processing helpers.
//!
//! Public API: [`text_process`], [`reduce_text_process`], [`reduce_text_process_emit`].
//! Internal API: [`get_process_matcher`] (returns cached `&'static ProcessMatcher`).

use std::borrow::Cow;
#[cfg(feature = "runtime_build")]
use std::collections::HashMap;
use std::sync::OnceLock;

use crate::process::process_type::ProcessType;
use crate::process::string_pool::{get_string_from_pool, return_string_to_pool};
use crate::process::transform::constants::*;
use crate::process::transform::multi_char_matcher::MultiCharMatcher;
use crate::process::transform::single_char_matcher::{SingleCharMatch, SingleCharMatcher};

/// Maps the bit position of a single-bit [`ProcessType`] to its compiled [`ProcessMatcher`].
static PROCESS_MATCHER_CACHE: [OnceLock<ProcessMatcher>; 8] = [
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
];

/// Underlying engine used by a single-step text transformation.
///
/// Consumers should not construct this directly; use [`get_process_matcher`] to
/// obtain a cached instance for a given [`ProcessType`] bit.
///
/// # Variants
///
/// * `MultiChar` — Multi-character replacement via [`MultiCharMatcher`]; used for
///   Normalize and as the no-op engine for `ProcessType::None`.
/// * `SingleChar` — Per-codepoint lookup via a [`SingleCharMatcher`]; used for
///   Fanjian, Pinyin, and Delete.
#[derive(Clone)]
pub(crate) enum ProcessMatcher {
    MultiChar(MultiCharMatcher),
    SingleChar(SingleCharMatcher),
}

impl ProcessMatcher {
    /// Generic scan-and-replace engine underlying both [`replace_all`](Self::replace_all) and
    /// [`delete_all`](Self::delete_all).
    ///
    /// Iterates over non-overlapping match spans from `iter` and builds a new string by
    /// copying the gaps between spans verbatim and calling `push_replacement` to emit the
    /// substitution for each span.
    ///
    /// # Type Parameters
    /// * `I` — an iterator that yields `(start_byte, end_byte, match_data)` tuples for each
    ///   matched span (non-overlapping, in order).
    /// * `M` — the match payload forwarded to `push_replacement` (e.g. a replacement `char`,
    ///   `&str`, or a `usize` index into a replacement list).
    /// * `F` — a closure `FnMut(&mut String, M)` that writes the replacement for one span.
    ///
    /// Returns `Some(result)` when at least one span was replaced, or `None` when `iter`
    /// yielded no matches (zero allocations).
    #[inline(always)]
    fn replace_scan<I, M, F>(text: &str, mut iter: I, mut push_replacement: F) -> Option<String>
    where
        I: Iterator<Item = (usize, usize, M)>,
        F: FnMut(&mut String, M),
    {
        if let Some((start, end, m)) = iter.next() {
            let mut result = get_string_from_pool(text.len());
            result.push_str(&text[0..start]);
            push_replacement(&mut result, m);
            let mut last_end = end;
            for (start, end, m) in iter {
                result.push_str(&text[last_end..start]);
                push_replacement(&mut result, m);
                last_end = end;
            }
            result.push_str(&text[last_end..]);
            Some(result)
        } else {
            None
        }
    }

    /// Replaces all matched patterns in `text`.
    ///
    /// Returns `Some(result)` when at least one replacement was made, or `None` when the
    /// text is unchanged (zero allocations).
    #[inline(always)]
    pub(crate) fn replace_all(&self, text: &str) -> Option<String> {
        match self {
            ProcessMatcher::SingleChar(matcher) => match matcher {
                SingleCharMatcher::Fanjian { .. } => {
                    Self::replace_scan(text, matcher.fanjian_iter(text), |result, m| {
                        if let SingleCharMatch::Char(c) = m {
                            result.push(c);
                        }
                    })
                }
                SingleCharMatcher::Pinyin { .. } => {
                    Self::replace_scan(text, matcher.pinyin_iter(text), |result, m| {
                        if let SingleCharMatch::Str(s) = m {
                            result.push_str(s);
                        }
                    })
                }
                SingleCharMatcher::Delete { .. } => {
                    debug_assert!(false, "replace_all called on Delete matcher");
                    None
                }
            },
            ProcessMatcher::MultiChar(mc) => {
                let replacements = mc.replace_list();
                Self::replace_scan(text, mc.find_iter(text), |result, idx| {
                    result.push_str(replacements[idx]);
                })
            }
        }
    }

    /// Removes all matched patterns from `text`.
    ///
    /// Returns `Some(result)` when at least one character or span was removed, or `None`
    /// when nothing matched (zero allocations).
    #[inline(always)]
    pub(crate) fn delete_all(&self, text: &str) -> Option<String> {
        let ProcessMatcher::SingleChar(matcher) = self else {
            debug_assert!(false, "delete_all called on non-Delete matcher");
            return None;
        };
        Self::replace_scan(text, matcher.delete_iter(text), |_, _| {})
    }
}

/// Returns a lazily-initialized `ProcessMatcher` for a **single-bit** [`ProcessType`].
///
/// The result is cached as the same `&'static` reference via OnceLock, so subsequent
/// calls for the same type return the same `&'static ProcessMatcher` without lock contention.
///
/// The construction strategy depends on the type:
/// - **Normalize** — builds a leftmost-longest Aho-Corasick automaton (`daachorse` by default,
///   DFA variant under the `dfa` feature). With `runtime_build` the normalization table is
///   read from `process_map/` text files; otherwise build-time artifacts are loaded lazily.
/// - **Fanjian / PinYin / PinYinChar** — 2-stage page tables built either from embedded
///   artifacts or from the source maps under `runtime_build`.
/// - **Delete** — a flat Unicode bitset built either from embedded artifacts or from the
///   source delete lists.
/// - **None** — an empty Aho-Corasick automaton (no-op).
///
/// # Panics
/// Passing anything other than a supported single-bit value is unsupported. In non-
/// `runtime_build` builds that reaches `unreachable!()`.
pub(crate) fn get_process_matcher(process_type_bit: ProcessType) -> &'static ProcessMatcher {
    let index = process_type_bit.bits().trailing_zeros() as usize;
    debug_assert!(index < 8, "ProcessType bit index out of bounds");

    PROCESS_MATCHER_CACHE[index].get_or_init(|| {
        #[cfg(feature = "runtime_build")]
        {
            match process_type_bit {
                ProcessType::Fanjian => {
                    let mut map = HashMap::new();
                    for line in FANJIAN.trim().lines() {
                        let mut split = line.split('\t');
                        let k = split.next().unwrap().chars().next().unwrap() as u32;
                        let v = split.next().unwrap().chars().next().unwrap() as u32;
                        if k != v {
                            map.insert(k, v);
                        }
                    }
                    ProcessMatcher::SingleChar(SingleCharMatcher::fanjian_from_map(map))
                }
                ProcessType::PinYin | ProcessType::PinYinChar => {
                    let mut map = HashMap::new();
                    for line in PINYIN.trim().lines() {
                        let mut split = line.split('\t');
                        let k = split.next().unwrap().chars().next().unwrap() as u32;
                        let v = split.next().unwrap();
                        map.insert(k, v);
                    }
                    ProcessMatcher::SingleChar(SingleCharMatcher::pinyin_from_map(
                        map,
                        process_type_bit == ProcessType::PinYinChar,
                    ))
                }
                ProcessType::Delete => ProcessMatcher::SingleChar(
                    SingleCharMatcher::delete_from_sources(TEXT_DELETE, WHITE_SPACE),
                ),
                ProcessType::Normalize => {
                    let mut process_dict: HashMap<&'static str, &'static str> = HashMap::new();
                    for process_map in [NORM, NUM_NORM] {
                        process_dict.extend(process_map.trim().lines().map(|pair_str| {
                            let mut split = pair_str.split('\t');
                            (split.next().unwrap(), split.next().unwrap())
                        }));
                    }
                    process_dict.retain(|&key, &mut value| key != value);
                    ProcessMatcher::MultiChar(MultiCharMatcher::new_from_dict(process_dict))
                }
                _ => ProcessMatcher::MultiChar(MultiCharMatcher::new_empty()),
            }
        }

        #[cfg(not(feature = "runtime_build"))]
        {
            match process_type_bit {
                ProcessType::None => ProcessMatcher::MultiChar(MultiCharMatcher::new_empty()),
                ProcessType::Fanjian => ProcessMatcher::SingleChar(SingleCharMatcher::fanjian(
                    Cow::Borrowed(FANJIAN_L1_BYTES),
                    Cow::Borrowed(FANJIAN_L2_BYTES),
                )),
                ProcessType::Delete => ProcessMatcher::SingleChar(SingleCharMatcher::delete(
                    Cow::Borrowed(DELETE_BITSET_BYTES),
                )),
                ProcessType::Normalize => {
                    #[cfg(feature = "dfa")]
                    {
                        ProcessMatcher::MultiChar(
                            MultiCharMatcher::new(NORMALIZE_PROCESS_LIST_STR.lines())
                                .with_replace_list(
                                    NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                                ),
                        )
                    }
                    #[cfg(not(feature = "dfa"))]
                    {
                        ProcessMatcher::MultiChar(
                            MultiCharMatcher::deserialize_from(NORMALIZE_PROCESS_MATCHER_BYTES)
                                .with_replace_list(
                                    NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                                ),
                        )
                    }
                }
                ProcessType::PinYin => ProcessMatcher::SingleChar(SingleCharMatcher::pinyin(
                    Cow::Borrowed(PINYIN_L1_BYTES),
                    Cow::Borrowed(PINYIN_L2_BYTES),
                    Cow::Borrowed(PINYIN_STR_BYTES),
                    false,
                )),
                ProcessType::PinYinChar => ProcessMatcher::SingleChar(SingleCharMatcher::pinyin(
                    Cow::Borrowed(PINYIN_L1_BYTES),
                    Cow::Borrowed(PINYIN_L2_BYTES),
                    Cow::Borrowed(PINYIN_STR_BYTES),
                    true,
                )),
                _ => unreachable!(),
            }
        }
    })
}

/// Applies a composite [`ProcessType`] pipeline to `text` and returns the final result.
///
/// Transformations are applied in [`ProcessType::iter`] order. Each step fetches a cached
/// engine and either replaces or deletes matching spans.
/// `Cow::Borrowed` is returned when no step modifies the text.
/// This is the "final result only" helper: intermediate variants are discarded.
///
/// For use cases where multiple composite types share common prefixes, prefer
/// [`crate::walk_process_tree`] which avoids redundant intermediate computations.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{text_process, ProcessType};
///
/// // Full-width digit '２' (U+FF12) normalizes to ASCII '2'.
/// let result = text_process(ProcessType::Normalize, "２");
/// assert_eq!(result.as_ref(), "2");
/// ```
#[inline(always)]
pub fn text_process<'a>(process_type: ProcessType, text: &'a str) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);

    for process_type_bit in process_type.iter() {
        let pm = get_process_matcher(process_type_bit);

        match process_type_bit {
            ProcessType::None => continue,
            ProcessType::Delete => {
                if let Some(processed) = pm.delete_all(result.as_ref())
                    && let Cow::Owned(old) = std::mem::replace(&mut result, Cow::Owned(processed))
                {
                    return_string_to_pool(old);
                }
            }
            _ => {
                if let Some(processed) = pm.replace_all(result.as_ref())
                    && let Cow::Owned(old) = std::mem::replace(&mut result, Cow::Owned(processed))
                {
                    return_string_to_pool(old);
                }
            }
        }
    }

    result
}

/// Shared implementation for [`reduce_text_process`] and [`reduce_text_process_emit`].
///
/// When `overwrite_replace` is `false`, each replace-type step appends a new entry.
/// When `overwrite_replace` is `true`, replace-type steps overwrite the last entry in place;
/// only `Delete` steps append. See the public wrappers for full semantics.
fn reduce_text_process_inner<'a>(
    process_type: ProcessType,
    text: &'a str,
    overwrite_replace: bool,
) -> Vec<Cow<'a, str>> {
    let mut text_list: Vec<Cow<'a, str>> = Vec::new();
    text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let pm = get_process_matcher(process_type_bit);
        let current_text = text_list
            .last_mut()
            .expect("text_list is never empty (seeded with original text)");

        match process_type_bit {
            ProcessType::None => continue,
            ProcessType::Delete => {
                if let Some(processed) = pm.delete_all(current_text.as_ref()) {
                    text_list.push(Cow::Owned(processed));
                }
            }
            _ => {
                if let Some(processed) = pm.replace_all(current_text.as_ref()) {
                    if overwrite_replace {
                        *current_text = Cow::Owned(processed);
                    } else {
                        text_list.push(Cow::Owned(processed));
                    }
                }
            }
        }
    }

    text_list
}

/// Applies a composite [`ProcessType`] pipeline to `text`, recording each changed result.
///
/// The first entry is always `Cow::Borrowed(text)` (the original input). Steps that leave
/// the text unchanged add no entry. Use this when you want a step-by-step view of one
/// composite pipeline.
///
/// For generating all variants needed for matching, prefer [`crate::walk_process_tree`].
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, reduce_text_process};
///
/// let variants = reduce_text_process(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");
///
/// assert_eq!(variants.len(), 4);
/// assert_eq!(variants[0], "~ᗩ~躶~𝚩~軆~Ⲉ~");
/// assert_eq!(variants[1], "~ᗩ~裸~𝚩~軆~Ⲉ~");
/// assert_eq!(variants[2], "ᗩ裸𝚩軆Ⲉ");
/// assert_eq!(variants[3], "a裸b軆c");
/// ```
#[inline(always)]
pub fn reduce_text_process<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    reduce_text_process_inner(process_type, text, false)
}

/// Like [`reduce_text_process`], but composing replace-type steps in-place.
///
/// When a *replace*-type step changes the text, the result overwrites the last entry
/// rather than appending a new one. Only `Delete` steps append a new entry.
///
/// Used internally by `SimpleMatcher::new` to register all emitted patterns that may be
/// scanned after Delete-normalized text is produced.
/// The returned variants correspond to distinct strings that may need to be indexed,
/// not every intermediate step that happened along the way.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{ProcessType, reduce_text_process_emit};
///
/// let variants = reduce_text_process_emit(ProcessType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");
///
/// // emit: Fanjian overwrites (1 entry), Delete appends, Normalize overwrites last
/// // 1. Fanjian:  ["~ᗩ~裸~𝚩~軆~Ⲉ~"]              (overwritten)
/// // 2. Delete:   ["~ᗩ~裸~𝚩~軆~Ⲉ~", "ᗩ裸𝚩軆Ⲉ"]   (pushed)
/// // 3. Normalize:["~ᗩ~裸~𝚩~軆~Ⲉ~", "a裸b軆c"]    (overwritten last)
/// assert_eq!(variants.len(), 2);
/// assert_eq!(variants[0], "~ᗩ~裸~𝚩~軆~Ⲉ~");
/// assert_eq!(variants[1], "a裸b軆c");
/// ```
#[inline(always)]
pub fn reduce_text_process_emit<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    reduce_text_process_inner(process_type, text, true)
}
