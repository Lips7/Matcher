use std::borrow::Cow;

use ahash::AHashMap;
use aho_corasick::{
    AhoCorasick, AhoCorasickBuilder, AhoCorasickKind::DFA, MatchKind as AhoCorasickMatchKind,
};
#[allow(unused_imports)]
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
};

use crate::process::constants::*;
use crate::SimpleMatchType;

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
    /// corresponding string from this list.
    ///
    /// # Returns
    ///
    /// * `(bool, Cow<'a, str>)`: A tuple where the first element is a boolean indicating whether any replacements were made,
    /// and the second element is a [Cow] string containing the modified text. If no replacements were made, the original text
    /// is returned as a [Cow::Borrowed].
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
    /// let (process_replace_list, matcher) = get_process_matcher(SimpleMatchType::TextDelete); // Assume this returns a valid ProcessMatcher
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
                    result.push_str(unsafe { &text.get_unchecked(last_end..mat.start()) });
                    result.push_str(unsafe {
                        process_replace_list.get_unchecked(mat.value() as usize)
                    });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Others(ac) => {
                for mat in ac.find_iter(text) {
                    result.push_str(unsafe { &text.get_unchecked(last_end..mat.start()) });
                    result.push_str(unsafe {
                        process_replace_list.get_unchecked(mat.pattern().as_usize())
                    });
                    last_end = mat.end();
                }
            }
        }

        if last_end > 0 {
            result.push_str(unsafe { &text.get_unchecked(last_end..) });
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
    /// and the second element is a [Cow] string containing the modified text. If no deletions were made, the original text
    /// is returned as a [Cow::Borrowed].
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
    /// let (process_replace_list, matcher) = get_process_matcher(SimpleMatchType::TextDelete); // Assume this returns a valid ProcessMatcher
    /// let text = "Some text for processing";
    /// let (deleted, result) = matcher.delete_all(text);
    /// ```
    pub fn delete_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
        let mut result = String::with_capacity(text.len());
        let mut last_end = 0;
        match self {
            ProcessMatcher::Chinese(ac) => {
                for mat in ac.find_iter(text) {
                    result.push_str(unsafe { &text.get_unchecked(last_end..mat.start()) });
                    last_end = mat.end();
                }
            }
            ProcessMatcher::Others(ac) => {
                for mat in ac.find_iter(text) {
                    result.push_str(unsafe { &text.get_unchecked(last_end..mat.start()) });
                    last_end = mat.end();
                }
            }
        }

        if last_end > 0 {
            result.push_str(unsafe { &text.get_unchecked(last_end..) });
            (true, Cow::Owned(result))
        } else {
            (false, Cow::Borrowed(text))
        }
    }
}

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
/// let (process_replace_list, matcher) = get_process_matcher(SimpleMatchType::TextDelete);
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
) -> (Vec<&'static str>, ProcessMatcher) {
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

    process_dict.retain(|&key, &mut value| (key == "#" || !key.starts_with('#')) && key != value);
    let process_replace_list = process_dict.iter().map(|(_, &val)| val).collect();

    match simple_match_type_bit {
        // SimpleMatchType::Fanjian | SimpleMatchType::PinYin | SimpleMatchType::PinYinChar => {
        //     let process_matcher = CharwiseDoubleArrayAhoCorasickBuilder::new()
        //         .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
        //         .build(
        //             process_dict
        //                 .iter()
        //                 .map(|(&key, _)| key)
        //                 .collect::<Vec<&str>>(),
        //         )
        //         .unwrap();
        //     (
        //         process_replace_list,
        //         ProcessMatcher::Chinese(process_matcher),
        //     )
        // }
        _ => {
            let process_matcher = AhoCorasickBuilder::new()
                .kind(Some(DFA))
                .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                .build(
                    process_dict
                        .iter()
                        .map(|(&key, _)| key)
                        .collect::<Vec<&str>>(),
                )
                .unwrap();
            (
                process_replace_list,
                ProcessMatcher::Others(process_matcher),
            )
        }
    }
}
