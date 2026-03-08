use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::sync::{Arc, LazyLock};

use aho_corasick::AhoCorasick;
#[cfg(any(feature = "runtime_build", feature = "dfa"))]
use aho_corasick::{AhoCorasickBuilder, AhoCorasickKind, MatchKind as AhoCorasickMatchKind};
use bitflags::bitflags;
#[cfg(not(feature = "runtime_build"))]
use daachorse::CharwiseDoubleArrayAhoCorasick;
#[cfg(feature = "runtime_build")]
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
};
use parking_lot::RwLock;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::process::constants::*;

thread_local! {
    static STRING_POOL: RefCell<Vec<String>> = RefCell::new(Vec::with_capacity(16));
}

pub fn get_string_from_pool(capacity: usize) -> String {
    STRING_POOL.with(|pool| {
        if let Some(mut s) = pool.borrow_mut().pop() {
            s.clear();
            if s.capacity() < capacity {
                s.reserve(capacity - s.capacity());
            }
            s
        } else {
            String::with_capacity(capacity)
        }
    })
}

pub fn return_string_to_pool(s: String) {
    STRING_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < 128 {
            pool.push(s);
        }
    });
}

pub fn return_processed_string_to_pool(mut processed_text_process_type_masks: ProcessedTextMasks) {
    for (cow, _) in processed_text_process_type_masks.drain(..) {
        if let Cow::Owned(s) = cow {
            return_string_to_pool(s);
        }
    }
}

bitflags! {
    /// Represents different types of text processing operations.
    ///
    /// This structure uses bitflags to allow combining multiple processing steps
    /// (e.g., converting to Simplified Chinese AND deleting noise characters).
    ///
    /// # Detailed Explanation / Algorithm
    /// The bitflags are used to build a `ProcessTypeBitNode` tree. When text is processed,
    /// the engine traverses this tree, applying the transformation for each set bit.
    /// This allows the system to generate all required variants of a text (e.g., Pinyin,
    /// Simplified Chinese) efficiently by sharing common intermediate steps.
    ///
    /// # Fields
    /// * `None` - No processing (sentinel value 0x01).
    /// * `Fanjian` - Traditional to Simplified Chinese conversion.
    /// * `Delete` - Noise character removal (punctuation, whitespace).
    /// * `Normalize` - Character normalization (full-width to half-width, etc.).
    /// * `PinYin` - Conversion to Pinyin with spaces.
    /// * `PinYinChar` - Conversion to Pinyin without spaces.
    ///
    /// # Examples
    /// ```rust
    /// use matcher_rs::ProcessType;
    ///
    /// let process = ProcessType::Fanjian | ProcessType::Delete;
    /// assert!(process.contains(ProcessType::Fanjian));
    /// ```
    #[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Default)]
    pub struct ProcessType: u8 {
        /// No processing action.
        const None = 0b00000001;

        /// Traditional Chinese to Simplified Chinese.
        const Fanjian = 0b00000010;

        /// Deleting noise characters and whitespace.
        const Delete = 0b00000100;

        /// General normalization (case folding, width normalization).
        const Normalize = 0b00001000;

        /// Combined: Delete + Normalize.
        const DeleteNormalize = 0b00001100;

        /// Combined: Fanjian + Delete + Normalize.
        const FanjianDeleteNormalize = 0b00001110;

        /// Pinyin conversion (with boundaries).
        const PinYin = 0b00010000;

        /// Pinyin conversion (character level).
        const PinYinChar = 0b00100000;
    }
}

impl Serialize for ProcessType {
    /// Serializes a [`ProcessType`] instance into its bit representation using the provided serializer.
    ///
    /// This implementation leverages the [`Serialize`] trait from Serde to convert the [`ProcessType`]
    /// bitflag into a serializable form. The [`ProcessType::bits`] method extracts the raw bit value of the
    /// [`ProcessType`], which is then passed to the serializer.
    ///
    /// # Arguments
    ///
    /// * [`Serializer`] - An instance of the [`Serializer`] trait that will handle the actual serialization.
    ///
    /// # Returns
    ///
    /// This method returns a result containing either the serialized value ([`Serializer::Ok`]) or an error ([`Serializer::Error`]).
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ProcessType {
    /// Deserializes a [`ProcessType`] instance from its bit representation using the provided deserializer.
    ///
    /// This implementation leverages the [`Deserialize`] trait from Serde to convert a bitflag
    /// representation back into a [`ProcessType`] instance. The [`ProcessType::from_bits_retain`] method is used
    /// to reconstruct the [`ProcessType`] from the deserialized bit value.
    ///
    /// # Arguments
    ///
    /// * [`Deserializer`] - An instance of the [`Deserializer`] trait that will handle the actual deserialization.
    ///
    /// # Returns
    ///
    /// This method returns a [`Result`] containing either the deserialized [`ProcessType`] instance
    /// (Ok([`ProcessType`])) or an error ([`Deserializer::Error`]).
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

type ProcessMatcherCache = RwLock<HashMap<ProcessType, Arc<(Vec<&'static str>, ProcessMatcher)>>>;

/// A global, lazily-initialized cache for storing process matchers.
///
/// Maps [`ProcessType`] keys to [`Arc`] instances holding tuples of a replacement-string
/// list and a `ProcessMatcher`. Protected by a [`parking_lot::RwLock`] for efficient
/// concurrent read access.
///
/// The cache is capped at 8 entries, matching the number of distinct single-bit
/// [`ProcessType`] variants that can be passed to [`get_process_matcher`].
pub static PROCESS_MATCHER_CACHE: LazyLock<ProcessMatcherCache> =
    LazyLock::new(|| RwLock::new(HashMap::with_capacity_and_hasher(8, Default::default())));

/// A fixed-capacity array of `(processed_text, u64)` pairs produced by
/// the `reduce_text_process_*` family of functions.
///
/// The capacity of 16 supports up to 16 distinct text-processing variants per input,
/// which is the practical upper bound given the number of [`ProcessType`] flags.
///
/// # Type Parameters
/// * `'a` - The lifetime of the underlying string slice.
pub type ProcessedTextMasks<'a> = Vec<(Cow<'a, str>, u64)>;

/// Represents different types of process matchers used for text processing.
///
/// This enum contains variants for different kinds of matchers that can operate on text to find and
/// replace or delete specific patterns. Each variant is designed to handle specific use cases
/// effectively. The enum is clonable, allowing for easy duplication when necessary.
///
/// # Variants
/// * `LeftMost` - Uses a [`CharwiseDoubleArrayAhoCorasick<u32>`] matcher to find the leftmost non-overlapping matches.
/// * `Chinese` - Uses a [`CharwiseDoubleArrayAhoCorasick<u32>`] matcher specifically tailored to handle Chinese text.
/// * `Others` - Uses a standard [`AhoCorasick`] matcher for general-purpose text processing.
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
    /// * `text` - A string slice representing the input text to be processed and matched.
    /// * `process_replace_list` - A slice of string slices containing the replacement strings. Each match
    ///   found in the text will be replaced by the respective string from this list.
    ///
    /// # Returns
    /// This method returns a tuple:
    /// * `bool` - A boolean indicating whether any replacements were made (`true` if replacements were made, `false` otherwise).
    /// * [`Cow<'a, str>`] - A copy-on-write string containing the processed text. If no replacements were made,
    ///   a borrowed version of the original text is returned. Otherwise, an owned version of the text with
    ///   the replacements is returned.
    #[inline(always)]
    pub fn replace_all<'a>(
        &self,
        text: &'a str,
        process_replace_list: &[&str],
    ) -> (bool, Cow<'a, str>) {
        /// Shared iteration logic for replace: given a mutable iterator of matches,
        /// builds the replaced string using the `$idx` expression to look up the
        /// replacement index from each match object.
        macro_rules! do_replace {
            ($iter:expr, $idx:expr) => {{
                let mut iter = $iter;
                if let Some(first_mat) = iter.next() {
                    let mut result = get_string_from_pool(text.len());
                    result.push_str(&text[0..first_mat.start()]);
                    result.push_str(process_replace_list[$idx(&first_mat)]);
                    let mut last_end = first_mat.end();
                    for mat in iter {
                        result.push_str(&text[last_end..mat.start()]);
                        result.push_str(process_replace_list[$idx(&mat)]);
                        last_end = mat.end();
                    }
                    result.push_str(&text[last_end..]);
                    return (true, Cow::Owned(result));
                }
            }};
        }

        match self {
            #[cfg(not(feature = "dfa"))]
            ProcessMatcher::LeftMost(ac) => do_replace!(
                ac.leftmost_find_iter(text),
                |m: &daachorse::Match<u32>| m.value() as usize
            ),
            ProcessMatcher::Chinese(ac) => {
                do_replace!(ac.find_iter(text), |m: &daachorse::Match<u32>| m.value()
                    as usize)
            }
            ProcessMatcher::Others(ac) => {
                do_replace!(ac.find_iter(text), |m: &aho_corasick::Match| m
                    .pattern()
                    .as_usize())
            }
        }
        (false, Cow::Borrowed(text))
    }

    /// Deletes all matched patterns in the provided text.
    ///
    /// This function iterates over the text and uses the appropriate `ProcessMatcher` variant to locate all matches.
    /// It then deletes the occurrences of these patterns, holding the remaining text fragments together.
    ///
    /// # Arguments
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    /// This function returns a tuple:
    /// * `bool` - A boolean indicating whether any deletions were made (`true` if deletions were made, `false` otherwise).
    /// * [`Cow<'a, str>`] - A copy-on-write string containing the processed text. If no deletions were made,
    ///   a borrowed version of the original text is returned. Otherwise, an owned version of the text with
    ///   the deletions is returned.
    #[inline(always)]
    pub fn delete_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
        /// Shared iteration logic for delete: given a mutable iterator of matches,
        /// builds the result string by concatenating only the non-matched segments.
        macro_rules! do_delete {
            ($iter:expr) => {{
                let mut iter = $iter;
                if let Some(first_mat) = iter.next() {
                    let mut result = get_string_from_pool(text.len());
                    result.push_str(&text[0..first_mat.start()]);
                    let mut last_end = first_mat.end();
                    for mat in iter {
                        result.push_str(&text[last_end..mat.start()]);
                        last_end = mat.end();
                    }
                    result.push_str(&text[last_end..]);
                    return (true, Cow::Owned(result));
                }
            }};
        }

        match self {
            #[cfg(not(feature = "dfa"))]
            ProcessMatcher::LeftMost(ac) => do_delete!(ac.leftmost_find_iter(text)),
            ProcessMatcher::Chinese(ac) => do_delete!(ac.find_iter(text)),
            ProcessMatcher::Others(ac) => do_delete!(ac.find_iter(text)),
        }
        (false, Cow::Borrowed(text))
    }
}

/// Retrieves or constructs a `ProcessMatcher` for a given single-bit [`ProcessType`].
///
/// # Algorithm
/// 1. Checks `PROCESS_MATCHER_CACHE` (`RwLock`). If exists, returns cloned `Arc`.
/// 2. If missing, dynamically configures the appropriate matching automaton:
///    - Transforms predefined dictionaries (`Fanjian`, `Delete`, `Normalize`, etc.) into lookup `HashMaps` or vector token lists.
///    - Depending on the `ProcessType` and compilation feature flags, instantiates:
///      * `CharwiseDoubleArrayAhoCorasick`: A highly-optimized state machine specifically for Chinese/CJK (handles UTF-8 char bounds compactly). Used for `Fanjian`, `PinYin`, etc.
///      * `AhoCorasick`: The standard string matcher optimal for general bytes (used for `Normalize`, `Delete`, etc.).
///    - If statically compiled (`not(feature = "runtime_build")`), loads serialized `daachorse` binaries (e.g. `FANJIAN_PROCESS_MATCHER_BYTES`) using `unsafe` zero-copy deserialization for instant startup.
/// 3. Safely inserts into the `RwLock` cache and returns.
///
/// # Arguments
/// * `process_type_bit` - The text processing rules to be applied, represented by the `ProcessType` bitflags enum. (Only a single bit is supported here).
///
/// # Returns
/// - An [`Arc`] containing a tuple of a vector of replacement strings and a `ProcessMatcher`.
///
/// # Panics
/// - The function will panic if the `process_type_bit` is any variant not explicitly handled in the routing layer.
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
        let mut process_dict = HashMap::new();

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
        // Re-check after acquiring the write lock: another thread may have inserted
        // the same key between our read-miss and this write acquisition.
        if let Some(cached_result) = process_matcher_cache.get(&process_type_bit) {
            return Arc::clone(cached_result);
        }
        process_matcher_cache.insert(process_type_bit, Arc::clone(&uncached_result));
        uncached_result
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
                // SAFETY: [`FANJIAN_PROCESS_MATCHER_BYTES`] matches the identical version and byte layout
                // exported manually using [`CharwiseDoubleArrayAhoCorasick`] build constraints during static compilation.
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
                    let mut process_dict = HashMap::new();
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
                        // SAFETY: [`TEXT_DELETE_PROCESS_MATCHER_BYTES`] matches the identical byte layout compiled.
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
                        // SAFETY: [`NORMALIZE_PROCESS_MATCHER_BYTES`] matches the identical byte layout compiled.
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
                // SAFETY: [`PINYIN_PROCESS_MATCHER_BYTES` matches the identical byte layout compiled.
                ProcessMatcher::Chinese(unsafe {
                    CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                        PINYIN_PROCESS_MATCHER_BYTES,
                    )
                    .0
                }),
            ),
            ProcessType::PinYinChar => (
                PINYINCHAR_PROCESS_REPLACE_LIST_STR.lines().collect(),
                // SAFETY: [`PINYIN_PROCESS_MATCHER_BYTES` matches the identical byte layout compiled.
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
        // Re-check after acquiring the write lock: another thread may have inserted
        // the same key between our read-miss and this write acquisition.
        if let Some(cached_result) = process_matcher_cache.get(&process_type_bit) {
            return Arc::clone(cached_result);
        }
        process_matcher_cache.insert(process_type_bit, Arc::clone(&uncached_result));
        uncached_result
    }
}

/// Process text based on a [`ProcessType`] bitmask.
///
/// # Detailed Explanation / Algorithm
/// This function iteratively applies transformations for each bit set in the
/// composite `process_type_bit`.
/// 1. Fetches the appropriate `ProcessMatcher` from the global cache for each bit.
/// 2. Applies the transformation (replace or delete) sequentially to the input string.
///
/// # Arguments
/// * `process_type_bit` - The rules (single or composite) representing the transformations to apply.
/// * `text` - The input string.
///
/// # Returns
/// The processed text (as a [`Cow`]).
#[inline(always)]
pub fn text_process<'a>(process_type_bit: ProcessType, text: &'a str) -> Cow<'a, str> {
    let mut result = Cow::Borrowed(text);

    for bit in process_type_bit.iter() {
        let cached_result = get_process_matcher(bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();

        match (bit, process_matcher) {
            (ProcessType::None, _) => continue,
            (ProcessType::Delete, pm) => {
                if let (true, Cow::Owned(pt)) = pm.delete_all(result.as_ref()) {
                    result = Cow::Owned(pt);
                }
            }
            (_, pm) => {
                if let (true, Cow::Owned(pt)) =
                    pm.replace_all(result.as_ref(), process_replace_list)
                {
                    result = Cow::Owned(pt);
                }
            }
        }
    }

    result
}

/// Applies a sequence of rules to text, returning all intermediate variants.
///
/// # Detailed Explanation / Algorithm
/// Iteratively applies transformations for each bit set in the composite `process_type`.
/// It maintains a list of all mutated states, chaining the output of one step into the next.
///
/// # Arguments
/// * `process_type` - Composite rules to apply.
/// * `text` - The input string.
///
/// # Returns
/// A vector of all text variants generated during the process.
#[inline(always)]
pub fn reduce_text_process<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    let mut processed_text_list: Vec<Cow<'a, str>> = Vec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let cached_result = get_process_matcher(process_type_bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();
        let tmp_processed_text = processed_text_list
            .last_mut()
            .expect("It should always have at least one element");

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
/// # Algorithm
/// Almost identical to `reduce_text_process`, but when executing `replace_all` and observing no matching blocks to alter,
/// it directly assigns ownership backward (`*tmp_processed_text = Cow::Owned(pt);`) avoiding extending the buffer length redundantly, keeping
/// the returned stage variations tightly condensed.
///
/// # Arguments
/// * `process_type` - The text processing rules to be applied, represented by the `ProcessType` bitflags enum.
/// * `text` - A string slice representing the input text to be processed and matched.
///
/// # Returns
/// A [`Vec`] containing the processed text at each step. The initial text is always included as the first element.
///
/// # Examples
///
/// ```rust
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
#[inline(always)]
pub fn reduce_text_process_emit<'a>(process_type: ProcessType, text: &'a str) -> Vec<Cow<'a, str>> {
    let mut processed_text_list: Vec<Cow<'a, str>> = Vec::new();
    processed_text_list.push(Cow::Borrowed(text));

    for process_type_bit in process_type.iter() {
        let cached_result = get_process_matcher(process_type_bit);
        let (process_replace_list, process_matcher) = cached_result.as_ref();
        let tmp_processed_text = processed_text_list
            .last_mut()
            .expect("It should always have at least one element");

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
/// # Algorithm
/// 1. Nodes represent isolated mutation states during text reductions.
/// 2. As bits from a composite `ProcessType` are evaluated sequentially, they form directional edges to child nodes.
/// 3. `processed_text_index` ensures multiple variants sharing the exact same sequence reuse the mutated string payload instead of duplicating work.
///
/// # Fields
///
/// * `process_type_list` - An [`Vec`] containing the list of processing types associated with this node.
/// * `process_type_bit` - A [`ProcessType`] representing the specific processing rule for this node.
/// * `is_processed` - A [`bool`] flag indicating whether the node has been processed.
/// * `processed_text_index` - An [`usize`] indicating the index of the processed text.
/// * `children` - An [`Vec`] containing the indices of child nodes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessTypeBitNode {
    process_type_list: Vec<ProcessType>,
    process_type_bit: ProcessType,
    is_processed: bool,
    processed_text_index: usize,
    children: Vec<usize>,
}

/// Constructs a processing type tree from a set of composite processing types.
///
/// # Algorithm
/// 1. Initializes the tree with a root `ProcessType::None` node.
/// 2. Iterates over each provided composite `ProcessType` bitmask from `process_type_set` (e.g. `Fanjian | Delete`).
/// 3. For each constituent single bit inside the composite type, traverses existing nodes downwards from the root.
/// 4. If a matching child node exists for that bit transition, navigates to it and appends the source composite `ProcessType` to the node's tracking list.
/// 5. If missing, spawns a new `ProcessTypeBitNode`, appending it as a child.
///
/// The resulting stricture is a Trie/DAG representing distinct permutation chains of text-processing rules.
/// This guarantees that overlapping intermediate sequences (e.g. `None -> Fanjian -> Delete` vs `None -> Fanjian`)
/// are evaluated cleanly by a runner, heavily optimizing pattern evaluation against distinct variations.
///
/// # Arguments
///
/// * `process_type_set` - A `HashSet` of underlying `u8` composite process types expected by matcher tables.
///
/// # Returns
///
/// A flattened [`Vec`] containing `ProcessTypeBitNode`s whose parent-child indices form the processing type tree.
pub fn build_process_type_tree(process_type_set: &HashSet<u8>) -> Vec<ProcessTypeBitNode> {
    let mut process_type_tree = Vec::new();
    let root = ProcessTypeBitNode {
        process_type_list: Vec::new(),
        process_type_bit: ProcessType::None,
        is_processed: true,
        processed_text_index: 0,
        children: Vec::new(),
    };
    process_type_tree.push(root);
    for process_type_bits in process_type_set.iter() {
        let process_type = ProcessType::from_bits(*process_type_bits).unwrap();
        let mut current_node_index = 0;
        for process_type_bit in process_type.into_iter() {
            let current_node = &process_type_tree[current_node_index];
            if current_node.process_type_bit == process_type_bit {
                continue;
            }

            let mut is_found = false;
            for child_node_index in &current_node.children {
                if process_type_bit == process_type_tree[*child_node_index].process_type_bit {
                    current_node_index = *child_node_index;
                    is_found = true;
                    break;
                }
            }

            if !is_found {
                let mut child = ProcessTypeBitNode {
                    process_type_list: Vec::new(),
                    process_type_bit,
                    is_processed: false,
                    processed_text_index: 0,
                    children: Vec::new(),
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

/// Reduces text by executing a pre-compiled `ProcessTypeBitNode` DAG.
///
/// # Detailed Explanation / Algorithm
/// This is the most efficient way to generate all required text variants.
/// 1. It performs a breadth-first traversal of the pre-compiled `process_type_tree`.
/// 2. For each node, it applies a transformation step ONLY IF the sequence leading to
///    that node hasn't been computed yet.
/// 3. It tracks results in a `ProcessedTextMasks` array.
/// 4. By sharing common prefixes in the transformation tree, it avoids redundant string allocations
///    and Aho-Corasick passes.
///
/// # Arguments
/// * `process_type_tree` - Pre-compiled transformation DAG.
/// * `text` - The input string.
///
/// # Returns
/// A collection of processed variants and their rule bitmasks.
#[inline(always)]
pub fn reduce_text_process_with_tree<'a>(
    process_type_tree: &[ProcessTypeBitNode],
    text: &'a str,
) -> ProcessedTextMasks<'a> {
    let mut process_type_tree_copied: Vec<ProcessTypeBitNode> = process_type_tree.to_vec();

    let mut processed_text_process_type_masks: ProcessedTextMasks<'a> = Vec::new();
    processed_text_process_type_masks.push((Cow::Borrowed(text), 1u64 << ProcessType::None.bits()));

    for (current_node_index, current_node) in process_type_tree.iter().enumerate() {
        let (left_tree, right_tree) = process_type_tree_copied.split_at_mut(current_node_index + 1);
        let current_copied_node = left_tree.get(current_node_index).expect("`current_node_index` will never exceed the iterator bounds of `left_tree` created above.");
        let mut current_index = current_copied_node.processed_text_index;

        for child_node_index in &current_node.children {
            let child_node = right_tree.get_mut(*child_node_index - current_node_index - 1).expect("`child_node_index` is sourced securely from the internally generated structural graph bounds. It is validated against `current_node_index` math ensuring safe projection over `right_tree`.");

            if child_node.is_processed {
                current_index = current_copied_node.processed_text_index;
            } else {
                let cached_result = get_process_matcher(child_node.process_type_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match child_node.process_type_bit {
                    ProcessType::None => {}
                    ProcessType::Delete => {
                        let current_text =
                            processed_text_process_type_masks[current_index].0.as_ref();
                        match process_matcher.delete_all(current_text) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_process_type_masks.push((
                                    Cow::Owned(pt),
                                    child_node
                                        .process_type_list
                                        .iter()
                                        .fold(0u64, |mask, smt| mask | (1u64 << smt.bits())),
                                ));
                                current_index = processed_text_process_type_masks.len() - 1;
                            }
                            (false, _) => {
                                current_index = current_copied_node.processed_text_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                    _ => {
                        let current_text =
                            processed_text_process_type_masks[current_index].0.as_ref();
                        match process_matcher.replace_all(current_text, process_replace_list) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_process_type_masks.push((Cow::Owned(pt), 0u64));
                                current_index = processed_text_process_type_masks.len() - 1;
                            }
                            (false, _) => {
                                current_index = current_copied_node.processed_text_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                }
                child_node.is_processed = true;
            }

            child_node.processed_text_index = current_index;
            let processed_text_process_type_tuple =
                &mut processed_text_process_type_masks[current_index];
            processed_text_process_type_tuple.1 |= child_node
                .process_type_list
                .iter()
                .fold(0u64, |mask, smt| mask | (1u64 << smt.bits()));
        }
    }

    processed_text_process_type_masks
}

/// Reduces text based on a set of rules, building a temporary tree.
///
/// # Detailed Explanation / Algorithm
/// This is similar to `reduce_text_process_with_tree` but it constructs the
/// transformation tree on-the-fly. It's useful when the set of rules is dynamic.
///
/// # Arguments
/// * `process_type_set` - Set of composite `ProcessType`s required.
/// * `text` - The input string.
///
/// # Returns
/// A collection of processed variants and their rule bitmasks.
#[inline(always)]
pub fn reduce_text_process_with_set<'a>(
    process_type_set: &HashSet<u8>,
    text: &'a str,
) -> ProcessedTextMasks<'a> {
    let mut process_type_tree = Vec::with_capacity(8);
    let mut root = ProcessTypeBitNode {
        process_type_list: Vec::new(),
        process_type_bit: ProcessType::None,
        is_processed: true,
        processed_text_index: 0,
        children: Vec::new(),
    };
    root.process_type_list.push(ProcessType::None);
    process_type_tree.push(root);

    let mut processed_text_process_type_masks: ProcessedTextMasks<'a> = Vec::new();
    processed_text_process_type_masks.push((Cow::Borrowed(text), 1u64 << ProcessType::None.bits()));

    for process_type_bits in process_type_set.iter() {
        let process_type = ProcessType::from_bits(*process_type_bits).unwrap();
        let mut current_text = text;
        let mut current_index = 0;
        let mut current_node_index = 0;

        for process_type_bit in process_type.iter() {
            let current_node = &process_type_tree[current_node_index];
            if current_node.process_type_bit == process_type_bit {
                continue;
            }

            let mut is_found = false;
            for child_node_index in &current_node.children {
                if process_type_bit == process_type_tree[*child_node_index].process_type_bit {
                    current_node_index = *child_node_index;
                    is_found = true;
                    break;
                }
            }
            let current_node = &mut process_type_tree[current_node_index];

            if !is_found {
                let cached_result = get_process_matcher(process_type_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match process_type_bit {
                    ProcessType::None => {}
                    ProcessType::Delete => match process_matcher.delete_all(current_text) {
                        (true, Cow::Owned(pt)) => {
                            processed_text_process_type_masks.push((Cow::Owned(pt), 0u64));
                            current_index = processed_text_process_type_masks.len() - 1;

                            let processed_text_process_type_tuple =
                                &mut processed_text_process_type_masks
                                    [current_node.processed_text_index];
                            processed_text_process_type_tuple.1 |= 1u64 << process_type.bits();
                        }
                        (false, _) => {
                            current_index = current_node.processed_text_index;
                        }
                        (_, _) => unreachable!(),
                    },
                    _ => match process_matcher.replace_all(current_text, process_replace_list) {
                        (true, Cow::Owned(pt)) => {
                            processed_text_process_type_masks.push((Cow::Owned(pt), 0u64));
                            current_index = processed_text_process_type_masks.len() - 1;
                        }
                        (false, _) => {
                            current_index = current_node.processed_text_index;
                        }
                        (_, _) => unreachable!(),
                    },
                }

                let mut child = ProcessTypeBitNode {
                    process_type_list: Vec::new(),
                    process_type_bit,
                    is_processed: true,
                    processed_text_index: current_index,
                    children: Vec::new(),
                };
                child.process_type_list.push(process_type);
                process_type_tree.push(child);

                let new_node_index = process_type_tree.len() - 1;
                let current_node = &mut process_type_tree[current_node_index];
                current_node.children.push(new_node_index);
                current_node_index = new_node_index;
            } else {
                current_index = current_node.processed_text_index;
                current_node.process_type_list.push(process_type);
            }

            let processed_text_process_type_tuple =
                &mut processed_text_process_type_masks[current_index];
            processed_text_process_type_tuple.1 |= 1u64 << process_type.bits();
            current_text = processed_text_process_type_masks[current_index].0.as_ref();
        }
    }

    processed_text_process_type_masks
}
