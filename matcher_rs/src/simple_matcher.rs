use std::fmt::Display;
use std::iter;
use std::{borrow::Cow, collections::HashMap};

use ahash::AHashMap;
use aho_corasick_unsafe::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use bitflags::bitflags;
use nohash_hasher::{IntMap, IntSet, IsEnabled};
use serde::{Deserializer, Serializer};
use sonic_rs::{Deserialize, Serialize};

use crate::matcher::{MatchResultTrait, TextMatcherTrait};
use crate::process::process_matcher::{
    build_smt_tree, reduce_text_process_emit, reduce_text_process_with_tree, SimpleMatchTypeBitNode,
};

bitflags! {
    /// [SimpleMatchType] is a set of flags used to specify various text transformation rules.
    ///
    /// Each flag represents a specific type of string conversion or deletion operation.
    /// The flags can be combined using bitwise operations to create complex transformation rules.
    ///
    /// # Flags
    ///
    /// * [None](SimpleMatchType::None) (0b00000001) - No transformation is applied.
    /// * [Fanjian](SimpleMatchType::Fanjian) (0b00000010) - Simplifies traditional Chinese characters to simplified ones.
    /// * [WordDelete](SimpleMatchType::WordDelete) (0b00000100) - Deletes word-level components based on predefined rules.
    /// * [TextDelete](SimpleMatchType::TextDelete) (0b00001000) - Deletes text-level components, including special characters and whitespace.
    /// * [Delete](SimpleMatchType::Delete) (0b00001100) - Combines [WordDelete](SimpleMatchType::WordDelete) and [TextDelete](SimpleMatchType::TextDelete) transformations.
    /// * [Normalize](SimpleMatchType::Normalize) (0b00010000) - Normalizes the text, including case normalization and removing variations.
    /// * [DeleteNormalize](SimpleMatchType::DeleteNormalize) (0b00011100) - Combines [Delete](SimpleMatchType::Delete) and [Normalize](SimpleMatchType::Normalize) transformations.
    /// * [FanjianDeleteNormalize](SimpleMatchType::FanjianDeleteNormalize) (0b00011110) - Combines [Fanjian](SimpleMatchType::Fanjian), [Delete](SimpleMatchType::Delete), and [Normalize](SimpleMatchType::Normalize) transformations.
    /// * [PinYin](SimpleMatchType::PinYin) (0b00100000) - Converts Chinese characters to their Pinyin representation.
    /// * [PinYinChar](SimpleMatchType::PinYinChar) (0b01000000) - Converts individual Chinese characters to their Pinyin representation.
    #[derive(Hash, PartialEq, Eq, Clone, Copy, Debug, Default)]
    pub struct SimpleMatchType: u8 {
        const None = 0b00000001;
        const Fanjian = 0b00000010;
        const WordDelete = 0b00000100;
        const TextDelete = 0b00001000;
        const Delete = 0b00001100;
        const Normalize = 0b00010000;
        const DeleteNormalize = 0b00011100;
        const FanjianDeleteNormalize = 0b00011110;
        const PinYin = 0b00100000;
        const PinYinChar = 0b01000000;
    }
}

impl Serialize for SimpleMatchType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.bits().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SimpleMatchType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bits: u8 = u8::deserialize(deserializer)?;
        Ok(SimpleMatchType::from_bits_retain(bits))
    }
}

impl Display for SimpleMatchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let display_str_list = self
            .iter_names()
            .map(|(name, _)| name.to_lowercase())
            .collect::<Vec<_>>();
        write!(f, "{:?}", display_str_list.join("_"))
    }
}

impl IsEnabled for SimpleMatchType {}

pub type SimpleMatchTypeWordMap<'a> = IntMap<SimpleMatchType, IntMap<u32, &'a str>>;

/// `WordConf` represents the configuration and attributes of a specific word,
/// including its textual representation, split bit vector, and a non-indexable position.
///
/// This structure is essential for configuring words that will be processed by the
/// [SimpleMatcher] for pattern matching and text transformations. The `word` field holds
/// the actual text of the word, `split_bit` contains the vector for split bits, and
/// `not_index` indicates a specific position that should not be indexed during the matching process.
///
/// # Fields
///
/// * `word` - A [String] representing the textual content of the word.
/// * `split_bit` - A [`Vec<i32>`] representing the vector that holds split bits for the word.
/// * `not_index` - A [usize] denoting a position in the word that is exempt from indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WordConf {
    word: String,
    split_bit: Vec<i32>,
    not_index: usize,
}

/// [SimpleResult] represents the result of a matching operation.
///
/// This structure is used to store the outcome of a text matching operation performed
/// by the [SimpleMatcher]. It holds details about the matched word, including its
/// unique identifier (`word_id`) and the matched text (`word`). The [SimpleResult]
/// structure is designed to provide a consistent and accessible interface for retrieving
/// the results of text matching operations.
///
/// # Fields
///
/// * `word_id` - A [u32] value representing the unique identifier of the matched word.
/// * `word` - A [Cow<'a, str>] representing the matched text. This allows the text to be
///   either borrowed or owned, providing flexibility in handling the string data.
///
/// # Example
///
/// ```
/// use matcher_rs::{SimpleResult, MatchResultTrait};
/// use std::borrow::Cow;
///
/// let result = SimpleResult {
///     word_id: 42,
///     word: Cow::Borrowed("example"),
/// };
///
/// assert_eq!(result.word_id(), 42);
/// ```
#[derive(Debug, Serialize)]
pub struct SimpleResult<'a> {
    pub word_id: u32,
    pub word: Cow<'a, str>,
}

impl MatchResultTrait<'_> for SimpleResult<'_> {
    fn word_id(&self) -> u32 {
        self.word_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

/// [SimpleMatcher] is a structure designed for efficient pattern matching and text transformations.
///
/// The [SimpleMatcher] structure encapsulates various configurations, matchers, and nodes needed to
/// perform text matching operations efficiently. It uses different matching rules defined by the
/// [SimpleMatchType] and builds necessary data structures for pattern matching, including an Aho-Corasick
/// automaton for fast multi-pattern matching.
///
/// # Fields
///
/// * `smt_tree` - A [Vec] of [SimpleMatchTypeBitNode] that represents the match type tree for hierarchical
///   or complex match type relationships.
/// * `smt_matcher` - An [AhoCorasick] matcher that facilitates the multi-pattern matching based on the configured
///   match types and word patterns.
/// * `smt_ac_dedup_word_conf_list` - A [Vec] of lists containing tuples of [SimpleMatchType], word ID [u32], and
///   a size [usize] that helps in deduplication of word configurations for the matcher.
/// * `simple_word_conf_map` - An [IntMap] that maps word IDs [u32] to their corresponding `WordConf` structures,
///   providing configuration details for each word.
///
/// The [SimpleMatcher] is typically initialized and configured using the provided word maps and match types,
/// and it is used to perform fast and reliable text matching operations in various applications.
///
/// # Example
///
/// ```
/// use std::collections::HashMap;
/// use matcher_rs::{SimpleMatcher, SimpleMatchType, TextMatcherTrait};
///
/// // Initialize word maps and SimpleMatchType instances.
/// let word_maps = HashMap::from([
///     (SimpleMatchType::Fanjian, HashMap::from([(1, "ChineseWord1"), (2, "ChineseWord2")])),
///     (SimpleMatchType::Normalize, HashMap::from([(3, "NormalizationExample1"), (4, "NormalizationExample2")]))
/// ]);
///
/// // Create a SimpleMatcher instance using the provided word maps.
/// let simple_matcher = SimpleMatcher::new(&word_maps);
///
/// // Check if a text matches any patterns based on the configured SimpleMatcher.
/// let text = "ExampleText";
/// let is_match = simple_matcher.is_match(text);
///
/// // Process the input text and return a list of matching results.
/// let results = simple_matcher.process(text);
/// ```
///
/// # See also:
///
/// * [SimpleMatchType] - Enum defining various match types and their respective flags.
/// * [SimpleMatcher::new] - Method to initialize a new `SimpleMatcher` instance.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SimpleMatcher {
    smt_tree: Vec<SimpleMatchTypeBitNode>,
    smt_matcher: AhoCorasick,
    smt_ac_dedup_word_conf_list: Vec<Vec<(SimpleMatchType, u32, usize)>>,
    simple_word_conf_map: IntMap<u32, WordConf>,
}

impl SimpleMatcher {
    /// Constructs a new `SimpleMatcher` instance from a provided map of `SimpleMatchType` to word maps.
    ///
    /// This function initializes a `SimpleMatcher` with mappings and configurations needed for efficient
    /// text matching based on the provided `SimpleMatchType` rules. It creates the necessary structures for
    /// pattern matching, including Aho-Corasick tables and word configuration mappings.
    ///
    /// # Arguments
    ///
    /// * `smt_word_map` - A reference to a `HashMap` where keys are `SimpleMatchType` and values
    ///   are `HashMap` of word IDs to their corresponding words.
    ///
    /// # Type Parameters
    ///
    /// * `I` - A type that can be referenced as a string slice. This represents the type of the words in the map.
    /// * `S1` - A hasher for the inner `HashMap` keys (word IDs).
    /// * `S2` - A hasher for the outer `HashMap` keys (`SimpleMatchType`).
    ///
    /// # Returns
    ///
    /// * `SimpleMatcher` - A configured `SimpleMatcher` instance ready for pattern matching.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use matcher_rs::{SimpleMatcher, SimpleMatchType};
    ///
    /// let smt_word_map = HashMap::from([
    ///     (SimpleMatchType::Fanjian, HashMap::from([(1, "example1"), (2, "example2")])),
    ///     (SimpleMatchType::Normalize, HashMap::from([(3, "example3"), (4, "example4")])),
    /// ]);
    ///
    /// let simple_matcher = SimpleMatcher::new(&smt_word_map);
    /// ```
    ///
    /// # Detailed Processing:
    ///
    /// 1. Collects and copies the keys from `smt_word_map` to create `smt_list`.
    /// 2. If the length of `smt_word_map` is 4 or more, builds the `smt_tree` using `build_smt_tree`.
    /// 3. Initializes empty vectors and maps for storing configurations and deduplication.
    /// 4. Iterates over each `SimpleMatchType` and its corresponding word map:
    ///     a. For each word, splits it based on '&' and '~' characters to separate the included and excluded parts.
    ///     b. Processes the split words and updates counters for both included and excluded parts.
    ///     c. Inserts the word configurations into `simple_word_conf_map`.
    ///     d. Processes and reduces the text for the Aho-Corasick matcher, updating the deduplication maps.
    /// 5. Chooses the Aho-Corasick matcher kind and prefilter settings based on feature flags.
    /// 6. Builds the Aho-Corasick matcher using the processed and reduced text words.
    /// 7. Returns a new `SimpleMatcher` instance with the initialized structures.
    ///
    pub fn new<I, S1, S2>(
        smt_word_map: &HashMap<SimpleMatchType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str>,
    {
        let mut smt_list = Vec::new();
        let mut smt_ac_dedup_word_conf_list = Vec::new();
        let mut simple_word_conf_map = IntMap::default();

        let mut ac_dedup_word_id = 0;
        let mut ac_dedup_word_list = Vec::new();
        let mut ac_dedup_word_id_map = AHashMap::new();

        for (&simple_match_type, simple_word_map) in smt_word_map {
            let word_simple_match_type = simple_match_type - SimpleMatchType::TextDelete;
            let text_simple_match_type = simple_match_type - SimpleMatchType::WordDelete;

            smt_list.push(text_simple_match_type);

            for (&simple_word_id, simple_word) in simple_word_map {
                let mut ac_split_word_and_counter = AHashMap::default();
                let mut ac_split_word_not_counter = AHashMap::default();

                let mut start = 0;
                let mut is_and = false;
                let mut is_not = false;

                for (index, char) in simple_word.as_ref().match_indices(['&', '~']) {
                    if (is_and || start == 0) && start != index {
                        ac_split_word_and_counter
                            // Guaranteed not failed
                            .entry(unsafe { simple_word.as_ref().get_unchecked(start..index) })
                            .and_modify(|cnt| *cnt += 1)
                            .or_insert(1);
                    }
                    if is_not && start != index {
                        ac_split_word_not_counter
                            // Guaranteed not failed
                            .entry(unsafe { simple_word.as_ref().get_unchecked(start..index) })
                            .and_modify(|cnt| *cnt -= 1)
                            .or_insert(0);
                    }
                    match char {
                        "&" => {
                            is_and = true;
                            is_not = false;
                            start = index + 1;
                        }
                        "~" => {
                            is_and = false;
                            is_not = true;
                            start = index + 1
                        }
                        _ => {}
                    }
                }
                if (is_and || start == 0) && start != simple_word.as_ref().len() {
                    ac_split_word_and_counter
                        // Guaranteed not failed
                        .entry(unsafe { simple_word.as_ref().get_unchecked(start..) })
                        .and_modify(|cnt| *cnt += 1)
                        .or_insert(1);
                }
                if is_not && start != simple_word.as_ref().len() {
                    ac_split_word_not_counter
                        // Guaranteed not failed
                        .entry(unsafe { simple_word.as_ref().get_unchecked(start..) })
                        .and_modify(|cnt| *cnt -= 1)
                        .or_insert(0);
                }

                let not_index = ac_split_word_and_counter.len();
                let split_bit = ac_split_word_and_counter
                    .values()
                    .copied()
                    .chain(ac_split_word_not_counter.values().copied())
                    .collect::<Vec<i32>>();

                simple_word_conf_map.insert(
                    simple_word_id,
                    WordConf {
                        word: simple_word.as_ref().to_owned(),
                        split_bit,
                        not_index,
                    },
                );

                for (offset, &split_word) in ac_split_word_and_counter
                    .keys()
                    .chain(ac_split_word_not_counter.keys())
                    .enumerate()
                {
                    for ac_word in reduce_text_process_emit(word_simple_match_type, split_word) {
                        if let Some(ac_dedup_word_id) = ac_dedup_word_id_map.get(ac_word.as_ref()) {
                            // Guaranteed not failed
                            let word_conf_list: &mut Vec<(SimpleMatchType, u32, usize)> = unsafe {
                                smt_ac_dedup_word_conf_list
                                    .get_unchecked_mut(*ac_dedup_word_id as usize)
                            };
                            word_conf_list.push((text_simple_match_type, simple_word_id, offset));
                        } else {
                            ac_dedup_word_id_map.insert(ac_word.clone(), ac_dedup_word_id);
                            smt_ac_dedup_word_conf_list.push(vec![(
                                text_simple_match_type,
                                simple_word_id,
                                offset,
                            )]);
                            ac_dedup_word_list.push(ac_word);
                            ac_dedup_word_id += 1;
                        }
                    }
                }
            }
        }

        let smt_tree = build_smt_tree(&smt_list);

        #[cfg(feature = "dfa")]
        let aho_corasick_kind = AhoCorasickKind::DFA;
        #[cfg(not(feature = "dfa"))]
        let aho_corasick_kind = AhoCorasickKind::ContiguousNFA;

        #[cfg(feature = "serde")]
        let prefilter = false;
        #[cfg(not(feature = "serde"))]
        let prefilter = true;

        let smt_matcher = AhoCorasickBuilder::new()
            .kind(Some(aho_corasick_kind))
            .ascii_case_insensitive(true)
            .prefilter(prefilter)
            .build(ac_dedup_word_list.iter().map(|ac_word| ac_word.as_ref()))
            .unwrap();

        SimpleMatcher {
            smt_tree,
            smt_matcher,
            smt_ac_dedup_word_conf_list,
            simple_word_conf_map,
        }
    }
}

impl<'a> TextMatcherTrait<'a, SimpleResult<'a>> for SimpleMatcher {
    /// Checks if the input text contains any matches based on the patterns stored in the matcher.
    ///
    /// This function returns a boolean indicating whether any patterns are found within the input text.
    /// It processes the input text according to transformations defined by each [SimpleMatchType],
    /// and utilizes the Aho-Corasick algorithm to find overlapping patterns. If at least one match is found
    /// according to the configurations, the function returns `true`.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be checked for matches.
    ///
    /// # Returns
    ///
    /// * `bool` - `true` if at least one match is found, `false` otherwise.
    ///
    /// # Detailed Processing:
    ///
    /// 1. If the input text is empty, return `false`.
    /// 2. Initialize maps and sets to track word configurations during processing, including:
    ///     * `word_id_split_bit_map`: A map to track the bit matrices for word configurations.
    ///     * `word_id_set`: A set to track word IDs that have a valid match.
    ///     * `not_word_id_set`: A set to track word IDs that should be excluded.
    /// 3. Process the input text using `reduce_text_process_with_tree` to get transformed versions
    ///    and corresponding [SimpleMatchType] sets.
    /// 4. Iterate through the processed text and corresponding sets:
    ///     a. Use the Aho-Corasick matcher to find overlapping patterns.
    ///     b. For each match, update the bit matrices according to the configurations.
    ///     c. Check if the match should be excluded based on the not set or existing configurations.
    ///     d. If a valid match is found according to the bit matrices, add it to the `word_id_set`.
    /// 5. If `word_id_set` is not empty after processing, return `true`.
    /// 6. Return `false` if no valid matches are found.
    fn is_match(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }

        let mut word_id_split_bit_map = IntMap::default();
        let mut word_id_set = IntSet::default();
        let mut not_word_id_set = IntSet::default();

        let processed_text_smt_list = reduce_text_process_with_tree(&self.smt_tree, text);
        let processed_times = processed_text_smt_list.len();

        for (index, (processed_text, smt_set)) in processed_text_smt_list.iter().enumerate() {
            // Guaranteed not failed
            for ac_dedup_result in unsafe {
                self.smt_matcher
                    .try_find_overlapping_iter(processed_text.as_ref())
                    .unwrap_unchecked()
            } {
                // Guaranteed not failed
                for &(match_simple_match_type, word_id, offset) in unsafe {
                    self.smt_ac_dedup_word_conf_list
                        .get_unchecked(ac_dedup_result.pattern().as_usize())
                } {
                    if !smt_set.contains(match_simple_match_type.bits() as usize)
                        || not_word_id_set.contains(&word_id)
                    {
                        continue;
                    }

                    // Guaranteed not failed
                    let word_conf =
                        unsafe { self.simple_word_conf_map.get(&word_id).unwrap_unchecked() };

                    let split_bit_matrix =
                        word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                            word_conf
                                .split_bit
                                .iter()
                                .map(|&bit| iter::repeat(bit).take(processed_times).collect())
                                .collect::<Vec<Vec<i32>>>()
                        });

                    // bit is i32, so it will not overflow almost 100%
                    unsafe {
                        let bit = split_bit_matrix
                            .get_unchecked_mut(offset)
                            .get_unchecked_mut(index);
                        *bit = bit.unchecked_add((offset < word_conf.not_index) as i32 * -2 + 1);

                        if offset >= word_conf.not_index && *bit > 0 {
                            not_word_id_set.insert(word_id);
                            word_id_set.remove(&word_id);
                            continue;
                        }

                        if split_bit_matrix
                            .iter()
                            .all(|split_bit_vec| split_bit_vec.iter().any(|&bit| bit <= 0))
                        {
                            word_id_set.insert(word_id);
                        }
                    }
                }
            }
            if !word_id_set.is_empty() {
                return true;
            }
        }

        false
    }

    ///
    /// This function processes the input text and returns a vector of [SimpleResult] containing word matches
    /// found within the text. It utilizes transformations defined by each [SimpleMatchType] and utilizes
    /// the Aho-Corasick algorithm to identify overlapping patterns.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be checked for matches.
    ///
    /// # Returns
    ///
    /// * [`Vec<SimpleResult>`] - A vector containing [SimpleResult] objects, each containing a `word_id` and `word`
    /// indicating a valid match found within the input text.
    ///
    /// # Detailed Processing:
    ///
    /// 1. If the input text is empty, return an empty vector.
    /// 2. Initialize maps and sets to track word configurations during processing, including:
    ///     * `word_id_split_bit_map`: A map to track the bit matrices for word configurations.
    ///     * `not_word_id_set`: A set to track word IDs that should be excluded.
    /// 3. Process the input text using `reduce_text_process_with_tree` to get transformed versions
    ///    and corresponding [SimpleMatchType] sets.
    /// 4. Iterate through the processed text and corresponding sets:
    ///     a. Use the Aho-Corasick matcher to find overlapping patterns.
    ///     b. For each match, update the bit matrices according to the configurations.
    ///     c. Check if the match should be excluded based on the not set or existing configurations.
    /// 5. Filter out and collect valid matches into a vector of [SimpleResult]:
    ///     * A match is considered valid if it satisfies the bit matrix configurations and
    ///       is not present in the `not_word_id_set`.
    ///
    /// # Safety
    ///
    /// The function uses several `unsafe` blocks for performance reasons, assuming that:
    /// * The iterator over the processed text will not fail.
    /// * The configurations for word ID and bit matrices are valid and properly aligned.
    /// * Accessing elements in maps and vectors using unchecked indexing will not lead to out-of-bound errors.
    ///
    /// Use of these `unsafe` blocks is carefully justified to ensure efficient processing and is based
    /// on guarantees provided either by the input text and configuration maps or the logical structure
    /// of the program.
    fn process(&'a self, text: &str) -> Vec<SimpleResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut word_id_split_bit_map = IntMap::default();
        let mut not_word_id_set = IntSet::default();

        let processed_text_smt_list = reduce_text_process_with_tree(&self.smt_tree, text);
        let processed_times = processed_text_smt_list.len();

        for (index, (processed_text, smt_set)) in processed_text_smt_list.iter().enumerate() {
            // Guaranteed not failed
            for ac_dedup_result in unsafe {
                self.smt_matcher
                    .try_find_overlapping_iter(processed_text.as_ref())
                    .unwrap_unchecked()
            } {
                // Guaranteed not failed
                for &(match_simple_match_type, word_id, offset) in unsafe {
                    self.smt_ac_dedup_word_conf_list
                        .get_unchecked(ac_dedup_result.pattern().as_usize())
                } {
                    if !smt_set.contains(match_simple_match_type.bits() as usize)
                        || not_word_id_set.contains(&word_id)
                    {
                        continue;
                    }

                    // Guaranteed not failed
                    let word_conf =
                        unsafe { self.simple_word_conf_map.get(&word_id).unwrap_unchecked() };

                    let split_bit_matrix =
                        word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                            word_conf
                                .split_bit
                                .iter()
                                .map(|&bit| iter::repeat(bit).take(processed_times).collect())
                                .collect::<Vec<Vec<i32>>>()
                        });

                    // split_bit is i32, so it will not overflow almost 100%
                    unsafe {
                        let split_bit = split_bit_matrix
                            .get_unchecked_mut(offset)
                            .get_unchecked_mut(index);
                        *split_bit =
                            split_bit.unchecked_add((offset < word_conf.not_index) as i32 * -2 + 1);

                        if offset >= word_conf.not_index && *split_bit > 0 {
                            not_word_id_set.insert(word_id);
                            word_id_split_bit_map.remove(&word_id);
                        }
                    }
                }
            }
        }

        word_id_split_bit_map
            .into_iter()
            .filter_map(|(word_id, split_bit_matrix)| {
                split_bit_matrix
                    .into_iter()
                    .all(|split_bit_vec| split_bit_vec.into_iter().any(|split_bit| split_bit <= 0))
                    .then_some(SimpleResult {
                        word_id,
                        word: Cow::Borrowed(
                            // Guaranteed not failed
                            &unsafe { self.simple_word_conf_map.get(&word_id).unwrap_unchecked() }
                                .word,
                        ),
                    })
            })
            .collect()
    }
}
