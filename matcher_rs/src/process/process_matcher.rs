use std::borrow::Cow;
use std::cell::RefCell;
#[cfg(feature = "runtime_build")]
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt::Display;
use std::sync::{Arc, OnceLock};

use aho_corasick::AhoCorasick;
#[cfg(feature = "dfa")]
use aho_corasick::{AhoCorasickBuilder, AhoCorasickKind, MatchKind as AhoCorasickMatchKind};
use bitflags::bitflags;
#[cfg(not(feature = "dfa"))]
use daachorse::CharwiseDoubleArrayAhoCorasick;
#[cfg(all(feature = "runtime_build", not(feature = "dfa")))]
use daachorse::{
    CharwiseDoubleArrayAhoCorasickBuilder, MatchKind as DoubleArrayAhoCorasickMatchKind,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::process::constants::*;
use crate::process::single_char_matcher::{SingleCharMatch, SingleCharMatcher};

thread_local! {
    static STRING_POOL: RefCell<Vec<String>> = RefCell::new(Vec::with_capacity(16));
    static REDUCE_STATE: RefCell<Vec<usize>> = RefCell::new(Vec::with_capacity(16));
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

type ProcessMatcherResult = Arc<(Vec<&'static str>, ProcessMatcher)>;

/// A lock-free, lazily-initialized array mapping bit positions to process matchers.
///
/// Maps the bit position of a single-bit [`ProcessType`] to an [`Arc`] instance holding
/// a tuple of a replacement-string list and a `ProcessMatcher`.
///
/// The array has a capacity of 8, covering all possible bits in the 8-bit [`ProcessType`].
pub static PROCESS_MATCHER_CACHE: [OnceLock<ProcessMatcherResult>; 8] = [
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
    OnceLock::new(),
];

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
/// replace or delete specific patterns.
///
/// # Variants
/// * `DAAC` - Uses a [`daachorse::CharwiseDoubleArrayAhoCorasick<u32>`] matcher for complex, overlapping transformations (e.g., Normalize).
/// * `AC` - Uses a standard [`AhoCorasick`] matcher for general-purpose string matching.
/// * `Fanjian` - Uses a **2-Stage Page Table** for ultra-fast, $O(1)$ Traditional-to-Simplified Chinese conversion.
/// * `Pinyin` - Uses a **2-Stage Page Table** and packed buffer for $O(1)$ character-to-pinyin conversion.
/// * `Delete` - Uses a **Flat BitSet** for extremely fast character deletion across the full Unicode range.
#[derive(Clone)]
pub enum ProcessMatcher {
    #[cfg(not(feature = "dfa"))]
    DAAC(CharwiseDoubleArrayAhoCorasick<u32>),
    AC(AhoCorasick),
    SingleChar(SingleCharMatcher),
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
            ProcessMatcher::SingleChar(matcher) => match matcher {
                SingleCharMatcher::Delete { .. } => unreachable!(),
                _ => {
                    let mut iter = matcher.find_iter(text);
                    if let Some((start, end, m)) = iter.next() {
                        let mut result = get_string_from_pool(text.len());
                        result.push_str(&text[0..start]);
                        match m {
                            SingleCharMatch::Char(c) => result.push(c),
                            SingleCharMatch::Str(s) => result.push_str(s),
                            SingleCharMatch::Delete => {}
                        }
                        let mut last_end = end;
                        for (start, end, m) in iter {
                            result.push_str(&text[last_end..start]);
                            match m {
                                SingleCharMatch::Char(c) => result.push(c),
                                SingleCharMatch::Str(s) => result.push_str(s),
                                SingleCharMatch::Delete => {}
                            }
                            last_end = end;
                        }
                        result.push_str(&text[last_end..]);
                        return (true, Cow::Owned(result));
                    }
                    return (false, Cow::Borrowed(text));
                }
            },
            #[cfg(not(feature = "dfa"))]
            ProcessMatcher::DAAC(ac) => do_replace!(
                ac.leftmost_find_iter(text),
                |m: &daachorse::Match<u32>| m.value() as usize
            ),
            ProcessMatcher::AC(ac) => {
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
            ProcessMatcher::SingleChar(matcher) => match matcher {
                SingleCharMatcher::Delete { .. } => {
                    let mut iter = matcher.find_iter(text);
                    if let Some((start, end, _)) = iter.next() {
                        let mut result = get_string_from_pool(text.len());
                        result.push_str(&text[0..start]);
                        let mut last_end = end;
                        for (start, end, _) in iter {
                            result.push_str(&text[last_end..start]);
                            last_end = end;
                        }
                        result.push_str(&text[last_end..]);
                        return (true, Cow::Owned(result));
                    }
                    return (false, Cow::Borrowed(text));
                }
                _ => unreachable!(),
            },
            #[cfg(not(feature = "dfa"))]
            ProcessMatcher::DAAC(ac) => do_delete!(ac.leftmost_find_iter(text)),
            ProcessMatcher::AC(ac) => do_delete!(ac.find_iter(text)),
        }
        (false, Cow::Borrowed(text))
    }
}

/// Retrieves or constructs a `ProcessMatcher` for a given single-bit [`ProcessType`].
///
/// ### Transformation Strategy:
/// 1. **Normalize**: Uses `daachorse` (Double-Array Aho-Corasick) or a standard Aho-Corasick DFA
///    to handle overlapping multi-character patterns (like Unicode combining marks).
/// 2. **Fanjian, Pinyin, PinyinChar**: Uses a **2-Stage Page Table** system for $O(1)$ lookups.
///    This eliminates the state-machine overhead for 1-to-1 or 1-to-N character mappings.
/// 3. **Delete**: Uses a **Global BitSet** covering all Unicode planes for branchless $O(1)$ filtering.
///
/// ### Algorithm
/// 1. Checks `PROCESS_MATCHER_CACHE`. If exists, returns cloned `Arc`.
/// 2. If missing, configures the appropriate optimized structure based on `ProcessType`.
///    - If `runtime_build` is enabled, structures are built dynamically from text files.
///    - Otherwise, static pre-compiled binary structures are loaded via zero-copy includes.
/// 3. Safely initializes the cache entry and returns.
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
    let index = process_type_bit.bits().trailing_zeros() as usize;
    debug_assert!(index < 8, "ProcessType bit index out of bounds");

    Arc::clone(PROCESS_MATCHER_CACHE[index].get_or_init(|| {
        #[cfg(feature = "runtime_build")]
        {
            fn build_2_stage_table_runtime(map: &HashMap<u32, u32>) -> (Vec<u8>, Vec<u8>) {
                let mut pages = HashSet::new();
                for &k in map.keys() {
                    pages.insert(k >> 8);
                }
                let mut page_list: Vec<u32> = pages.into_iter().collect();
                page_list.sort_unstable();
                let mut l1 = vec![0u16; 4352];
                let mut l2 = vec![0u32; (page_list.len() + 1) * 256];
                for (i, &page) in page_list.iter().enumerate() {
                    let l2_page_idx = (i + 1) as u16;
                    l1[page as usize] = l2_page_idx;
                    for char_idx in 0..256 {
                        let cp = (page << 8) | char_idx;
                        if let Some(&val) = map.get(&cp) {
                            l2[(l2_page_idx as usize * 256) + char_idx as usize] = val;
                        }
                    }
                }
                let mut l1_bytes = Vec::with_capacity(l1.len() * 2);
                for val in l1 {
                    l1_bytes.extend_from_slice(&val.to_le_bytes());
                }
                let mut l2_bytes = Vec::with_capacity(l2.len() * 4);
                for val in l2 {
                    l2_bytes.extend_from_slice(&val.to_le_bytes());
                }
                (l1_bytes, l2_bytes)
            }

            let process_matcher = match process_type_bit {
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
                    let (l1, l2) = build_2_stage_table_runtime(&map);
                    ProcessMatcher::SingleChar(SingleCharMatcher::Fanjian {
                        l1: Cow::Owned(l1),
                        l2: Cow::Owned(l2),
                    })
                }
                ProcessType::PinYin | ProcessType::PinYinChar => {
                    let mut map = HashMap::new();
                    let mut strings = String::new();
                    for line in PINYIN.trim().lines() {
                        let mut split = line.split('\t');
                        let k = split.next().unwrap().chars().next().unwrap() as u32;
                        let v = split.next().unwrap();
                        let offset = strings.len();
                        strings.push_str(v);
                        let length = v.len();
                        map.insert(k, ((offset as u32) << 8) | (length as u32));
                    }
                    let (l1, l2) = build_2_stage_table_runtime(&map);
                    ProcessMatcher::SingleChar(SingleCharMatcher::Pinyin {
                        l1: Cow::Owned(l1),
                        l2: Cow::Owned(l2),
                        strings: Cow::Owned(strings),
                        trim_space: process_type_bit == ProcessType::PinYinChar,
                    })
                }
                ProcessType::Delete => {
                    let mut bitset = vec![0u8; 139264];
                    for line in TEXT_DELETE.trim().lines() {
                        for c in line.chars() {
                            let cp = c as usize;
                            bitset[cp / 8] |= 1 << (cp % 8);
                        }
                    }
                    for &val in WHITE_SPACE {
                        for c in val.chars() {
                            let cp = c as usize;
                            bitset[cp / 8] |= 1 << (cp % 8);
                        }
                    }
                    ProcessMatcher::SingleChar(SingleCharMatcher::Delete {
                        bitset: Cow::Owned(bitset),
                    })
                }
                ProcessType::Normalize => {
                    let mut process_dict = HashMap::new();
                    for process_map in [NORM, NUM_NORM] {
                        process_dict.extend(process_map.trim().lines().map(|pair_str| {
                            let mut pair_str_split = pair_str.split('\t');
                            (
                                pair_str_split.next().unwrap(),
                                pair_str_split.next().unwrap(),
                            )
                        }));
                    }
                    process_dict.retain(|&key, &mut value| key != value);
                    let mut keys: Vec<&str> = process_dict.keys().copied().collect();
                    keys.sort_unstable();
                    #[cfg(not(feature = "dfa"))]
                    {
                        ProcessMatcher::DAAC(
                            CharwiseDoubleArrayAhoCorasickBuilder::new()
                                .match_kind(DoubleArrayAhoCorasickMatchKind::LeftmostLongest)
                                .build(&keys)
                                .unwrap(),
                        )
                    }
                    #[cfg(feature = "dfa")]
                    {
                        ProcessMatcher::AC(
                            AhoCorasickBuilder::new()
                                .kind(Some(AhoCorasickKind::DFA))
                                .match_kind(AhoCorasickMatchKind::LeftmostLongest)
                                .build(&keys)
                                .unwrap(),
                        )
                    }
                }
                _ => ProcessMatcher::AC(AhoCorasick::new(Vec::<&str>::new()).unwrap()),
            };

            let process_replace_list = match process_type_bit {
                ProcessType::Normalize => {
                    let mut process_dict = HashMap::new();
                    for process_map in [NORM, NUM_NORM] {
                        process_dict.extend(process_map.trim().lines().map(|pair_str| {
                            let mut pair_str_split = pair_str.split('\t');
                            (
                                pair_str_split.next().unwrap(),
                                pair_str_split.next().unwrap(),
                            )
                        }));
                    }
                    process_dict.retain(|&key, &mut value| key != value);
                    let mut pairs: Vec<(&str, &str)> = process_dict.into_iter().collect();
                    pairs.sort_unstable_by_key(|&(k, _)| k);
                    pairs.into_iter().map(|(_, v)| v).collect()
                }
                _ => Vec::new(),
            };

            Arc::new((process_replace_list, process_matcher))
        }

        #[cfg(not(feature = "runtime_build"))]
        {
            let (process_replace_list, process_matcher) = match process_type_bit {
                ProcessType::None => {
                    let empty_patterns: Vec<&str> = Vec::new();
                    (
                        Vec::new(),
                        ProcessMatcher::AC(AhoCorasick::new(&empty_patterns).unwrap()),
                    )
                }
                ProcessType::Fanjian => (
                    Vec::new(),
                    ProcessMatcher::SingleChar(SingleCharMatcher::Fanjian {
                        l1: Cow::Borrowed(FANJIAN_L1_BYTES),
                        l2: Cow::Borrowed(FANJIAN_L2_BYTES),
                    }),
                ),
                ProcessType::Delete => (
                    Vec::new(),
                    ProcessMatcher::SingleChar(SingleCharMatcher::Delete {
                        bitset: Cow::Borrowed(DELETE_BITSET_BYTES),
                    }),
                ),
                ProcessType::Normalize => {
                    #[cfg(feature = "dfa")]
                    {
                        (
                            NORMALIZE_PROCESS_REPLACE_LIST_STR.lines().collect(),
                            ProcessMatcher::AC(
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
                            ProcessMatcher::DAAC(unsafe {
                                CharwiseDoubleArrayAhoCorasick::<u32>::deserialize_unchecked(
                                    NORMALIZE_PROCESS_MATCHER_BYTES,
                                )
                                .0
                            }),
                        )
                    }
                }
                ProcessType::PinYin => (
                    Vec::new(),
                    ProcessMatcher::SingleChar(SingleCharMatcher::Pinyin {
                        l1: Cow::Borrowed(PINYIN_L1_BYTES),
                        l2: Cow::Borrowed(PINYIN_L2_BYTES),
                        strings: Cow::Borrowed(PINYIN_STR_BYTES),
                        trim_space: false,
                    }),
                ),
                ProcessType::PinYinChar => (
                    Vec::new(),
                    ProcessMatcher::SingleChar(SingleCharMatcher::Pinyin {
                        l1: Cow::Borrowed(PINYIN_L1_BYTES),
                        l2: Cow::Borrowed(PINYIN_L2_BYTES),
                        strings: Cow::Borrowed(PINYIN_STR_BYTES),
                        trim_space: true,
                    }),
                ),
                _ => unreachable!(),
            };
            Arc::new((process_replace_list, process_matcher))
        }
    }))
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
/// process types (`process_type_list`), and its child nodes (`children`).
///
/// # Algorithm
/// 1. Nodes represent isolated mutation states during text reductions.
/// 2. As bits from a composite `ProcessType` are evaluated sequentially, they form directional edges to child nodes.
///
/// # Fields
///
/// * `process_type_list` - An [`Vec`] containing the list of processing types associated with this node.
/// * `process_type_bit` - A [`ProcessType`] representing the specific processing rule for this node.
/// * `children` - An [`Vec`] containing the indices of child nodes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProcessTypeBitNode {
    process_type_list: Vec<ProcessType>,
    process_type_bit: ProcessType,
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
///    and transformation passes.
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
    REDUCE_STATE.with(|state| {
        let mut node_processed_indices = state.borrow_mut();
        node_processed_indices.clear();
        node_processed_indices.resize(process_type_tree.len(), 0);

        let mut processed_text_process_type_masks: ProcessedTextMasks<'a> = Vec::new();
        processed_text_process_type_masks
            .push((Cow::Borrowed(text), 1u64 << ProcessType::None.bits()));

        for (current_node_index, current_node) in process_type_tree.iter().enumerate() {
            let current_index = node_processed_indices[current_node_index];

            for &child_node_index in &current_node.children {
                let child_node = &process_type_tree[child_node_index];
                let mut child_index = current_index;

                let cached_result = get_process_matcher(child_node.process_type_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match child_node.process_type_bit {
                    ProcessType::None => {}
                    ProcessType::Delete => {
                        let current_text =
                            processed_text_process_type_masks[current_index].0.as_ref();
                        match process_matcher.delete_all(current_text) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_process_type_masks.push((Cow::Owned(pt), 0u64));
                                child_index = processed_text_process_type_masks.len() - 1;
                            }
                            (false, _) => {
                                child_index = current_index;
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
                                child_index = processed_text_process_type_masks.len() - 1;
                            }
                            (false, _) => {
                                child_index = current_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                }

                node_processed_indices[child_node_index] = child_index;
                let processed_text_process_type_tuple =
                    &mut processed_text_process_type_masks[child_index];
                processed_text_process_type_tuple.1 |= child_node
                    .process_type_list
                    .iter()
                    .fold(0u64, |mask, smt| mask | (1u64 << smt.bits()));
            }
        }

        processed_text_process_type_masks
    })
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
    let root = ProcessTypeBitNode {
        process_type_list: Vec::new(),
        process_type_bit: ProcessType::None,
        children: Vec::new(),
    };
    process_type_tree.push(root);

    let mut node_processed_indices = Vec::with_capacity(8);
    node_processed_indices.push(0);

    let mut processed_text_process_type_masks: ProcessedTextMasks<'a> = Vec::new();
    processed_text_process_type_masks.push((Cow::Borrowed(text), 1u64 << ProcessType::None.bits()));

    for process_type_bits in process_type_set.iter() {
        let process_type = ProcessType::from_bits(*process_type_bits).unwrap();
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

            if !is_found {
                let current_index = node_processed_indices[current_node_index];
                let mut child_index = current_index;

                let cached_result = get_process_matcher(process_type_bit);
                let (process_replace_list, process_matcher) = cached_result.as_ref();

                match process_type_bit {
                    ProcessType::None => {}
                    ProcessType::Delete => {
                        let current_text =
                            processed_text_process_type_masks[current_index].0.as_ref();
                        match process_matcher.delete_all(current_text) {
                            (true, Cow::Owned(pt)) => {
                                processed_text_process_type_masks.push((Cow::Owned(pt), 0u64));
                                child_index = processed_text_process_type_masks.len() - 1;
                            }
                            (false, _) => {
                                child_index = current_index;
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
                                child_index = processed_text_process_type_masks.len() - 1;
                            }
                            (false, _) => {
                                child_index = current_index;
                            }
                            (_, _) => unreachable!(),
                        }
                    }
                }

                let mut child = ProcessTypeBitNode {
                    process_type_list: Vec::new(),
                    process_type_bit,
                    children: Vec::new(),
                };
                child.process_type_list.push(process_type);
                process_type_tree.push(child);
                node_processed_indices.push(child_index);

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

            let current_index = node_processed_indices[current_node_index];
            let processed_text_process_type_tuple =
                &mut processed_text_process_type_masks[current_index];
            processed_text_process_type_tuple.1 |= 1u64 << process_type.bits();
        }
    }

    processed_text_process_type_masks
}
