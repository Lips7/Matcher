use std::borrow::Cow;
use std::sync::Arc;

use ahash::{AHashMap, HashMapExt};
use aho_corasick_unsafe::{
    AhoCorasick, AhoCorasickBuilder, AhoCorasickKind, MatchKind as AhoCorasickMatchKind,
};
#[cfg(feature = "prebuilt")]
use daachorse::CharwiseDoubleArrayAhoCorasick;
#[cfg(feature = "runtime_build")]
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
};
use id_set::IdSet;
use lazy_static::lazy_static;
use nohash_hasher::IntMap;
use parking_lot::RwLock;
#[cfg(feature = "serde")]
use sonic_rs::{Deserialize, Serialize};
use tinyvec::ArrayVec;

#[cfg(feature = "prebuilt")]
use crate::process::constants::prebuilt_feature::*;

#[cfg(feature = "runtime_build")]
use crate::process::constants::runtime_build_feature::*;

use crate::SimpleMatchType;

type ProcessMatcherCache =
    RwLock<IntMap<SimpleMatchType, Arc<(Vec<&'static str>, ProcessMatcher)>>>;

lazy_static! {
    pub static ref PROCESS_MATCHER_CACHE: ProcessMatcherCache =
        RwLock::new(IntMap::with_capacity(8));
}

/// [ProcessMatcher] is an enum designed to differentiate between matching strategies based on the input text type.
///
/// This enum is used as part of the text processing framework, allowing for specialized handling of Chinese text
/// compared to other types of text. It supports two variants:
///
/// - [LeftMost](ProcessMatcher::LeftMost): Utilizes a [`CharwiseDoubleArrayAhoCorasick<u32>`] matcher optimized for leftmost search mechanics.
/// - [Chinese](ProcessMatcher::Chinese): Utilizes a [`CharwiseDoubleArrayAhoCorasick<u32>`] matcher optimized for Chinese characters.
/// - [Others](ProcessMatcher::Others): Uses an [AhoCorasick] matcher for all other types of text.
///
/// By distinguishing between these two categories, [ProcessMatcher] allows for more efficient and accurate pattern
/// matching tailored to the linguistic properties of the text being processed.
#[derive(Clone)]
pub enum ProcessMatcher {
    #[cfg(not(feature = "dfa"))]
    LeftMost(CharwiseDoubleArrayAhoCorasick<u32>),
    Chinese(CharwiseDoubleArrayAhoCorasick<u32>),
    Others(AhoCorasick),
}

impl ProcessMatcher {
    /// Replaces all occurrences of patterns in the input text with corresponding replacements from the provided list.
    ///
    /// This function performs a find-and-replace operation on the input text. It searches for patterns using the internal matcher
    /// (either [`CharwiseDoubleArrayAhoCorasick<u32>`] for Chinese text or [AhoCorasick] for other text) and replaces each match
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
    #[inline(always)]
    pub fn replace_all<'a>(
        &self,
        text: &'a str,
        process_replace_list: &[&str],
    ) -> (bool, Cow<'a, str>) {
        let mut result = String::with_capacity(text.len());
        let mut last_end = 0;
        match self {
            #[cfg(not(feature = "dfa"))]
            ProcessMatcher::LeftMost(ac) => {
                for mat in ac.leftmost_find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    // Guaranteed not failed
                    result.push_str(unsafe {
                        process_replace_list.get_unchecked(mat.value() as usize)
                    });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Chinese(ac) => {
                for mat in ac.find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    // Guaranteed not failed
                    result.push_str(unsafe {
                        process_replace_list.get_unchecked(mat.value() as usize)
                    });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Others(ac) => {
                for mat in ac.find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    // Guaranteed not failed
                    result.push_str(unsafe {
                        process_replace_list.get_unchecked(mat.pattern().as_usize())
                    });
                    last_end = mat.end();
                }
            }
        }

        if last_end > 0 {
            // Guaranteed not failed
            result.push_str(unsafe { text.get_unchecked(last_end..) });
            (true, Cow::Owned(result))
        } else {
            (false, Cow::Borrowed(text))
        }
    }

    /// Deletes all occurrences of patterns in the input text.
    ///
    /// This function performs a delete operation on the input text. It searches for patterns using the internal matcher
    /// (either [`CharwiseDoubleArrayAhoCorasick<u32>`] for Chinese text or [AhoCorasick] for other text) and removes each match
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
    #[inline(always)]
    pub fn delete_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
        let mut result = String::with_capacity(text.len());
        let mut last_end = 0;
        match self {
            #[cfg(not(feature = "dfa"))]
            ProcessMatcher::LeftMost(ac) => {
                for mat in ac.leftmost_find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Chinese(ac) => {
                for mat in ac.find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Others(ac) => {
                for mat in ac.find_iter(text) {
                    // Guaranteed not failed
                    result.push_str(unsafe { text.get_unchecked(last_end..mat.start()) });
                    last_end = mat.end();
                }
            }
        }

        if last_end > 0 {
            // Guaranteed not failed
            result.push_str(unsafe { text.get_unchecked(last_end..) });
            (true, Cow::Owned(result))
        } else {
            (false, Cow::Borrowed(text))
        }
    }
}

/// Generates a [ProcessMatcher] based on the provided [SimpleMatchType] at runtime.
///
/// This implementation constructs the matcher and replacement list at runtime based on the specified [SimpleMatchType].
/// The function generates the matcher data and caches it for future use.
///
/// # Parameters
///
/// - `smt_bit`: A variant of [SimpleMatchType] which specifies the type of matching operation to be performed.
///
/// # Returns
///
/// - An [`Arc`] containing a tuple:
///   - A vector of replacement patterns ([`Vec<&str>`]).
///   - A [ProcessMatcher] object configured for the specified match type.
///
/// # Match Types
///
/// The function supports the following match types:
///
/// - [SimpleMatchType::None]: Returns an empty matcher.
/// - [SimpleMatchType::Fanjian]: Builds a matcher for Fanjian text normalization using runtime construction.
/// - [SimpleMatchType::WordDelete]: Builds a matcher for deleting whitespace and punctuation.
/// - [SimpleMatchType::TextDelete]: Builds a matcher for deleting special text characters and whitespace.
/// - [SimpleMatchType::Normalize]: Builds a matcher for normalizing symbols, text, and numbers.
/// - [SimpleMatchType::PinYin]: Builds a matcher for converting text to PinYin using runtime construction.
/// - [SimpleMatchType::PinYinChar]: Builds a matcher for converting text to PinYin characters using runtime construction.
///
/// # Notes
///
/// - The matcher construction utilizes the Aho-Corasick algorithm for efficient pattern matching.
/// - The function retains key-value pairs in the replacement dictionary where the key and value are not identical.
/// - The matcher data is cached to optimize repeated calls with the same match type, improving performance.
///
/// The function may use either the `Chinese` or `Others` variant of the [ProcessMatcher], depending on the [[SimpleMatchType]].
#[cfg(feature = "runtime_build")]
pub fn get_process_matcher(smt_bit: SimpleMatchType) -> Arc<(Vec<&'static str>, ProcessMatcher)> {
    {
        let process_matcher_cache = PROCESS_MATCHER_CACHE.read();

        if let Some(cached_result) = process_matcher_cache.get(&smt_bit) {
            return Arc::clone(cached_result);
        }
    }

    {
        let mut process_dict = AHashMap::default();

        match smt_bit {
            SimpleMatchType::None => {}

            SimpleMatchType::Fanjian => {
                process_dict.extend(FANJIAN.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }

            SimpleMatchType::WordDelete => {
                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }

            SimpleMatchType::TextDelete => {
                process_dict.extend(TEXT_DELETE.trim().lines().map(|pair_str| (pair_str, "")));
                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }
            SimpleMatchType::Normalize => {
                for str_conv_map in [NORM, NUM_NORM] {
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
                process_dict.extend(PINYIN.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap().trim_matches(' '),
                    )
                }));
            }
            _ => {}
        }

        process_dict.retain(|&key, &mut value| key != value);

        let (process_replace_list, process_matcher) = match smt_bit {
            SimpleMatchType::Fanjian | SimpleMatchType::PinYin | SimpleMatchType::PinYinChar => (
                process_dict.iter().map(|(_, &val)| val).collect(),
                ProcessMatcher::Chinese(
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
            ),
            #[cfg(not(feature = "dfa"))]
            SimpleMatchType::TextDelete | SimpleMatchType::Normalize => (
                process_dict.iter().map(|(_, &val)| val).collect(),
                ProcessMatcher::LeftMost(
                    CharwiseDoubleArrayAhoCorasickBuilder::new()
                        .match_kind(DoubleArrayAhoCorasickMatchKind::LeftmostLongest)
                        .build(
                            process_dict
                                .iter()
                                .map(|(&key, _)| key)
                                .collect::<Vec<&str>>(),
                        )
                        .unwrap(),
                ),
            ),
            _ => (
                process_dict.iter().map(|(_, &val)| val).collect(),
                ProcessMatcher::Others(
                    AhoCorasickBuilder::new()
                        .kind(Some(AhoCorasickKind::DFA))
                        .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                        .build(
                            process_dict
                                .iter()
                                .map(|(&key, _)| key)
                                .collect::<Vec<&str>>(),
                        )
                        .unwrap(),
                ),
            ),
        };

        let uncached_result = Arc::new((process_replace_list, process_matcher));
        let mut process_matcher_cache = PROCESS_MATCHER_CACHE.write();
        process_matcher_cache.insert(smt_bit, Arc::clone(&uncached_result));
        uncached_result
    }
}

/// Generates a [ProcessMatcher] based on the provided [SimpleMatchType].
///
/// This implementation makes use of prebuilt, serialized data for certain match types to enhance
/// performance by avoiding runtime construction of the matcher and replacement list. The function
/// expects that the relevant data has been compiled with the `prebuilt` feature.
///
/// # Parameters
///
/// - `smt_bit`: A variant of [SimpleMatchType] enumerating the various matching strategies.
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
/// This function requires the `prebuilt` feature to be enabled.
#[cfg(feature = "prebuilt")]
pub fn get_process_matcher(smt_bit: SimpleMatchType) -> Arc<(Vec<&'static str>, ProcessMatcher)> {
    {
        let process_matcher_cache = PROCESS_MATCHER_CACHE.read();

        if let Some(cached_result) = process_matcher_cache.get(&smt_bit) {
            return Arc::clone(cached_result);
        }
    }

    {
        let (process_replace_list, process_matcher) = match smt_bit {
            SimpleMatchType::None => {
                let empty_patterns: Vec<&str> = Vec::new();
                (
                    Vec::new(),
                    ProcessMatcher::Others(AhoCorasick::new(&empty_patterns).unwrap()),
                )
            }
            SimpleMatchType::Fanjian => (
                FANJIAN_PROCESS_REPLACE_LIST_STR.lines().collect(),
                // Guaranteed not failed
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                        FANJIAN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),
            SimpleMatchType::WordDelete => {
                let mut process_dict = AHashMap::default();
                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
                process_dict.retain(|&key, &mut value| key != value);
                let process_list = process_dict
                    .iter()
                    .map(|(&key, _)| key)
                    .collect::<Vec<&str>>();

                (
                    Vec::new(),
                    ProcessMatcher::Others(
                        AhoCorasickBuilder::new()
                            .kind(Some(AhoCorasickKind::DFA))
                            .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                            .build(&process_list)
                            .unwrap(),
                    ),
                )
            }
            SimpleMatchType::TextDelete => {
                #[cfg(feature = "dfa")]
                {
                    let mut process_dict = AHashMap::default();
                    process_dict.extend(TEXT_DELETE.trim().lines().map(|pair_str| (pair_str, "")));
                    process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
                    process_dict.retain(|&key, &mut value| key != value);
                    let process_list = process_dict
                        .iter()
                        .map(|(&key, _)| key)
                        .collect::<Vec<&str>>();

                    (
                        Vec::new(),
                        ProcessMatcher::Others(
                            AhoCorasickBuilder::new()
                                .kind(Some(AhoCorasickKind::DFA))
                                .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                                .build(&process_list)
                                .unwrap(),
                        ),
                    )
                }

                #[cfg(not(feature = "dfa"))]
                {
                    (
                        Vec::new(),
                        ProcessMatcher::LeftMost(unsafe {
                            CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                                TEXT_DELETE_PROCESS_MATCHER_BYTES,
                            )
                            .0
                        }),
                    )
                }
            }
            SimpleMatchType::Normalize => {
                #[cfg(feature = "dfa")]
                {
                    (
                        NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                        ProcessMatcher::Others(
                            AhoCorasickBuilder::new()
                                .kind(Some(AhoCorasickKind::DFA))
                                .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                                .build(NORMALIZE_PROCESS_LIST_STR.lines())
                                .unwrap(),
                        ),
                    )
                }

                #[cfg(not(feature = "dfa"))]
                {
                    (
                        NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                        ProcessMatcher::LeftMost(unsafe {
                            CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                                NORMALIZE_PROCESS_MATCHER_BYTES,
                            )
                            .0
                        }),
                    )
                }
            }
            SimpleMatchType::PinYin => (
                PINYIN_PROCESS_REPLACE_LIST_STR.lines().collect(),
                // Guaranteed not failed
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                        PINYIN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),

            SimpleMatchType::PinYinChar => (
                PINYINCHAR_PROCESS_REPLACE_LIST_STR.lines().collect(),
                // Guaranteed not failed
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                        PINYIN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),
            _ => unreachable!(),
        };

        let uncached_result = Arc::new((process_replace_list, process_matcher));
        let mut process_matcher_cache = PROCESS_MATCHER_CACHE.write();
        process_matcher_cache.insert(smt_bit, Arc::clone(&uncached_result));
        uncached_result
    }
}

/// Processes the input text according to the specified single-bit [SimpleMatchType].
///
/// This function takes a [SimpleMatchType] bit flag and transforms the input text based on the rules
/// associated with that flag. It accepts only a single bit of `simple_match_type` and returns a Result
/// containing the transformed text or an error.
///
/// # Arguments
///
/// * `smt_bit` - A single bit of [SimpleMatchType] defining a specific text transformation rule.
/// * `text` - A string slice representing the input text to be transformed.
///
/// # Returns
///
/// * [`Result<Cow<'_, str>, &'static str>`] - The function returns a `Cow` (Copy on Write) string containing
///   the processed text if the transformation is successful or an error message if more than one bit is set.
///
/// # Errors
///
/// This function will return an error if the `smt_bit` contains more than one active transformation bit.
///
/// # Detailed Processing:
///
/// 1. Checks if more than one bit is set in `smt_bit` and returns an error if true.
/// 2. Retrieves the cached matcher and replacement list for the given bit.
/// 3. Initializes the `result` as a borrowed version of the input `text`.
/// 4. Matches the transformation type and applies the corresponding matcher:
///     a. [SimpleMatchType::None] - Do nothing.
///     b. [SimpleMatchType::Fanjian] - Apply the matcher and replace all occurrences.
///     c. [SimpleMatchType::TextDelete] | [SimpleMatchType::WordDelete] - Apply the matcher and delete all occurrences.
///     d. Other types - Apply the matcher and replace all occurrences.
/// 5. Updates the `result` accordingly and returns it within an `Ok`.
#[inline(always)]
pub fn text_process(smt_bit: SimpleMatchType, text: &str) -> Result<Cow<'_, str>, &'static str> {
    if smt_bit.iter().count() > 1 {
        return Err("text_process function only accept one bit of simple_match_type");
    }

    let cached_result = get_process_matcher(smt_bit);
    let (process_replace_list, process_matcher) = cached_result.as_ref();
    let mut result = Cow::Borrowed(text);
    match (smt_bit, process_matcher) {
        (SimpleMatchType::None, _) => {}
        (SimpleMatchType::Fanjian, pm) => match pm.replace_all(text, process_replace_list) {
            (true, Cow::Owned(pt)) => {
                result = Cow::Owned(pt);
            }
            (false, _) => {}
            (_, _) => unreachable!(),
        },
        (SimpleMatchType::TextDelete | SimpleMatchType::WordDelete, pm) => {
            match pm.delete_all(text) {
                (true, Cow::Owned(pt)) => {
                    result = Cow::Owned(pt);
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            }
        }
        (_, pm) => match pm.replace_all(text, process_replace_list) {
            (true, Cow::Owned(pt)) => {
                result = Cow::Owned(pt);
            }
            (false, _) => {}
            (_, _) => unreachable!(),
        },
    };
    Ok(result)
}

/// Processes the input text to apply transformations specified by the SimpleMatchType.
///
/// This function iterates over the bits of a SimpleMatchType to apply various text transformations.
/// Depending on the transformation type (e.g., text replace, text delete, etc.), it processes the text
/// and stores the result in an array of [Cow] (Copy on Write) strings.
///
/// # Arguments
///
/// * `simple_match_type` - A [SimpleMatchType] bit flags that define specific text transformation rules.
/// * `text` - A string slice representing the input text to be transformed.
///
/// # Returns
///
/// * [`ArrayVec<\[Cow<'a, str>; 8\]>`] - A fixed-size vector containing the processed versions of the input text.
///
/// # Detailed Processing:
///
/// 1. Initialize an [ArrayVec] to hold up to 8 versions of the processed text.
/// 2. Push the original text into the vector as the first entry.
/// 3. Iterate over each bit in the `simple_match_type`:
///    a. Retrieve the cached matcher and replacement list for the current bit.
///    b. Borrow the last processed text from the vector using an unsafe operation.
///    c. Match the current transformation type and apply the corresponding matcher:
///         i.  [SimpleMatchType::None] - Do nothing.
///         iii. [SimpleMatchType::TextDelete] | [SimpleMatchType::WordDelete] - Apply the matcher and delete all occurrences.
///         iv. Other types - Apply the matcher and replace all occurrences.
///    d. Update the current text entry or append new entries to the vector depending on the transformation result.
/// 4. Return the populated [ArrayVec] containing all processed text variations.
#[inline(always)]
pub fn reduce_text_process<'a>(
    simple_match_type: SimpleMatchType,
    text: &'a str,
) -> ArrayVec<[Cow<'a, str>; 8]> {
    let mut processed_text_list: ArrayVec<[Cow<'a, str>; 8]> = ArrayVec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for smt_bit in simple_match_type.iter() {
        let cached_result = get_process_matcher(smt_bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();
        // Guaranteed not failed
        let tmp_processed_text = unsafe { processed_text_list.last_mut().unwrap_unchecked() };

        match (smt_bit, process_matcher) {
            (SimpleMatchType::None, _) => {}
            (SimpleMatchType::TextDelete | SimpleMatchType::WordDelete, pm) => {
                match pm.delete_all(tmp_processed_text.as_ref()) {
                    (true, Cow::Owned(pt)) => {
                        processed_text_list.push(Cow::Owned(pt));
                    }
                    (false, _) => {}
                    (_, _) => unreachable!(),
                }
            }
            (_, pm) => match pm.replace_all(tmp_processed_text.as_ref(), process_replace_list) {
                (true, Cow::Owned(pt)) => {
                    processed_text_list.push(Cow::Owned(pt));
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
        }
    }

    processed_text_list
}

/// Processes the input text to apply transformations specified by the SimpleMatchType.
///
/// This function iterates over the bits of a SimpleMatchType to apply various text transformations.
/// Depending on the transformation type (e.g., text replace, text delete, etc.), it processes the text
/// and stores the result in an array of [Cow] (Copy on Write) strings.
///
/// # Arguments
///
/// * `simple_match_type` - A [SimpleMatchType] bit flags that define specific text transformation rules.
/// * `text` - A string slice representing the input text to be transformed.
///
/// # Returns
///
/// * [`ArrayVec<\[Cow<'a, str>; 8\]>`] - A fixed-size vector containing the processed versions of the input text.
///
/// # Detailed Processing:
///
/// 1. Initialize an [ArrayVec] to hold up to 8 versions of the processed text.
/// 2. Push the original text into the vector as the first entry.
/// 3. Iterate over each bit in the `simple_match_type`:
///    a. Retrieve the cached matcher and replacement list for the current bit.
///    b. Borrow the last processed text from the vector using an unsafe operation.
///    c. Match the current transformation type and apply the corresponding matcher:
///         i.  [SimpleMatchType::None] - Do nothing.
///         ii. [SimpleMatchType::Fanjian] | [SimpleMatchType::Normalize] - Apply the matcher and replace all occurrences.
///         iii. [SimpleMatchType::TextDelete] | [SimpleMatchType::WordDelete] - Apply the matcher and delete all occurrences.
///         iv. Other types - Apply the matcher and replace all occurrences.
///    d. Update the current text entry or append new entries to the vector depending on the transformation result.
/// 4. Return the populated [ArrayVec] containing all processed text variations.
#[inline(always)]
pub fn reduce_text_process_emit<'a>(
    simple_match_type: SimpleMatchType,
    text: &'a str,
) -> ArrayVec<[Cow<'a, str>; 8]> {
    let mut processed_text_list: ArrayVec<[Cow<'a, str>; 8]> = ArrayVec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for smt_bit in simple_match_type.iter() {
        let cached_result = get_process_matcher(smt_bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();
        // Guaranteed not failed
        let tmp_processed_text = unsafe { processed_text_list.last_mut().unwrap_unchecked() };

        match (smt_bit, process_matcher) {
            (SimpleMatchType::None, _) => {}
            (SimpleMatchType::Fanjian | SimpleMatchType::Normalize, pm) => {
                match pm.replace_all(tmp_processed_text.as_ref(), process_replace_list) {
                    (true, Cow::Owned(pt)) => {
                        *tmp_processed_text = Cow::Owned(pt);
                    }
                    (false, _) => {}
                    (_, _) => unreachable!(),
                }
            }
            (SimpleMatchType::TextDelete | SimpleMatchType::WordDelete, pm) => {
                match pm.delete_all(tmp_processed_text.as_ref()) {
                    (true, Cow::Owned(pt)) => {
                        processed_text_list.push(Cow::Owned(pt));
                    }
                    (false, _) => {}
                    (_, _) => unreachable!(),
                }
            }
            (_, pm) => match pm.replace_all(tmp_processed_text.as_ref(), process_replace_list) {
                (true, Cow::Owned(pt)) => {
                    processed_text_list.push(Cow::Owned(pt));
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
        }
    }

    processed_text_list
}

/// A node representing a SimpleMatchType in a tree structure.
///
/// This struct is used to build a tree of [SimpleMatchType] transformations, where each node
/// corresponds to a particular bit (transformation type) and holds a list of [SimpleMatchType]
/// values, the index of the processed text, and the indices of its child nodes.
///
/// # Fields
///
/// * `smt_list` - An [ArrayVec] holding up to 8 [SimpleMatchType] values that this node represents.
/// * `smt_bit` - A [SimpleMatchType] value representing the bit for this node.
/// * `is_processed` - A boolean value that check the node is processed.
/// * `processed_text_index` - An index pointing to the processed text associated with this node.
/// * `children` - An [ArrayVec] holding up to 8 usize indices pointing to the child nodes in the tree.
///
/// # Example Usage
///
/// The [SimpleMatchTypeBitNode] is primarily used within a tree structure to efficiently manage
/// and retrieve the various text transformations specified by different [SimpleMatchType] bit flags.
/// It leverages [ArrayVec] for efficient, fixed-size storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SimpleMatchTypeBitNode {
    smt_list: ArrayVec<[SimpleMatchType; 8]>,
    smt_bit: SimpleMatchType,
    is_processed: bool,
    processed_text_index: usize,
    children: ArrayVec<[usize; 8]>,
}

///
/// Constructs a tree of [SimpleMatchTypeBitNode] instances based on the given list of [SimpleMatchType] transformations.
///
/// This function creates a hierarchy of [SimpleMatchTypeBitNode] nodes representing different transformation types
/// defined by the provided `smt_list`. Each node in the tree corresponds to a specific bit transformation and may have
/// child nodes representing subsequent transformations.
///
/// # Parameters
///
/// * `smt_list`: A slice of [SimpleMatchType] representing the match types to be processed and included in the tree.
///
/// # Returns
///
/// A [Vec] containing the constructed tree of [SimpleMatchTypeBitNode]'s, where each node represents a different bit
/// transformation as defined in the `smt_list`.
///
/// # Details
///
/// The function starts by initializing the root node of the tree with a [SimpleMatchType::None].
/// It then iterates through each [SimpleMatchType] in the input list and constructs the tree as follows:
///
/// 1. For each `simple_match_type`, set the starting node as the root node.
/// 2. Iterate over each bit in the `simple_match_type`.
///    - If a child node with the current bit already exists, move to that child node.
///    - If no such child node exists, create a new child node, update the current node's children, and move to the new node.
/// 3. Upon finding or creating a node for the current bit, append the `simple_match_type` to the `smt_list` of that node.
///
/// # Safety
///
/// This function does not use any unsafe code, ensuring type safety and memory correctness.
///
pub fn build_smt_tree(smt_list: &[SimpleMatchType]) -> Vec<SimpleMatchTypeBitNode> {
    let mut smt_tree = Vec::new();
    let root = SimpleMatchTypeBitNode {
        smt_list: ArrayVec::new(),
        smt_bit: SimpleMatchType::None,
        is_processed: true,
        processed_text_index: 0,
        children: ArrayVec::new(),
    };
    smt_tree.push(root);
    for &simple_match_type in smt_list.iter() {
        let mut current_node_index = 0;
        for smt_bit in simple_match_type.iter() {
            let current_node = smt_tree[current_node_index];
            if current_node.smt_bit == smt_bit {
                continue;
            }

            let mut is_found = false;
            for child_node_index in current_node.children {
                if smt_bit == smt_tree[child_node_index].smt_bit {
                    current_node_index = child_node_index;
                    is_found = true;
                    break;
                }
            }

            if !is_found {
                let mut child = SimpleMatchTypeBitNode {
                    smt_list: ArrayVec::new(),
                    smt_bit,
                    is_processed: false,
                    processed_text_index: 0,
                    children: ArrayVec::new(),
                };
                child.smt_list.push(simple_match_type);
                smt_tree.push(child);
                let new_node_index = smt_tree.len() - 1;
                smt_tree[current_node_index].children.push(new_node_index);
                current_node_index = new_node_index;
            } else {
                smt_tree[current_node_index]
                    .smt_list
                    .push(simple_match_type);
            }
        }
    }
    smt_tree
}

/// Reduces and processes the given text through a pre-built tree of [SimpleMatchTypeBitNode].
///
/// This function applies text transformations specified in a tree structure to the input text,
/// producing various transformed versions along with their associated match types.
/// Each node in the tree corresponds to a specific text transformation, and child nodes represent
/// subsequent transformations.
///
/// # Parameters
///
/// * `smt_tree`: A slice of [SimpleMatchTypeBitNode] representing a pre-built tree structure
///               containing text transformation rules.
/// * `text`: A string slice holding the initial text to be transformed.
///
/// # Returns
///
/// [`ArrayVec<\[(Cow<'a, str>, IdSet); 16\]>`]: A collection of tuples, where each tuple
/// contains a transformed version of the text and a set of [SimpleMatchType] transformations that
/// were applied to generate that version of the text.
///
/// # Details
///
/// The function begins by copying the input `smt_tree` into a mutable vector. It also initializes
/// an array to store the processed text and their associated match types. For each node in the tree,
/// the function processes the input text according to the transformation rules specified in the node.
///
/// 1. It retrieves the current node and its processed text index.
/// 2. It iterates through each child node of the current node.
///    - If the child node is already processed, it updates the current index with the processed text index of the current node.
///    - If the child node is not processed, it applies the transformation specified by the match type of the child node.
///      - Based on the match type, it either deletes or replaces parts of the text.
///      - The transformed text and its match types are then stored in an array.
///      - The processed text index is updated with the index of the newly transformed text.
/// 3. The child node is marked as processed, and its processed text index is updated.
/// 4. The array of transformed texts and their match types is returned at the end.
///
/// # Safety
///
/// This function uses `unsafe` blocks to access elements in the vectors and arrays directly,
/// assuming that all necessary bounds checks and precautions are performed implicitly. Care should be
/// taken when modifying this function to avoid introducing undefined behavior.
/// ```
#[inline(always)]
pub fn reduce_text_process_with_tree<'a>(
    smt_tree: &[SimpleMatchTypeBitNode],
    text: &'a str,
) -> ArrayVec<[(Cow<'a, str>, IdSet); 16]> {
    let mut smt_tree_copied: Vec<SimpleMatchTypeBitNode> = smt_tree.to_vec();

    let mut processed_text_smt_set: ArrayVec<[(Cow<'a, str>, IdSet); 16]> = ArrayVec::new();
    processed_text_smt_set.push((
        Cow::Borrowed(text),
        IdSet::from_iter([SimpleMatchType::None.bits() as usize]),
    ));

    for (current_node_index, current_node) in smt_tree.iter().enumerate() {
        let (left_tree, right_tree) =
            unsafe { smt_tree_copied.split_at_mut_unchecked(current_node_index.unchecked_add(1)) };

        let current_copied_node = unsafe { left_tree.get_unchecked(current_node_index) };
        let mut current_index = current_copied_node.processed_text_index;
        let current_text_ptr = unsafe { processed_text_smt_set.get_unchecked(current_index) }
            .0
            .as_ref() as *const str;

        for child_node_index in current_node.children {
            let child_node = unsafe {
                right_tree.get_unchecked_mut(
                    child_node_index
                        .unchecked_sub(current_node_index)
                        .unchecked_sub(1),
                )
            };

            if child_node.is_processed {
                current_index = current_copied_node.processed_text_index;
            } else {
                let cached_result = get_process_matcher(child_node.smt_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match child_node.smt_bit {
                    SimpleMatchType::None => {}
                    SimpleMatchType::TextDelete | SimpleMatchType::WordDelete => {
                        match process_matcher.delete_all(unsafe { &*current_text_ptr }) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_smt_set.push((
                                    Cow::Owned(pt),
                                    IdSet::from_iter(
                                        child_node.smt_list.iter().map(|smt| smt.bits() as usize),
                                    ),
                                ));
                                current_index =
                                    unsafe { processed_text_smt_set.len().unchecked_sub(1) };
                            }
                            (false, _) => {
                                current_index = current_copied_node.processed_text_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                    _ => match process_matcher
                        .replace_all(unsafe { &*current_text_ptr }, process_replace_list)
                    {
                        (true, Cow::Owned(pt)) => {
                            processed_text_smt_set.push((Cow::Owned(pt), IdSet::new()));
                            current_index =
                                unsafe { processed_text_smt_set.len().unchecked_sub(1) };
                        }
                        (false, _) => {
                            current_index = current_copied_node.processed_text_index;
                        }
                        (_, _) => unreachable!(),
                    },
                }
                child_node.is_processed = true;
            }

            child_node.processed_text_index = current_index;
            let processed_text_smt_tuple =
                unsafe { processed_text_smt_set.get_unchecked_mut(current_index) };
            processed_text_smt_tuple
                .1
                .extend(child_node.smt_list.iter().map(|smt| smt.bits() as usize));
        }
    }

    processed_text_smt_set
}

/// Processes the given text through a list of specified [SimpleMatchType] transformations.
///
/// This function builds a tree structure from the list of [SimpleMatchType] transformations
/// and uses it to apply text transformations. The resulting texts and their associated
/// match types are collected into an [ArrayVec].
///
/// # Parameters
///
/// * `smt_list`: A slice of [SimpleMatchType] enums representing the match types and their associated
///              transformations to be applied to the text.
/// * `text`: A string slice holding the initial text to be transformed.
///
/// # Returns
///
/// [`ArrayVec<\[(Cow<'a, str>, IdSet); 16\]>`]: A collection of tuples, where each tuple
/// contains a transformed version of the text and a set of [SimpleMatchType] transformations that
/// were applied to generate that version of the text.
///
/// # Safety
///
/// This function employs `unsafe` code to efficiently access and manipulate internal data structures.
/// Care should be taken when modifying this function to avoid introducing undefined behavior.
#[inline(always)]
pub fn reduce_text_process_with_list<'a>(
    smt_list: &[SimpleMatchType],
    text: &'a str,
) -> ArrayVec<[(Cow<'a, str>, IdSet); 16]> {
    let mut smt_tree = Vec::with_capacity(8);
    let mut root = SimpleMatchTypeBitNode {
        smt_list: ArrayVec::new(),
        smt_bit: SimpleMatchType::None,
        is_processed: true,
        processed_text_index: 0,
        children: ArrayVec::new(),
    };
    root.smt_list.push(SimpleMatchType::None);
    smt_tree.push(root);

    let mut processed_text_smt_set: ArrayVec<[(Cow<'a, str>, IdSet); 16]> = ArrayVec::new();
    processed_text_smt_set.push((
        Cow::Borrowed(text),
        IdSet::from_iter([SimpleMatchType::None.bits() as usize]),
    ));

    for &simple_match_type in smt_list.iter() {
        let mut current_text = text;
        let mut current_index = 0;
        let mut current_node_index = 0;

        for smt_bit in simple_match_type.iter() {
            let current_node = unsafe { smt_tree.get_unchecked(current_node_index) };
            if current_node.smt_bit == smt_bit {
                continue;
            }

            let mut is_found = false;
            for child_node_index in current_node.children {
                if smt_bit == unsafe { smt_tree.get_unchecked(child_node_index) }.smt_bit {
                    current_node_index = child_node_index;
                    is_found = true;
                    break;
                }
            }
            let current_node = unsafe { smt_tree.get_unchecked_mut(current_node_index) };

            if !is_found {
                let cached_result = get_process_matcher(smt_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match smt_bit {
                    SimpleMatchType::None => {}
                    SimpleMatchType::TextDelete | SimpleMatchType::WordDelete => {
                        match process_matcher.delete_all(current_text) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_smt_set.push((Cow::Owned(pt), IdSet::new()));
                                current_index = processed_text_smt_set.len() - 1;

                                let processed_text_smt_tuple = unsafe {
                                    processed_text_smt_set
                                        .get_unchecked_mut(current_node.processed_text_index)
                                };
                                processed_text_smt_tuple
                                    .1
                                    .insert(simple_match_type.bits() as usize);
                            }
                            (false, _) => {
                                current_index = current_node.processed_text_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                    _ => match process_matcher.replace_all(current_text, process_replace_list) {
                        (true, Cow::Owned(pt)) => {
                            processed_text_smt_set.push((Cow::Owned(pt), IdSet::new()));
                            current_index = processed_text_smt_set.len() - 1;
                        }
                        (false, _) => {
                            current_index = current_node.processed_text_index;
                        }
                        (_, _) => unreachable!(),
                    },
                }

                let mut child = SimpleMatchTypeBitNode {
                    smt_list: ArrayVec::new(),
                    smt_bit,
                    is_processed: true,
                    processed_text_index: current_index,
                    children: ArrayVec::new(),
                };
                child.smt_list.push(simple_match_type);
                smt_tree.push(child);

                let new_node_index = smt_tree.len() - 1;
                let current_node = unsafe { smt_tree.get_unchecked_mut(current_node_index) };
                current_node.children.push(new_node_index);
                current_node_index = new_node_index;
            } else {
                current_index = current_node.processed_text_index;
                current_node.smt_list.push(simple_match_type);
            }

            let processed_text_smt_tuple =
                unsafe { processed_text_smt_set.get_unchecked_mut(current_index) };
            processed_text_smt_tuple
                .1
                .insert(simple_match_type.bits() as usize);
            current_text = unsafe { processed_text_smt_set.get_unchecked(current_index) }
                .0
                .as_ref();
        }
    }

    processed_text_smt_set
}

#[cfg(test)]
mod test_text_process {
    use super::*;

    #[test]
    fn test_text_process() {
        let text = text_process(SimpleMatchType::Fanjian, "躶軆");
        println!("{:?}", text);
    }

    #[test]
    fn test_reduce_text_process() {
        let text = reduce_text_process(SimpleMatchType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");
        println!("{:?}", text);
    }

    #[test]
    fn test_reduce_text_process_emit() {
        let text =
            reduce_text_process_emit(SimpleMatchType::FanjianDeleteNormalize, "~ᗩ~躶~𝚩~軆~Ⲉ~");
        println!("{:?}", text);
    }

    #[test]
    fn test_build_smt_tree() {
        let smt_list = vec![
            SimpleMatchType::Fanjian,
            SimpleMatchType::DeleteNormalize - SimpleMatchType::WordDelete,
            SimpleMatchType::FanjianDeleteNormalize - SimpleMatchType::WordDelete,
            SimpleMatchType::TextDelete,
            SimpleMatchType::Normalize,
        ];
        let smt_tree = build_smt_tree(&smt_list);
        println!("{:?}", smt_tree);
    }

    #[test]
    fn test_reduce_text_process_with_tree() {
        let smt_list = vec![
            SimpleMatchType::Fanjian,
            SimpleMatchType::DeleteNormalize - SimpleMatchType::WordDelete,
            SimpleMatchType::FanjianDeleteNormalize - SimpleMatchType::WordDelete,
            SimpleMatchType::TextDelete,
            SimpleMatchType::Normalize,
        ];
        let smt_tree = build_smt_tree(&smt_list);
        let text = "test爽-︻";

        let processed_text_smt_set = reduce_text_process_with_tree(&smt_tree, text);
        println!("{processed_text_smt_set:?}");
    }

    #[test]
    fn test_reduce_text_process_with_list() {
        let smt_list = vec![
            SimpleMatchType::Fanjian,
            SimpleMatchType::DeleteNormalize - SimpleMatchType::WordDelete,
            SimpleMatchType::FanjianDeleteNormalize - SimpleMatchType::WordDelete,
            SimpleMatchType::TextDelete,
            SimpleMatchType::Normalize,
        ];
        let text = "test爽-︻";

        let processed_text_smt_set = reduce_text_process_with_list(&smt_list, text);
        println!("{processed_text_smt_set:?}");
    }
}
