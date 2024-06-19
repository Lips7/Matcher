use std::borrow::Cow;
use std::sync::Arc;

use ahash::AHashMap;
use aho_corasick::{
    AhoCorasick, AhoCorasickBuilder, AhoCorasickKind::DFA, MatchKind as AhoCorasickMatchKind,
};
#[allow(unused_imports)]
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
};
use lazy_static::lazy_static;
use nohash_hasher::IntMap;
use parking_lot::RwLock;

#[cfg(feature = "prebuilt")]
use crate::process::constants::prebuilt_feature::*;

#[cfg(feature = "runtime_build")]
use crate::process::constants::runtime_build_feature::*;

use crate::SimpleMatchType;

lazy_static! {
    pub static ref PROCESS_MATCHER_CACHE: RwLock<IntMap<SimpleMatchType, Arc<(Vec<&'static str>, ProcessMatcher)>>> =
        RwLock::new(IntMap::default());
}

#[derive(Clone)]
/// [ProcessMatcher] is an enum designed to differentiate between matching strategies based on the input text type.
///
/// This enum is used as part of the text processing framework, allowing for specialized handling of Chinese text
/// compared to other types of text. It supports two variants:
///
/// - [Chinese](ProcessMatcher::Chinese): Utilizes a [`CharwiseDoubleArrayAhoCorasick<u64>`] matcher optimized for Chinese characters.
/// - [Others](ProcessMatcher::Others): Uses an [AhoCorasick] matcher for all other types of text.
///
/// By distinguishing between these two categories, [ProcessMatcher] allows for more efficient and accurate pattern
/// matching tailored to the linguistic properties of the text being processed.
pub enum ProcessMatcher {
    Chinese(CharwiseDoubleArrayAhoCorasick<u64>),
    Others(AhoCorasick),
}

impl ProcessMatcher {
    #[inline(always)]
    /// Replaces all occurrences of patterns in the input text with corresponding replacements from the provided list.
    ///
    /// This function performs a find-and-replace operation on the input text. It searches for patterns using the internal matcher
    /// (either [`CharwiseDoubleArrayAhoCorasick<u64>`] for Chinese text or [AhoCorasick] for other text) and replaces each match
    /// with the corresponding replacement string from the given `process_replace_list`.
    ///
    /// # Parameters
    ///
    /// * `text`: A reference to the input text where replacements will be made.
    /// * `process_replace_list`: A slice of replacement strings. Each match from the internal matcher is replaced with the
    ///   corresponding string from this list.
    ///
    /// # Returns
    ///
    /// * `(bool, Cow<'a, str>)`: A tuple where the first element is a boolean indicating whether any replacements were made,
    ///   and the second element is a [Cow] string containing the modified text. If no replacements were made, the original text
    ///   is returned as a [Cow::Borrowed].
    ///
    /// # Safety
    ///
    /// This function uses unsafe code to access slices and indices. This assumes that the match indices and the replacement list
    /// indices are always within bounds.
    ///
    /// # Example
    ///
    /// ```
    /// use matcher_rs::{ProcessMatcher, SimpleMatchType, get_process_matcher};
    ///
    /// let cached_result = get_process_matcher(SimpleMatchType::Normalize);
    /// let (process_replace_list, matcher) = cached_result.as_ref(); // Assume this returns a valid ProcessMatcher
    /// let text = "Some text for processing";
    /// let (replaced, result) = matcher.replace_all(text, &process_replace_list);
    /// ```
    pub fn replace_all<'a>(
        &self,
        text: &'a str,
        process_replace_list: &[&str],
    ) -> (bool, Cow<'a, str>) {
        let mut result = String::with_capacity(text.len());
        let mut last_end = 0;
        match self {
            ProcessMatcher::Chinese(ac) => {
                for mat in ac.find_iter(text) {
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    result.push_str(unsafe {
                        process_replace_list.get_unchecked(mat.value() as usize)
                    });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Others(ac) => {
                for mat in ac.find_iter(text) {
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    result.push_str(unsafe {
                        process_replace_list.get_unchecked(mat.pattern().as_usize())
                    });
                    last_end = mat.end();
                }
            }
        }

        if last_end > 0 {
            result.push_str(unsafe { text.get_unchecked(last_end..) });
            (true, Cow::Owned(result))
        } else {
            (false, Cow::Borrowed(text))
        }
    }

    #[inline(always)]
    /// Deletes all occurrences of patterns in the input text.
    ///
    /// This function performs a delete operation on the input text. It searches for patterns using the internal matcher
    /// (either [`CharwiseDoubleArrayAhoCorasick<u64>`] for Chinese text or [AhoCorasick] for other text) and removes each match
    /// from the input.
    ///
    /// # Parameters
    ///
    /// * `text`: A reference to the input text where patterns will be deleted.
    ///
    /// # Returns
    ///
    /// * `(bool, Cow<'a, str>)`: A tuple where the first element is a boolean indicating whether any deletions were made,
    ///   and the second element is a [Cow] string containing the modified text. If no deletions were made, the original text
    ///   is returned as a [Cow::Borrowed].
    ///
    /// # Safety
    ///
    /// This function uses unsafe code to access slices and indices. This assumes that the match indices are always within bounds.
    ///
    /// # Example
    ///
    /// ```
    /// use matcher_rs::{ProcessMatcher, SimpleMatchType, get_process_matcher};
    ///
    /// let cached_result = get_process_matcher(SimpleMatchType::Normalize);
    /// let (process_replace_list, matcher) = cached_result.as_ref(); // Assume this returns a valid ProcessMatcher
    /// let text = "Some text for processing";
    /// let (deleted, result) = matcher.delete_all(text);
    /// ```
    pub fn delete_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
        let mut result = String::with_capacity(text.len());
        let mut last_end = 0;
        match self {
            ProcessMatcher::Chinese(ac) => {
                for mat in ac.find_iter(text) {
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Others(ac) => {
                for mat in ac.find_iter(text) {
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    last_end = mat.end();
                }
            }
        }

        if last_end > 0 {
            result.push_str(unsafe { text.get_unchecked(last_end..) });
            (true, Cow::Owned(result))
        } else {
            (false, Cow::Borrowed(text))
        }
    }
}

#[cfg(feature = "runtime_build")]
/// Generates a [ProcessMatcher] based on the specified [SimpleMatchType].
///
/// This function generates a matcher and a corresponding replacement list
/// tailored to the given [SimpleMatchType]. The [SimpleMatchType] determines
/// the kind of text processing and transformation rules that will be applied,
/// whether it's deleting text, normalizing text, converting between simplified
/// and traditional Chinese characters, etc.
///
/// The function constructs a HashMap (`process_dict`) containing string
/// transformation rules. These rules are derived from predefined sets of
/// string mappings, which are filtered and adjusted based on the given
/// [SimpleMatchType].
///
/// Finally, the function creates an appropriate matcher ([AhoCorasick] for general text
/// or potentially [CharwiseDoubleArrayAhoCorasick] for specific types, though the latter
/// is commented out here). It returns a tuple containing a list of replacement strings
/// and the constructed [ProcessMatcher].
///
/// # Parameters
///
/// * `simple_match_type_bit`: The type of matching and processing to be applied, specified
///   by a [SimpleMatchType] enum value.
///
/// # Returns
///
/// A tuple containing:
/// 1. A vector of replacement strings ([Vec<&'static str>]).
/// 2. A [ProcessMatcher] which can be used to perform the specified matching and text processing operations.
///
/// # Example
///
/// ```
/// use matcher_rs::{SimpleMatchType, get_process_matcher};
///
/// let cached_result = get_process_matcher(SimpleMatchType::TextDelete);
/// let (process_replace_list, matcher) = cached_result.as_ref();
/// // you can now use `matcher` with `process_replace_list` to perform text replacement or deletion
/// ```
///
/// # Notes
///
/// - The function assumes that specific datasets like `FANJIAN`, `UNICODE`, `PUNCTUATION_SPECIAL`, etc.,
///   are predefined and contain the necessary mappings.
/// - It uses [AhoCorasick] for most match types, but has a commented-out section for
///   [CharwiseDoubleArrayAhoCorasick] for specific types.
///
/// # Safety
///
/// The function uses `unwrap()` when accessing elements in the string mapping. It assumes that the
/// provided datasets are correctly formatted and always provide key-value pairs for transformations.
///
/// # Limitations
///
/// The commented-out section for [CharwiseDoubleArrayAhoCorasick] implies that it is not yet used in
/// the current version. Any errors regarding missing or incorrectly formatted string mappings will
/// result in a panic due to the use of `unwrap()`.
pub fn get_process_matcher(
    simple_match_type_bit: SimpleMatchType,
) -> Arc<(Vec<&'static str>, ProcessMatcher)> {
    {
        let process_matcher_cache = PROCESS_MATCHER_CACHE.read();

        if let Some(cached_result) = process_matcher_cache.get(&simple_match_type_bit) {
            return Arc::clone(cached_result);
        }
    }

    {
        let mut process_dict = AHashMap::default();

        match simple_match_type_bit {
            SimpleMatchType::None => {}

            SimpleMatchType::Fanjian => {
                for str_conv_map in [FANJIAN, UNICODE] {
                    process_dict.extend(str_conv_map.trim().lines().map(|pair_str| {
                        let mut pair_str_split = pair_str.split('\t');
                        (
                            pair_str_split.next().unwrap(),
                            pair_str_split.next().unwrap(),
                        )
                    }));
                }
            }

            SimpleMatchType::WordDelete => {
                process_dict.extend(
                    PUNCTUATION_SPECIAL
                        .trim()
                        .lines()
                        .map(|pair_str| (pair_str, "")),
                );

                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }

            SimpleMatchType::TextDelete => {
                for str_conv_map in [PUNCTUATION_SPECIAL, CN_SPECIAL, EN_SPECIAL] {
                    process_dict.extend(str_conv_map.trim().lines().map(|pair_str| (pair_str, "")));
                }

                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }
            SimpleMatchType::Normalize => {
                for str_conv_map in [UPPER_LOWER, EN_VARIATION, NUM_NORM] {
                    process_dict.extend(str_conv_map.trim().lines().map(|pair_str| {
                        let mut pair_str_split = pair_str.split('\t');
                        (
                            pair_str_split.next().unwrap(),
                            pair_str_split.next().unwrap(),
                        )
                    }));
                }
            }

            SimpleMatchType::PinYin => {
                process_dict.extend(PINYIN.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }

            SimpleMatchType::PinYinChar => {
                process_dict.extend(PINYIN_CHAR.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        // Each line in the conversion data corresponds to a key-value pair.
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }
            _ => {}
        }

        process_dict
            .retain(|&key, &mut value| (key == "#" || !key.starts_with('#')) && key != value);

        let (process_replace_list, process_matcher) = match simple_match_type_bit {
            SimpleMatchType::Fanjian | SimpleMatchType::PinYin | SimpleMatchType::PinYinChar => (
                process_dict.iter().map(|(_, &val)| val).collect(),
                CharwiseDoubleArrayAhoCorasickBuilder::new()
                    .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                    .build(
                        process_dict
                            .iter()
                            .map(|(&key, _)| key)
                            .collect::<Vec<&str>>(),
                    )
                    .unwrap(),
            ),
            _ => (
                process_dict.iter().map(|(_, &val)| val).collect(),
                AhoCorasickBuilder::new()
                    .kind(Some(DFA))
                    .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                    .build(
                        process_dict
                            .iter()
                            .map(|(&key, _)| key)
                            .collect::<Vec<&str>>(),
                    )
                    .unwrap(),
            ),
        };

        let uncached_result = Arc::new((process_replace_list, process_matcher));
        let mut process_matcher_cache = PROCESS_MATCHER_CACHE.write();
        process_matcher_cache.insert(simple_match_type_bit, Arc::clone(&uncached_result));
        uncached_result
    }
}

#[cfg(feature = "prebuilt")]
/// Generates a [ProcessMatcher] based on the provided [SimpleMatchType].
///
/// This implementation makes use of prebuilt, serialized data for certain match types to enhance
/// performance by avoiding runtime construction of the matcher and replacement list. The function
/// expects that the relevant data has been compiled with the `prebuilt` feature.
///
/// # Parameters
///
/// - `simple_match_type_bit`: A variant of [SimpleMatchType] enumerating the various matching strategies.
///
/// # Returns
///
/// - A tuple containing:
///   - A vector of replacement patterns ([`Vec<&str>`]).
///   - A [ProcessMatcher] object relevant to the specified match type.
///
/// # Safety
///
/// For certain match types like [Fanjian](SimpleMatchType::Fanjian), [PinYin](SimpleMatchType::PinYin), [PinYinChar](SimpleMatchType::PinYinChar), unsafe deserialization is performed
/// using [deserialize_unchecked](CharwiseDoubleArrayAhoCorasick::deserialize_unchecked). This assumes that the prebuilt serialized data is trustworthy and correctly formatted.
///
/// # Match Types
///
/// The function supports the following match types:
///
/// - [SimpleMatchType::None]: Returns an empty matcher.
/// - [SimpleMatchType::Fanjian]: Returns a matcher using prebuilt replacement list and matcher data for Fanjian.
/// - [SimpleMatchType::WordDelete]: Builds a matcher for deleting punctuation and whitespace.
/// - [SimpleMatchType::TextDelete]: Builds a matcher for deleting special text characters and whitespace.
/// - [SimpleMatchType::Normalize]: Returns a matcher using prebuilt normalization data.
/// - [SimpleMatchType::PinYin]: Returns a matcher using prebuilt replacement list and matcher data for PinYin.
/// - [SimpleMatchType::PinYinChar]: Returns a matcher using prebuilt replacement list and matcher data for PinYin characters.
///
/// # Example
///
/// ```
/// use matcher_rs::{SimpleMatchType, get_process_matcher};
///
/// let cached_result = get_process_matcher(SimpleMatchType::TextDelete);
/// let (process_replace_list, matcher) = cached_result.as_ref();
/// ```
///
/// This function requires the `prebuilt` feature to be enabled.
pub fn get_process_matcher(
    simple_match_type_bit: SimpleMatchType,
) -> Arc<(Vec<&'static str>, ProcessMatcher)> {
    {
        let process_matcher_cache = PROCESS_MATCHER_CACHE.read();

        if let Some(cached_result) = process_matcher_cache.get(&simple_match_type_bit) {
            return Arc::clone(cached_result);
        }
    }

    {
        let (process_replace_list, process_matcher) = match simple_match_type_bit {
            SimpleMatchType::None => {
                let empty_patterns: Vec<&str> = Vec::new();
                (
                    Vec::new(),
                    ProcessMatcher::Others(AhoCorasick::new(&empty_patterns).unwrap()),
                )
            }
            SimpleMatchType::Fanjian => (
                FANJIAN_PROCESS_REPLACE_LIST_STR.lines().collect(),
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u64>::deserialize_unchecked(
                        FANJIAN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),
            SimpleMatchType::WordDelete => {
                let mut process_dict = AHashMap::new();
                process_dict.extend(
                    PUNCTUATION_SPECIAL
                        .trim()
                        .lines()
                        .map(|pair_str| (pair_str, "")),
                );
                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
                process_dict.retain(|&key, &mut value| {
                    (key == "#" || !key.starts_with('#')) && key != value
                });
                let process_list = process_dict
                    .iter()
                    .map(|(&key, _)| key)
                    .collect::<Vec<&str>>();

                (
                    Vec::new(),
                    ProcessMatcher::Others(
                        AhoCorasickBuilder::new()
                            .kind(Some(DFA))
                            .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                            .build(&process_list)
                            .unwrap(),
                    ),
                )
            }
            SimpleMatchType::TextDelete => {
                let mut process_dict = AHashMap::new();
                for str_conv_map in [PUNCTUATION_SPECIAL, CN_SPECIAL, EN_SPECIAL] {
                    process_dict.extend(str_conv_map.trim().lines().map(|pair_str| (pair_str, "")));
                }
                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
                process_dict.retain(|&key, &mut value| {
                    (key == "#" || !key.starts_with('#')) && key != value
                });
                let process_list = process_dict
                    .iter()
                    .map(|(&key, _)| key)
                    .collect::<Vec<&str>>();

                (
                    Vec::new(),
                    ProcessMatcher::Others(
                        AhoCorasickBuilder::new()
                            .kind(Some(DFA))
                            .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                            .build(&process_list)
                            .unwrap(),
                    ),
                )
            }
            SimpleMatchType::Normalize => (
                NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                ProcessMatcher::Others(
                    AhoCorasickBuilder::new()
                        .kind(Some(DFA))
                        .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                        .build(NORMALIZE_PROCESS_LIST_STR.lines())
                        .unwrap(),
                ),
            ),
            SimpleMatchType::PinYin => (
                PINYIN_PROCESS_REPLACE_LIST_STR.lines().collect(),
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u64>::deserialize_unchecked(
                        PINYIN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),

            SimpleMatchType::PinYinChar => (
                PINYINCHAR_PROCESS_REPLACE_LIST_STR.lines().collect(),
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u64>::deserialize_unchecked(
                        PINYINCHAR_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),
            _ => unreachable!(),
        };

        let uncached_result = Arc::new((process_replace_list, process_matcher));
        let mut process_matcher_cache = PROCESS_MATCHER_CACHE.write();
        process_matcher_cache.insert(simple_match_type_bit, Arc::clone(&uncached_result));
        uncached_result
    }
}
