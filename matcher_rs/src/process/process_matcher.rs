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
use lazy_static::lazy_static;
use nohash_hasher::{IntMap, IntSet};
use parking_lot::RwLock;
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
/// - [Chinese](ProcessMatcher::Chinese): Utilizes a [`CharwiseDoubleArrayAhoCorasick<u32>`] matcher optimized for Chinese characters.
/// - [Others](ProcessMatcher::Others): Uses an [AhoCorasick] matcher for all other types of text.
///
/// By distinguishing between these two categories, [ProcessMatcher] allows for more efficient and accurate pattern
/// matching tailored to the linguistic properties of the text being processed.
#[derive(Clone)]
pub enum ProcessMatcher {
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
/// - `simple_match_type_bit`: A variant of [SimpleMatchType] which specifies the type of matching operation to be performed.
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
/// The function may use either the `Chinese` or `Others` variant of the [ProcessMatcher], depending on the [`SimpleMatchType`].
#[cfg(feature = "runtime_build")]
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
                        pair_str_split.next().unwrap().trim_matches('␀'),
                    )
                }));
            }
            _ => {}
        }

        process_dict.retain(|&key, &mut value| key != value);

        let (process_replace_list, process_matcher) = match simple_match_type_bit {
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
        process_matcher_cache.insert(simple_match_type_bit, Arc::clone(&uncached_result));
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
/// This function requires the `prebuilt` feature to be enabled.
#[cfg(feature = "prebuilt")]
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
            SimpleMatchType::Normalize => (
                NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                ProcessMatcher::Others(
                    AhoCorasickBuilder::new()
                        .kind(Some(AhoCorasickKind::DFA))
                        .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                        .build(NORMALIZE_PROCESS_LIST_STR.lines())
                        .unwrap(),
                ),
            ),
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
        process_matcher_cache.insert(simple_match_type_bit, Arc::clone(&uncached_result));
        uncached_result
    }
}

/// Processes the input text according to the specified single-bit `SimpleMatchType`.
///
/// This function takes a `SimpleMatchType` bit flag and transforms the input text based on the rules
/// associated with that flag. It accepts only a single bit of `simple_match_type` and returns a Result
/// containing the transformed text or an error.
///
/// # Arguments
///
/// * `simple_match_type_bit` - A single bit of [SimpleMatchType] defining a specific text transformation rule.
/// * `text` - A string slice representing the input text to be transformed.
///
/// # Returns
///
/// * [`Result<Cow<'_, str>, &'static str>`] - The function returns a `Cow` (Copy on Write) string containing
///   the processed text if the transformation is successful or an error message if more than one bit is set.
///
/// # Errors
///
/// This function will return an error if the `simple_match_type_bit` contains more than one active transformation bit.
///
/// # Detailed Processing:
///
/// 1. Checks if more than one bit is set in `simple_match_type_bit` and returns an error if true.
/// 2. Retrieves the cached matcher and replacement list for the given bit.
/// 3. Initializes the `result` as a borrowed version of the input `text`.
/// 4. Matches the transformation type and applies the corresponding matcher:
///     a. [SimpleMatchType::None] - Do nothing.
///     b. [SimpleMatchType::Fanjian] - Apply the matcher and replace all occurrences.
///     c. [SimpleMatchType::TextDelete] | [SimpleMatchType::WordDelete] - Apply the matcher and delete all occurrences.
///     d. Other types - Apply the matcher and replace all occurrences.
/// 5. Updates the `result` accordingly and returns it within an `Ok`.
#[inline(always)]
pub fn text_process(
    simple_match_type_bit: SimpleMatchType,
    text: &str,
) -> Result<Cow<'_, str>, &'static str> {
    if simple_match_type_bit.iter().count() > 1 {
        return Err("text_process function only accept one bit of simple_match_type");
    }

    let cached_result = get_process_matcher(simple_match_type_bit);
    let (process_replace_list, process_matcher) = cached_result.as_ref();
    let mut result = Cow::Borrowed(text);
    match (simple_match_type_bit, process_matcher) {
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

    for simple_match_type_bit in simple_match_type.iter() {
        let cached_result = get_process_matcher(simple_match_type_bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();
        // Guaranteed not failed
        let tmp_processed_text = unsafe { processed_text_list.last_mut().unwrap_unchecked() };

        match (simple_match_type_bit, process_matcher) {
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

    for simple_match_type_bit in simple_match_type.iter() {
        let cached_result = get_process_matcher(simple_match_type_bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();
        // Guaranteed not failed
        let tmp_processed_text = unsafe { processed_text_list.last_mut().unwrap_unchecked() };

        match (simple_match_type_bit, process_matcher) {
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

/// A node representing a bit in the [SimpleMatchType] and its associated processed text index.
///
/// `SimpleMatchTypeBitNode` is used to create a hierarchy of match type bits,
/// allowing each node to have children corresponding to subsequent match type bits.
///
/// # Fields
///
/// * `simple_match_type_bit` - A bit from the [SimpleMatchType] that defines the specific text transformation rule.
/// * `processed_text_index` - An index referring to the position of the processed text in a list.
/// * `children` - An [ArrayVec] containing indices of children `SimpleMatchTypeBitNode`, up to a maximum of 8.
///
/// This structure is pivotal in organizing the transformation pipeline for text processing, where each bit in
/// the [SimpleMatchType] can lead to subsequent transformations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SimpleMatchTypeBitNode {
    simple_match_type_bit: SimpleMatchType,
    processed_text_index: usize,
    children: ArrayVec<[usize; 8]>,
}

type SimpleMatchTypeIndexSetMap = IntMap<SimpleMatchType, IntSet<usize>>;

// pub fn build_simple_match_type_bit_node_list(
//     simple_match_type_list: &[SimpleMatchType],
// ) -> Vec<SimpleMatchTypeBitNode> {
//     let mut simple_match_type_bit_node_list = Vec::new();
//     simple_match_type_bit_node_list.push(SimpleMatchTypeBitNode {
//         simple_match_type_bit: SimpleMatchType::None,
//         processed_text_index: 0,
//         children: ArrayVec::new(),
//     });
//     for simple_match_type in simple_match_type_list.iter() {
//         let mut current_node_index = 0;
//         for simple_match_type_bit in simple_match_type.iter() {
//             let mut is_found = false;
//             let current_node = simple_match_type_bit_node_list[current_node_index];
//             for child_node_index in current_node.children {
//                 if simple_match_type_bit
//                     == simple_match_type_bit_node_list[child_node_index].simple_match_type_bit
//                 {
//                     current_node_index = child_node_index;
//                     is_found = true;
//                     break;
//                 }
//             }

//             if !is_found {
//                 simple_match_type_bit_node_list.push(SimpleMatchTypeBitNode {
//                     simple_match_type_bit,
//                     processed_text_index: 0,
//                     children: ArrayVec::new(),
//                 });
//                 let new_node_index = simple_match_type_bit_node_list.len() - 1;
//                 simple_match_type_bit_node_list[current_node_index]
//                     .children
//                     .push(new_node_index);
//                 current_node_index = new_node_index;
//             }
//         }
//     }
//     simple_match_type_bit_node_list
// }

/// Reduces the text processing pipeline and maps each [SimpleMatchType] to the indices
/// of its associated processed texts.
///
/// This function processes each [SimpleMatchType] in the given list, applies text
/// transformations according to the match type bits, and maintains the hierarchy of
/// transformations in a trie-like structure of nodes. It outputs a map of match types
/// to sets of processed text indices, and the list of all processed texts.
///
/// # Parameters
///
/// * `simple_match_type_list`: A slice of [SimpleMatchType] representing the match types
///   to be processed.
/// * `text`: A string slice holding the initial text to be transformed.
///
/// # Returns
///
/// A tuple containing:
/// * [`IntMap<SimpleMatchType, IntSet<usize>>`]: A map of [SimpleMatchType] to the set of
///   indices in the processed text list where the transformation results for that type
///   can be found.
/// * [`ArrayVec<\[Cow<'a, str>; 8\]>`]: A list of processed texts corresponding to the applied
///   transformations.
///
/// # Safety
///
/// This function makes use of some unsafe code to access and manipulate internal data
/// structures efficiently. Care should be taken when modifying this function to avoid
/// introducing undefined behavior.
#[inline(always)]
pub fn reduce_text_process_emit_with_list<'a>(
    simple_match_type_list: &[SimpleMatchType],
    text: &'a str,
) -> (
    SimpleMatchTypeIndexSetMap,
    ArrayVec<[Cow<'a, str>; 8]>,
) {

    let mut simple_match_type_bit_node_list = Vec::new();
    simple_match_type_bit_node_list.push(SimpleMatchTypeBitNode {
        simple_match_type_bit: SimpleMatchType::None,
        processed_text_index: 0,
        children: ArrayVec::new(),
    });

    let mut simple_match_type_index_list_map = IntMap::with_capacity(8);
    let mut processed_text_list: ArrayVec<[Cow<'a, str>; 8]> = ArrayVec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for simple_match_type in simple_match_type_list.iter() {
        let mut current_text = text;
        let mut current_index = 0;
        let mut current_node_index = 0;

        for simple_match_type_bit in simple_match_type.iter() {
            let mut is_found = false;
            let current_node =
                unsafe { simple_match_type_bit_node_list.get_unchecked(current_node_index) };
            for child_node_index in current_node.children {
                if simple_match_type_bit
                    == unsafe { simple_match_type_bit_node_list.get_unchecked(child_node_index) }
                        .simple_match_type_bit
                {
                    current_node_index = child_node_index;
                    is_found = true;
                    break;
                }
            }

            if !is_found {
                let cached_result = get_process_matcher(simple_match_type_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match (simple_match_type_bit, process_matcher) {
                    (SimpleMatchType::None, _) => {}
                    (SimpleMatchType::TextDelete | SimpleMatchType::WordDelete, pm) => {
                        match pm.delete_all(current_text.as_ref()) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_list.push(Cow::Owned(pt));
                                current_index = processed_text_list.len() - 1;
                            }
                            (false, _) => {
                                current_index = unsafe {
                                    simple_match_type_bit_node_list
                                        .get_unchecked(current_node_index)
                                }
                                .processed_text_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                    (_, pm) => match pm.replace_all(current_text.as_ref(), process_replace_list) {
                        (true, Cow::Owned(pt)) => {
                            processed_text_list.push(Cow::Owned(pt));
                            current_index = processed_text_list.len() - 1;
                        }
                        (false, _) => {
                            current_index = unsafe {
                                simple_match_type_bit_node_list.get_unchecked(current_node_index)
                            }
                            .processed_text_index;
                        }
                        (_, _) => unreachable!(),
                    },
                }

                if simple_match_type_bit != SimpleMatchType::None {
                    simple_match_type_bit_node_list.push(SimpleMatchTypeBitNode {
                        simple_match_type_bit,
                        processed_text_index: current_index,
                        children: ArrayVec::new(),
                    });
                    let new_node_index = simple_match_type_bit_node_list.len() - 1;
                    let current_node = unsafe {
                        simple_match_type_bit_node_list.get_unchecked_mut(current_node_index)
                    };
                    current_node.children.push(new_node_index);
                    current_node_index = new_node_index;
                }
            } else {
                current_index =
                    unsafe { simple_match_type_bit_node_list.get_unchecked(current_node_index) }
                        .processed_text_index;
            }

            let index_list = simple_match_type_index_list_map
                .entry(*simple_match_type)
                .or_insert(IntSet::default());
            index_list.insert(
                unsafe { simple_match_type_bit_node_list.get_unchecked(current_node_index) }
                    .processed_text_index,
            );

            current_text = unsafe { processed_text_list.get_unchecked(current_index) }.as_ref();
        }
    }

    (simple_match_type_index_list_map, processed_text_list)
}

#[cfg(test)]
mod test_text_process {
    use super::*;

    #[test]
    fn test_reduce_text_process_emit_with_list() {
        let simple_match_type_list = vec![
            SimpleMatchType::Fanjian | SimpleMatchType::TextDelete,
            SimpleMatchType::Fanjian,
            SimpleMatchType::Normalize,
            SimpleMatchType::Fanjian | SimpleMatchType::Normalize,
            SimpleMatchType::TextDelete,
            SimpleMatchType::TextDelete | SimpleMatchType::Normalize,
        ];
        let text = "test爽-︻";

        let (simple_match_type_index_list_map, processed_text_list) =
            reduce_text_process_emit_with_list(&simple_match_type_list, text);
        println!(
            "{:?}, {:?}",
            simple_match_type_index_list_map, processed_text_list
        );
    }
}
