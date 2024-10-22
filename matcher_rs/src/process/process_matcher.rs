use std::borrow::Cow;
use std::fmt::Display;
use std::sync::Arc;

use aho_corasick_unsafe::AhoCorasick;
#[cfg(any(feature = "runtime_build", feature = "dfa"))]
use aho_corasick_unsafe::{AhoCorasickBuilder, AhoCorasickKind, MatchKind as AhoCorasickMatchKind};
use bitflags::bitflags;
#[cfg(not(feature = "runtime_build"))]
use daachorse::CharwiseDoubleArrayAhoCorasick;
#[cfg(feature = "runtime_build")]
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
};
use id_set::IdSet;
use lazy_static::lazy_static;
use micromap::Map;
use nohash_hasher::IsEnabled;
use parking_lot::RwLock;
#[cfg(any(feature = "runtime_build", feature = "dfa"))]
use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tinyvec::ArrayVec;

use crate::process::constants::*;

bitflags! {
    /// Represents different types of processes that can be applied.
    ///
    /// This structure uses bitflags to allow combining multiple process types
    /// using bitwise operations.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::ProcessType;
    ///
    /// let process = ProcessType::Fanjian | ProcessType::Delete;
    /// if process.contains(ProcessType::Fanjian) {
    ///     println!("Fanjian process is included.");
    /// }
    /// ```
    #[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Default)]
    pub struct ProcessType: u8 {
        /// No processing action.
        const None = 0b00000001;

        /// Processing involving Fanjian (traditional Chinese to simplified Chinese conversion).
        const Fanjian = 0b00000010;

        /// Processing that involves deleting specific elements or characters.
        const Delete = 0b00000100;

        /// Processing that normalizes the input (possibly dealing with character encodings, formats, etc.).
        const Normalize = 0b00001000;

        /// Combined processing of deleting and normalizing the input.
        const DeleteNormalize = 0b00001100;

        /// Combined processing involving Fanjian conversion, deleting specific elements, and normalizing the input.
        const FanjianDeleteNormalize = 0b00001110;

        /// Processing that converts the input into Pinyin with boundaries.
        const PinYin = 0b00010000;

        /// Processing that converts the input into Pinyin without boundaries.
        const PinYinChar = 0b00100000;
    }
}

impl Serialize for ProcessType {
    /// Serializes a [ProcessType] instance into its bit representation using the provided serializer.
    ///
    /// This implementation leverages the [Serialize] trait from Serde to convert the [ProcessType]
    /// bitflag into a serializable form. The `bits()` method extracts the raw bit value of the
    /// [ProcessType], which is then passed to the serializer.
    ///
    /// # Arguments
    ///
    /// * [Serializer] - An instance of the [Serializer] trait that will handle the actual serialization.
    ///
    /// # Returns
    ///
    /// This method returns a result containing either the serialized value ([Serializer::Ok]) or an error ([Serializer::Error]).
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcessType {
    /// Deserializes a [ProcessType] instance from its bit representation using the provided deserializer.
    ///
    /// This implementation leverages the [Deserialize] trait from Serde to convert a bitflag
    /// representation back into a [ProcessType] instance. The `from_bits_retain` method is used
    /// to reconstruct the [ProcessType] from the deserialized bit value.
    ///
    /// # Arguments
    ///
    /// * [Deserializer] - An instance of the [Deserializer] trait that will handle the actual deserialization.
    ///
    /// # Returns
    ///
    /// This method returns a [Result] containing either the deserialized [ProcessType] instance
    /// (Ok([ProcessType])) or an error ([Deserializer::Error]).
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bits: u8 = u8::deserialize(deserializer)?;
        Ok(ProcessType::from_bits_retain(bits))
    }
}

impl Display for ProcessType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let display_str_list = self
            .iter_names()
            .map(|(name, _)| name.to_lowercase())
            .collect::<Vec<_>>();
        write!(f, "{:?}", display_str_list.join("_"))
    }
}

/// Implements the [IsEnabled] trait for the [ProcessType] struct.
///
/// This trait allows for [ProcessType] to be used in [Map].
impl IsEnabled for ProcessType {}

type ProcessMatcherCache = RwLock<Map<ProcessType, Arc<(Vec<&'static str>, ProcessMatcher)>, 8>>;

lazy_static! {
    /// A global, lazily-initialized cache for storing process matchers.
    ///
    /// This cache is implemented using a read-write lock ([RwLock]) around an [Map] that maps
    /// [ProcessType] keys to [Arc] instances holding tuples of a [Vec] of string slices and `ProcessMatcher`
    /// instances. This allows for efficient shared access to commonly used process matchers without incurring
    /// the overhead of creating new matcher instances.
    ///
    /// The cache is initialized with a capacity of 8 entries. The `lazy_static!` macro ensures that the
    /// cache is created and initialized only when it is first accessed.
    ///
    /// # Note
    ///
    /// The [PROCESS_MATCHER_CACHE] is intended to be used in scenarios where process matchers are frequently
    /// reused across different parts of an application. Storing matchers in the cache can significantly improve
    /// performance by avoiding redundant computations and allocations.
    pub static ref PROCESS_MATCHER_CACHE: ProcessMatcherCache =
        RwLock::new(Map::default());
}

/// Represents different types of process matchers used for text processing.
///
/// This enum contains variants for different kinds of matchers that can operate on text to find and
/// replace or delete specific patterns. Each variant is designed to handle specific use cases
/// effectively. The enum is clonable, allowing for easy duplication when necessary.
///
/// # Variants
///
/// - `LeftMost`: Uses a [`CharwiseDoubleArrayAhoCorasick<u32>`] matcher to find the leftmost non-overlapping matches
///   in the text. This variant is only available when the "dfa" feature is not enabled.
///
/// - `Chinese`: Uses a [`CharwiseDoubleArrayAhoCorasick<u32>`] matcher specifically tailored to handle Chinese text,
///   focusing on character-wise matching to find the patterns.
///
/// - `Others`: Uses a standard [AhoCorasick] matcher for general-purpose text processing. This is suitable for
///   finding matches for patterns not covered by the other two variants.
///
/// Each variant encapsulates a matcher implementation that is optimized for its specific use case,
/// allowing for efficient text processing operations such as finding, replacing, or deleting patterns
/// within the text.
#[derive(Clone)]
pub enum ProcessMatcher {
    #[cfg(not(feature = "dfa"))]
    LeftMost(CharwiseDoubleArrayAhoCorasick<u32>),
    Chinese(CharwiseDoubleArrayAhoCorasick<u32>),
    Others(AhoCorasick),
}

impl ProcessMatcher {
    /// Replaces all matched patterns in the provided text with the corresponding replacement strings
    /// from the `process_replace_list`.
    ///
    /// This method iterates through the text using the `ProcessMatcher` variant to find all matches,
    /// and replaces the occurrences with the respective strings from the `process_replace_list`.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice that contains the text to be processed.
    /// * `process_replace_list` - A slice of string slices containing the replacement strings. Each match
    ///   found in the text will be replaced by the respective string from this list.
    ///
    /// # Returns
    ///
    /// This method returns a tuple:
    /// * `bool` - A boolean indicating whether any replacements were made (`true` if replacements were made, `false` otherwise).
    /// * [Cow<'a, str>] - A copy-on-write string containing the processed text. If no replacements were made,
    ///   a borrowed version of the original text is returned. Otherwise, an owned version of the text with
    ///   the replacements is returned.
    ///
    /// # Safety
    ///
    /// This method uses `unsafe` blocks to perform unchecked slicing of the text and to access elements
    /// in the `process_replace_list`. These operations are guaranteed not to fail based on the matchers' behavior.
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

    /// Deletes all matched patterns in the provided text.
    ///
    /// This function iterates over the text and uses the appropriate `ProcessMatcher` variant to locate all matches.
    /// It then deletes the occurrences of these patterns, holding the remaining text fragments together.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice that contains the text to be processed.
    ///
    /// # Returns
    ///
    /// This function returns a tuple:
    /// * `bool` - A boolean indicating whether any deletions were made (`true` if deletions were made, `false` otherwise).
    /// * [Cow<'a, str>] - A copy-on-write string containing the processed text. If no deletions were made,
    ///   a borrowed version of the original text is returned. Otherwise, an owned version of the text with
    ///   the deletions is returned.
    ///
    /// # Safety
    ///
    /// This function uses `unsafe` blocks to perform unchecked slicing of the text. These operations are guaranteed
    /// not to fail based on the matcher's behavior.
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

/// Retrieves or constructs a `ProcessMatcher` for a given [ProcessType].
///
/// This function looks up a cached `ProcessMatcher` for the provided `process_type_bit`.
/// If a cached entry exists, it returns a cloned reference to the cached value. If not,
/// it constructs a new matcher based on the [ProcessType], caches it, and returns the
/// new matcher. The function distinguishes between compile-time and runtime build options
/// to decide how to construct the matcher.
///
/// # Parameters
/// - `process_type_bit`: The [ProcessType] for which a matcher is to be retrieved or constructed.
///
/// # Returns
/// - An [Arc] containing a tuple of a vector of replacement strings and a `ProcessMatcher`.
///
/// # Important
/// - For the [ProcessType::Fanjian], [ProcessType::Delete], [ProcessType::Normalize],
///   [ProcessType::PinYin], and [ProcessType::PinYinChar] variants, the function prepares
///   a dictionary for character replacements or deletions.
/// - The function makes use of the [AhoCorasick] and [CharwiseDoubleArrayAhoCorasick]
///   for efficient text processing.
///
/// # Caching
/// - This function employs a read-write lock for the cache to ensure thread safety.
///   If the matcher isn't already cached, it creates the matcher, adds it to the cache,
///   and then returns it.
///
/// # Configuration
/// - By setting the `runtime_build` feature flag, the function creates matchers at runtime.
/// - The `dfa` feature flag determines whether to use Deterministic Finite Automaton (DFA)
///   based [AhoCorasick] matcher.
///
/// # Safety
/// - This function utilizes `unsafe` blocks for deserializing predefined binary patterns
///   into [CharwiseDoubleArrayAhoCorasick], ensuring it's guaranteed safe as assumed by the context.
///
/// # Panics
/// - The function will panic if the `process_type_bit` is any variant not handled in the match arms.
///
/// # Examples
/// ```
/// use matcher_rs::{ProcessType, get_process_matcher};
///
/// let process_type = ProcessType::Fanjian;
/// let process_matcher = get_process_matcher(process_type);
/// // Use `process_matcher` for text processing
/// ```
pub fn get_process_matcher(
    process_type_bit: ProcessType,
) -> Arc<(Vec<&'static str>, ProcessMatcher)> {
    {
        let process_matcher_cache = PROCESS_MATCHER_CACHE.read();

        if let Some(cached_result) = process_matcher_cache.get(&process_type_bit) {
            return Arc::clone(cached_result);
        }
    }

    #[cfg(feature = "runtime_build")]
    {
        let mut process_dict = FxHashMap::default();

        match process_type_bit {
            ProcessType::None => {}
            ProcessType::Fanjian => {
                process_dict.extend(FANJIAN.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }
            ProcessType::Delete => {
                process_dict.extend(TEXT_DELETE.trim().lines().map(|pair_str| (pair_str, "")));
                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }
            ProcessType::Normalize => {
                for process_map in [NORM, NUM_NORM] {
                    process_dict.extend(process_map.trim().lines().map(|pair_str| {
                        let mut pair_str_split = pair_str.split('\t');
                        (
                            pair_str_split.next().unwrap(),
                            pair_str_split.next().unwrap(),
                        )
                    }));
                }
            }
            ProcessType::PinYin => {
                process_dict.extend(PINYIN.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }
            ProcessType::PinYinChar => {
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

        let (process_replace_list, process_matcher) = match process_type_bit {
            ProcessType::Fanjian | ProcessType::PinYin | ProcessType::PinYinChar => (
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
            ProcessType::Delete | ProcessType::Normalize => (
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
        process_matcher_cache.insert(process_type_bit, Arc::clone(&uncached_result));
        return uncached_result;
    }

    #[cfg(not(feature = "runtime_build"))]
    {
        let (process_replace_list, process_matcher) = match process_type_bit {
            ProcessType::None => {
                let empty_patterns: Vec<&str> = Vec::new();
                (
                    Vec::new(),
                    ProcessMatcher::Others(AhoCorasick::new(&empty_patterns).unwrap()),
                )
            }
            ProcessType::Fanjian => (
                FANJIAN_PROCESS_REPLACE_LIST_STR.lines().collect(),
                // Guaranteed not failed
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                        FANJIAN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),
            ProcessType::Delete => {
                #[cfg(feature = "dfa")]
                {
                    let mut process_dict = FxHashMap::default();
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
            ProcessType::Normalize => {
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
            ProcessType::PinYin => (
                PINYIN_PROCESS_REPLACE_LIST_STR.lines().collect(),
                // Guaranteed not failed
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                        PINYIN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),
            ProcessType::PinYinChar => (
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
        process_matcher_cache.insert(process_type_bit, Arc::clone(&uncached_result));
        uncached_result
    }
}

/// Process a given text based on a single-bit process type.
///
/// This function applies a specific processing rule to the input text, based on
/// the provided `process_type_bit`. Note that this function can only handle one
/// bit of `process_type` at a time; it will return an error if more than one bit
/// is set in `process_type_bit`.
///
/// # Arguments
///
/// * `process_type_bit` - A [ProcessType] representing a single processing rule to apply.
/// * `text` - A string slice representing the text to be processed.
///
/// # Returns
///
/// A [Result] containing either:
/// * `Ok(Cow<str>)` with the processed text, or
/// * `Err(&'static str)` with an error message if more than one bit is set in `process_type_bit`.
///
/// # Errors
///
/// This function returns an error if `process_type_bit` has more than one bit set,
/// as the function is designed to process only one type of transformation at a time.
///
/// # Example
///
/// ```
/// use matcher_rs::{text_process, ProcessType};
///
/// let process_type = ProcessType::Delete;
/// let text = "Some text to process";
///
/// match text_process(process_type, text) {
///     Ok(processed_text) => println!("Processed text: {}", processed_text),
///     Err(e) => println!("Error: {}", e),
/// };
/// ```
///
/// # Panics
///
/// This function does not panic under normal circumstances. It uses `unreachable!()`
/// to mark code paths that should not be possible based on earlier checks and logic.
#[inline(always)]
pub fn text_process(
    process_type_bit: ProcessType,
    text: &str,
) -> Result<Cow<'_, str>, &'static str> {
    if process_type_bit.iter().count() > 1 {
        return Err("text_process function only accept one bit of process_type");
    }

    let cached_result = get_process_matcher(process_type_bit);
    let (process_replace_list, process_matcher) = cached_result.as_ref();
    let mut result = Cow::Borrowed(text);
    match (process_type_bit, process_matcher) {
        (ProcessType::None, _) => {}
        (ProcessType::Delete, pm) => match pm.delete_all(text) {
            (true, Cow::Owned(pt)) => {
                result = Cow::Owned(pt);
            }
            (false, _) => {}
            (_, _) => unreachable!(),
        },
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

/// Reduces the text based on a composite process type by applying a sequence of processing rules.
///
/// This function iteratively applies multiple processing rules specified by `process_type` to the
/// input `text`. It maintains a list of `processed_text_list` where each entry represents the text
/// at a particular stage of processing.
///
/// # Arguments
///
/// * `process_type` - A [ProcessType] representing a composite of multiple processing rules to apply.
/// * `text` - A string slice representing the text to be processed.
///
/// # Returns
///
/// An [ArrayVec] containing the processed text at each step. The initial text is always included as the first element.
///
/// # Example
///
/// ```
/// use matcher_rs::{reduce_text_process, ProcessType};
///
/// let process_type = ProcessType::Delete | ProcessType::PinYin;
/// let text = "Some text to process";
///
/// let result = reduce_text_process(process_type, text);
/// for processed_text in result.iter() {
///     println!("Processed text: {}", processed_text);
/// }
/// ```
///
/// # Safety
///
/// Unsafe code is used to access the last element of `processed_text_list`. This is safe because
/// the list is always guaranteed to have at least one element (the original input text) before accessing
/// its last element.
///
/// # Panics
///
/// This function does not panic under normal circumstances. It uses `unreachable!()` to mark code
/// paths that should not be possible based on earlier checks and logic.
#[inline(always)]
pub fn reduce_text_process<'a>(
    process_type: ProcessType,
    text: &'a str,
) -> ArrayVec<[Cow<'a, str>; 8]> {
    let mut processed_text_list: ArrayVec<[Cow<'a, str>; 8]> = ArrayVec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let cached_result = get_process_matcher(process_type_bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();
        // Guaranteed not failed
        let tmp_processed_text = unsafe { processed_text_list.last_mut().unwrap_unchecked() };

        match (process_type_bit, process_matcher) {
            (ProcessType::None, _) => {}
            (ProcessType::Delete, pm) => match pm.delete_all(tmp_processed_text.as_ref()) {
                (true, Cow::Owned(pt)) => {
                    processed_text_list.push(Cow::Owned(pt));
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
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

/// Applies a sequence of processing rules to the input `text` and emits the processed text at each step.
///
/// This function iteratively applies multiple processing rules specified by `process_type` to the
/// input `text`. It maintains a list of `processed_text_list` where each entry represents the text
/// at a particular stage of processing.
///
/// # Arguments
///
/// * `process_type` - A [ProcessType] representing a composite of multiple processing rules to apply.
/// * `text` - A string slice representing the text to be processed.
///
/// # Returns
///
/// An [ArrayVec] containing the processed text at each step. The initial text is always included as the first element.
///
/// # Example
///
/// ```
/// use matcher_rs::{reduce_text_process_emit, ProcessType};
///
/// let process_type = ProcessType::Delete | ProcessType::PinYin;
/// let text = "Some text to process";
///
/// let result = reduce_text_process_emit(process_type, text);
/// for processed_text in result.iter() {
///     println!("Processed text: {}", processed_text);
/// }
/// ```
///
/// # Safety
///
/// Unsafe code is used to access the last element of `processed_text_list`. This is safe because
/// the list is always guaranteed to have at least one element (the original input text) before accessing
/// its last element.
///
/// # Panics
///
/// This function does not panic under normal circumstances. It uses `unreachable!()` to mark code
/// paths that should not be possible based on earlier checks and logic.
#[inline(always)]
pub fn reduce_text_process_emit<'a>(
    process_type: ProcessType,
    text: &'a str,
) -> ArrayVec<[Cow<'a, str>; 8]> {
    let mut processed_text_list: ArrayVec<[Cow<'a, str>; 8]> = ArrayVec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let cached_result = get_process_matcher(process_type_bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();
        // Guaranteed not failed
        let tmp_processed_text = unsafe { processed_text_list.last_mut().unwrap_unchecked() };

        match (process_type_bit, process_matcher) {
            (ProcessType::None, _) => {}
            (ProcessType::Delete, pm) => match pm.delete_all(tmp_processed_text.as_ref()) {
                (true, Cow::Owned(pt)) => {
                    processed_text_list.push(Cow::Owned(pt));
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
            (_, pm) => match pm.replace_all(tmp_processed_text.as_ref(), process_replace_list) {
                (true, Cow::Owned(pt)) => {
                    *tmp_processed_text = Cow::Owned(pt);
                }
                (false, _) => {}
                (_, _) => unreachable!(),
            },
        }
    }

    processed_text_list
}

/// A node in the process type tree, representing a processing rule and its children.
///
/// This struct is used in the context of applying a series of text processing rules. Each node
/// holds a specific processing rule (represented by `process_type_bit`), a list of associated
/// process types (`process_type_list`), a flag indicating whether the node has been processed
/// (`is_processed`), the index of the processed text (`processed_text_index`), and its child nodes
/// (`children`).
///
/// # Fields
///
/// * `process_type_list` - An [ArrayVec] containing the list of processing types associated with this node.
/// * `process_type_bit` - A [ProcessType] representing the specific processing rule for this node.
/// * `is_processed` - A [bool] flag indicating whether the node has been processed.
/// * `processed_text_index` - An [usize] indicating the index of the processed text.
/// * `children` - An [ArrayVec] containing the indices of child nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ProcessTypeBitNode {
    process_type_list: ArrayVec<[ProcessType; 8]>,
    process_type_bit: ProcessType,
    is_processed: bool,
    processed_text_index: usize,
    children: ArrayVec<[usize; 8]>,
}

/// Constructs a processing type tree from a list of processing types.
///
/// This function builds a [Vec] of `ProcessTypeBitNode` from a given list of [ProcessType].
/// Each node in the tree corresponds to a bit in a composite processing type, allowing for
/// efficient traversal and application of processing rules.
///
/// # Arguments
///
/// * `process_type_list` - A slice of [ProcessType] that will be used to construct the tree.
///
/// # Returns
///
/// A [Vec] containing `ProcessTypeBitNode`s that represent the processing type tree.
///
/// # Details
///
/// The tree is constructed by traversing each [ProcessType] in the input set and building a chain
/// of nodes for each bit in the [ProcessType]. If a node for a specific bit already exists, it reuses
/// the node; otherwise, it creates a new node. Each node maintains a list of process types and its children,
/// ensuring efficient lookups and updates.
///
/// # Panics
///
/// This function does not panic under normal circumstances. It assumes that [ProcessType::iter()]
/// provides a finite iterator and that array operations on [ArrayVec] are safe as long as the constraints
/// are respected.
///
/// # Safety
///
/// The function does not involve any unsafe operations.
pub fn build_process_type_tree(process_type_set: &IdSet) -> Vec<ProcessTypeBitNode> {
    let mut process_type_tree = Vec::new();
    let root = ProcessTypeBitNode {
        process_type_list: ArrayVec::new(),
        process_type_bit: ProcessType::None,
        is_processed: true,
        processed_text_index: 0,
        children: ArrayVec::new(),
    };
    process_type_tree.push(root);
    for process_type_usize in process_type_set.iter() {
        let process_type = ProcessType::from_bits(process_type_usize as u8).unwrap();
        let mut current_node_index = 0;
        for process_type_bit in process_type.into_iter() {
            let current_node = process_type_tree[current_node_index];
            if current_node.process_type_bit == process_type_bit {
                continue;
            }

            let mut is_found = false;
            for child_node_index in current_node.children {
                if process_type_bit == process_type_tree[child_node_index].process_type_bit {
                    current_node_index = child_node_index;
                    is_found = true;
                    break;
                }
            }

            if !is_found {
                let mut child = ProcessTypeBitNode {
                    process_type_list: ArrayVec::new(),
                    process_type_bit,
                    is_processed: false,
                    processed_text_index: 0,
                    children: ArrayVec::new(),
                };
                child.process_type_list.push(process_type);
                process_type_tree.push(child);
                let new_node_index = process_type_tree.len() - 1;
                process_type_tree[current_node_index]
                    .children
                    .push(new_node_index);
                current_node_index = new_node_index;
            } else {
                process_type_tree[current_node_index]
                    .process_type_list
                    .push(process_type);
            }
        }
    }
    process_type_tree
}

/// Reduces the text process by applying a tree of process type nodes.
///
/// This function takes a preconstructed tree of `ProcessTypeBitNode` and applies the processing rules
/// to the input text based on the tree structure. It iterates over each node in the tree and applies
/// the corresponding processing rules, ensuring that each node's `process_type_bit` is handled
/// appropriately. The results of the processing are stored in an [ArrayVec] which contains tuples of
/// the processed text and an [IdSet] of process type bits.
///
/// # Arguments
///
/// * `process_type_tree` - A reference to a slice of `ProcessTypeBitNode` that represents the
///   process type tree. Each node in this tree corresponds to a specific bit in a composite process type.
/// * `text` - A string slice that represents the text to be processed.
///
/// # Returns
///
/// An [ArrayVec] containing tuples. Each tuple consists of:
/// * A [Cow] string, which could be either the borrowed input text or an owned version of the processed text.
/// * An [IdSet] which contains the bits of the processed [ProcessType].
///
/// # Safety
///
/// This function uses unsafe code to manipulate slices and raw pointers. The unsafe blocks are
/// used for unchecked access to slices and indices, which is safe as long as the assumptions
/// about the data structures hold. Ensure that the provided `process_type_tree` is well-formed
/// and the indices are valid.
///
/// # Panics
///
/// This function assumes that array operations on [ArrayVec] and slice operations on the process type tree
/// and `processed_text_process_type_set` are safe. It may panic if the assumptions about the data structure
/// are violated, such as out-of-bounds access.
#[inline(always)]
pub fn reduce_text_process_with_tree<'a>(
    process_type_tree: &[ProcessTypeBitNode],
    text: &'a str,
) -> ArrayVec<[(Cow<'a, str>, IdSet); 16]> {
    let mut process_type_tree_copied: Vec<ProcessTypeBitNode> = process_type_tree.to_vec();

    let mut processed_text_process_type_set: ArrayVec<[(Cow<'a, str>, IdSet); 16]> =
        ArrayVec::new();
    processed_text_process_type_set.push((
        Cow::Borrowed(text),
        IdSet::from_iter([ProcessType::None.bits() as usize]),
    ));

    for (current_node_index, current_node) in process_type_tree.iter().enumerate() {
        let (left_tree, right_tree) = unsafe {
            process_type_tree_copied.split_at_mut_unchecked(current_node_index.unchecked_add(1))
        };

        let current_copied_node = unsafe { left_tree.get_unchecked(current_node_index) };
        let mut current_index = current_copied_node.processed_text_index;
        let current_text_ptr =
            unsafe { processed_text_process_type_set.get_unchecked(current_index) }
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
                let cached_result = get_process_matcher(child_node.process_type_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match child_node.process_type_bit {
                    ProcessType::None => {}
                    ProcessType::Delete => {
                        match process_matcher.delete_all(unsafe { &*current_text_ptr }) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_process_type_set.push((
                                    Cow::Owned(pt),
                                    IdSet::from_iter(
                                        child_node
                                            .process_type_list
                                            .iter()
                                            .map(|smt| smt.bits() as usize),
                                    ),
                                ));
                                current_index = unsafe {
                                    processed_text_process_type_set.len().unchecked_sub(1)
                                };
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
                            processed_text_process_type_set.push((Cow::Owned(pt), IdSet::new()));
                            current_index =
                                unsafe { processed_text_process_type_set.len().unchecked_sub(1) };
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
            let processed_text_process_type_tuple =
                unsafe { processed_text_process_type_set.get_unchecked_mut(current_index) };
            processed_text_process_type_tuple.1.extend(
                child_node
                    .process_type_list
                    .iter()
                    .map(|smt| smt.bits() as usize),
            );
        }
    }

    processed_text_process_type_set
}

/// Reduces the given `text` based on a list of `process_type`s and returns an array of tuples
/// containing the processed text and an [IdSet] of process type identifiers.
///
/// # Arguments
///
/// * `process_type_list` - A slice of [ProcessType] indicating how the text should be processed.
/// * `text` - A string slice that is to be processed.
///
/// # Returns
///
/// An [ArrayVec] containing tuples where each tuple consists of:
/// - A [Cow<'a, str>] representing the processed text.
/// - An [IdSet] containing the identifiers of the process types applied.
#[inline(always)]
pub fn reduce_text_process_with_set<'a>(
    process_type_set: &IdSet,
    text: &'a str,
) -> ArrayVec<[(Cow<'a, str>, IdSet); 16]> {
    let mut process_type_tree = Vec::with_capacity(8);
    let mut root = ProcessTypeBitNode {
        process_type_list: ArrayVec::new(),
        process_type_bit: ProcessType::None,
        is_processed: true,
        processed_text_index: 0,
        children: ArrayVec::new(),
    };
    root.process_type_list.push(ProcessType::None);
    process_type_tree.push(root);

    let mut processed_text_process_type_set: ArrayVec<[(Cow<'a, str>, IdSet); 16]> =
        ArrayVec::new();
    processed_text_process_type_set.push((
        Cow::Borrowed(text),
        IdSet::from_iter([ProcessType::None.bits() as usize]),
    ));

    for process_type_usize in process_type_set.iter() {
        let process_type = ProcessType::from_bits(process_type_usize as u8).unwrap();
        let mut current_text = text;
        let mut current_index = 0;
        let mut current_node_index = 0;

        for process_type_bit in process_type.iter() {
            let current_node = unsafe { process_type_tree.get_unchecked(current_node_index) };
            if current_node.process_type_bit == process_type_bit {
                continue;
            }

            let mut is_found = false;
            for child_node_index in current_node.children {
                if process_type_bit
                    == unsafe { process_type_tree.get_unchecked(child_node_index) }.process_type_bit
                {
                    current_node_index = child_node_index;
                    is_found = true;
                    break;
                }
            }
            let current_node = unsafe { process_type_tree.get_unchecked_mut(current_node_index) };

            if !is_found {
                let cached_result = get_process_matcher(process_type_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match process_type_bit {
                    ProcessType::None => {}
                    ProcessType::Delete => match process_matcher.delete_all(current_text) {
                        (true, Cow::Owned(pt)) => {
                            processed_text_process_type_set.push((Cow::Owned(pt), IdSet::new()));
                            current_index = processed_text_process_type_set.len() - 1;

                            let processed_text_process_type_tuple = unsafe {
                                processed_text_process_type_set
                                    .get_unchecked_mut(current_node.processed_text_index)
                            };
                            processed_text_process_type_tuple
                                .1
                                .insert(process_type.bits() as usize);
                        }
                        (false, _) => {
                            current_index = current_node.processed_text_index;
                        }
                        (_, _) => unreachable!(),
                    },
                    _ => match process_matcher.replace_all(current_text, process_replace_list) {
                        (true, Cow::Owned(pt)) => {
                            processed_text_process_type_set.push((Cow::Owned(pt), IdSet::new()));
                            current_index = processed_text_process_type_set.len() - 1;
                        }
                        (false, _) => {
                            current_index = current_node.processed_text_index;
                        }
                        (_, _) => unreachable!(),
                    },
                }

                let mut child = ProcessTypeBitNode {
                    process_type_list: ArrayVec::new(),
                    process_type_bit,
                    is_processed: true,
                    processed_text_index: current_index,
                    children: ArrayVec::new(),
                };
                child.process_type_list.push(process_type);
                process_type_tree.push(child);

                let new_node_index = process_type_tree.len() - 1;
                let current_node =
                    unsafe { process_type_tree.get_unchecked_mut(current_node_index) };
                current_node.children.push(new_node_index);
                current_node_index = new_node_index;
            } else {
                current_index = current_node.processed_text_index;
                current_node.process_type_list.push(process_type);
            }

            let processed_text_process_type_tuple =
                unsafe { processed_text_process_type_set.get_unchecked_mut(current_index) };
            processed_text_process_type_tuple
                .1
                .insert(process_type.bits() as usize);
            current_text = unsafe { processed_text_process_type_set.get_unchecked(current_index) }
                .0
                .as_ref();
        }
    }

    processed_text_process_type_set
}
