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

/// [SimpleAcTable] is a structure that encapsulates an Aho-Corasick matcher and a
/// deduplicated list of word configurations.
///
/// This structure is designed to provide efficient pattern matching using the Aho-Corasick
/// algorithm, which is particularly suited for matching a large set of patterns in a text.
/// It includes an Aho-Corasick matcher and a list of deduplicated word configurations,
/// which are used to manage and optimize the word matching process.
///
/// # Fields
///
/// * `ac_matcher` - An [AhoCorasick] instance that performs the actual pattern matching.
/// * `ac_dedup_word_conf_list` - A [Vec] of [Vec] containing tuples of a word identifier ([u32])
///   and its corresponding position ([usize]) in the deduplicated word configuration list.
///
/// This structure ensures that matched patterns are processed efficiently and that the word
/// configurations are kept organized and deduplicated to avoid redundant processing.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct SimpleAcTable {
    ac_matcher: AhoCorasick,
    ac_dedup_word_conf_list: Vec<Vec<(u32, usize)>>,
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

/// [SimpleMatcher] is a structure that encapsulates the logic for text matching and transformation
/// based on various [SimpleMatchType] rules.
///
/// This structure holds mappings and configurations for text processing, enabling efficient
/// pattern matching and transformation operations. It includes a mapping of [SimpleMatchType]
/// to process mappings, a mapping of [SimpleMatchType] to Aho-Corasick tables, and a mapping
/// of word IDs to word configurations.
///
/// # Fields
///
/// * `smt_tree` - A vec of `SimpleMatchTypeBitNode`.
/// * `smt_ac_table_map` - A mapping of [SimpleMatchType] to `SimpleAcTable`, which contains
///   the Aho-Corasick matcher and word configurations for efficient text matching.
/// * `simple_wordconf_map` - A mapping of word IDs to `WordConf` structures, which hold the textual
///   representation of a word and a SIMD vector representing the split bits for the word.
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
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SimpleMatcher {
    smt_tree: Option<Vec<SimpleMatchTypeBitNode>>,
    smt_ac_table_map: IntMap<SimpleMatchType, SimpleAcTable>,
    simple_word_conf_map: IntMap<u32, WordConf>,
}

impl SimpleMatcher {
    /// Constructs a new [SimpleMatcher] from a provided map of [SimpleMatchType] to word maps.
    ///
    /// This function initializes a [SimpleMatcher] by creating `SimpleAcTable` instances for
    /// each [SimpleMatchType] based on the provided word maps. It processes the word maps to
    /// generate efficient Aho-Corasick tables for pattern matching.
    ///
    /// # Arguments
    ///
    /// * `smt_word_map` - A reference to a [HashMap] mapping [SimpleMatchType]
    ///   to another [HashMap] of word identifiers ([u32]) and words. The inner [HashMap] contains:
    ///   * The key: a word identifier ([u32]).
    ///   * The value: a word that implements [`AsRef<str>`].
    ///
    /// # Returns
    ///
    /// * [SimpleMatcher] - A new [SimpleMatcher] instance initialized with the provided word maps and
    ///   their associated match types.
    ///
    /// The constructed [SimpleMatcher] will have its match type list and Aho-Corasick tables set up
    /// based on the provided mappings. If there are at least 4 match types, the `smt_list`
    /// field will be populated with the match types; otherwise, it remains `None`.
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
    pub fn new<I, S1, S2>(
        smt_word_map: &HashMap<SimpleMatchType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str>,
    {
        let mut simple_matcher = SimpleMatcher {
            smt_tree: None,
            smt_ac_table_map: IntMap::default(),
            simple_word_conf_map: IntMap::default(),
        };

        for (&simple_match_type, simple_word_map) in smt_word_map {
            let simple_ac_table = simple_matcher.build_simple_ac_table(
                simple_match_type - SimpleMatchType::TextDelete,
                simple_word_map,
            );

            simple_matcher.smt_ac_table_map.insert(
                simple_match_type - SimpleMatchType::WordDelete,
                simple_ac_table,
            );
        }

        if smt_word_map.len() >= 4 {
            simple_matcher.smt_tree = Some(build_smt_tree(
                &simple_matcher
                    .smt_ac_table_map
                    .keys()
                    .copied()
                    .collect::<Vec<SimpleMatchType>>(),
            ));
        }

        simple_matcher
    }

    /// Builds a `SimpleAcTable` for a given [SimpleMatchType] and a word map.
    ///
    /// This function generates an Aho-Corasick table structured for efficient pattern matching,
    /// based on the specified [SimpleMatchType] and a supplied mapping of words. It processes
    /// the word map to split words into sub-patterns based on specified delimiters ('&' and '~'),
    /// constructs the Aho-Corasick matcher, and sets up the internal configuration for each word.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type` - The [SimpleMatchType] specifying the type of text transformation/matching rule to apply.
    /// * `simple_word_map` - A reference to a [`HashMap<u32, I, S2>`] where:
    ///   * The key is a word identifier ([u32]).
    ///   * The value is a word itself, which is a type that implements [`AsRef<str>`].
    ///
    /// # Returns
    ///
    /// * `SimpleAcTable` - The constructed Aho-Corasick table for the given match type and word map.
    ///
    /// # Detailed Processing:
    ///
    /// 1. Initialize empty vectors for `ac_wordlist` and `ac_word_conf_list`.
    /// 2. For each word in `simple_word_map`:
    ///     a. Split the word into sub-patterns based on '&' and '~' delimiters.
    ///     b. Track sub-patterns that should be counted positively ('&') or negatively ('~').
    ///     c. Construct split bit vectors combining positive and negative counts.
    ///     d. Store word configuration (`WordConf`) with its split bit vector and special index.
    /// 3. For each sub-pattern, apply text processing based on `simple_match_type`.
    /// 4. Add processed sub-patterns to `ac_wordlist` and their configurations to `ac_word_conf_list`.
    /// 5. Build and return a `SimpleAcTable` with the constructed Aho-Corasick matcher and configurations.
    ///
    /// # Safety
    ///
    /// Unsafe code is used for unchecked slice accesses and integer operations to maximize performance.
    /// Ensure input data complies with expected formats and types to avoid undefined behavior.
    fn build_simple_ac_table<I, S2>(
        &mut self,
        simple_match_type: SimpleMatchType,
        simple_word_map: &HashMap<u32, I, S2>,
    ) -> SimpleAcTable
    where
        I: AsRef<str>,
    {
        let mut ac_dedup_word_id = 0;
        let mut ac_dedup_word_conf_list = Vec::with_capacity(simple_word_map.len());
        let mut ac_dedup_word_list = Vec::with_capacity(simple_word_map.len());
        let mut ac_dedup_word_id_map = AHashMap::with_capacity(simple_word_map.len());

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

            self.simple_word_conf_map.insert(
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
                for ac_word in reduce_text_process_emit(simple_match_type, split_word) {
                    if let Some(ac_dedup_word_id) = ac_dedup_word_id_map.get(ac_word.as_ref()) {
                        // Guaranteed not failed
                        let word_conf_list: &mut Vec<(u32, usize)> = unsafe {
                            ac_dedup_word_conf_list.get_unchecked_mut(*ac_dedup_word_id as usize)
                        };
                        word_conf_list.push((simple_word_id, offset));
                    } else {
                        ac_dedup_word_id_map.insert(ac_word.clone(), ac_dedup_word_id);
                        ac_dedup_word_conf_list.push(vec![(simple_word_id, offset)]);
                        ac_dedup_word_list.push(ac_word);
                        ac_dedup_word_id += 1;
                    }
                }
            }
        }

        SimpleAcTable {
            #[cfg(not(feature = "serde"))]
            ac_matcher: AhoCorasickBuilder::new()
                .kind(Some(AhoCorasickKind::DFA))
                .ascii_case_insensitive(true)
                .build(ac_dedup_word_list.iter().map(|ac_word| ac_word.as_ref()))
                .unwrap(),
            #[cfg(feature = "serde")]
            ac_matcher: AhoCorasickBuilder::new()
                .kind(Some(AhoCorasickKind::DFA))
                .ascii_case_insensitive(true)
                .prefilter(false)
                .build(
                    ac_dedup_word_list
                        .iter()
                        .map(|ac_word| ac_word.as_ref().as_bytes()),
                )
                .unwrap(),
            ac_dedup_word_conf_list,
        }
    }
}

impl<'a> TextMatcherTrait<'a, SimpleResult<'a>> for SimpleMatcher {
    /// Checks if the input text matches any of the patterns stored in the matcher.
    ///
    /// This function processes the input text based on each [SimpleMatchType] transformation and
    /// uses the Aho-Corasick algorithm to determine if any patterns from the `smt_ac_table_map`
    /// are present in the input text. It utilizes a bit vector technique to keep track of matched
    /// patterns and their configurations.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be checked for matches.
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if any of the patterns match the input text, otherwise returns `false`.
    ///
    /// # Detailed Processing:
    ///
    /// 1. If the input text is empty, return `false`.
    /// 2. Initialize a map (`word_id_split_bit_map`) to track word configurations during processing.
    /// 3. For each [SimpleMatchType] and its corresponding `SimpleAcTable`:
    ///     a. Apply the transformation rules defined by the [SimpleMatchType] to process the text.
    ///     b. Iterate over each processed version of the text.
    ///     c. Use the Aho-Corasick matcher to find overlapping patterns in the processed text.
    ///     d. Retrieve the word configuration based on the pattern found.
    ///     e. Initialize or update the split bit vector corresponding to the word ID.
    ///     f. Update the split bit vector by shifting the bit to the right.
    /// 4. Check if any entry in `word_id_split_bit_map` contains a bit vector where all bits are zero
    ///    after processing. If such an entry exists, return `true`.
    ///
    /// This function ensures efficient pattern matching using Aho-Corasick algorithms across
    /// transformed versions of the input text.
    fn is_match(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }

        let mut word_id_split_bit_map = IntMap::default();
        let mut word_id_set = IntSet::default();
        let mut not_word_id_set = IntSet::default();

        for (&simple_match_type, simple_ac_table) in &self.smt_ac_table_map {
            let processed_text_list = reduce_text_process_emit(simple_match_type, text);
            let processed_times = processed_text_list.len();

            for (index, processed_text) in processed_text_list.iter().enumerate() {
                // Guaranteed not failed
                for ac_dedup_result in unsafe {
                    simple_ac_table
                        .ac_matcher
                        .try_find_overlapping_iter(processed_text.as_ref())
                        .unwrap_unchecked()
                } {
                    // Guaranteed not failed
                    for &(word_id, offset) in unsafe {
                        simple_ac_table
                            .ac_dedup_word_conf_list
                            .get_unchecked(ac_dedup_result.pattern().as_usize())
                    } {
                        if not_word_id_set.contains(&word_id) {
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
                            *bit =
                                bit.unchecked_add((offset < word_conf.not_index) as i32 * -2 + 1);

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
            }

            if !word_id_set.is_empty() {
                return true;
            }
        }

        false
    }

    /// Processes the input text to find matches based on the patterns stored in the matcher.
    ///
    /// This function works similarly to the `is_match` method but provides detailed match
    /// information in the form of `SimpleResult` instances. It processes the input text
    /// according to the transformations defined by each [SimpleMatchType] and utilizes
    /// the Aho-Corasick algorithm to find overlapping patterns. The matched patterns
    /// are checked against defined word configurations to form the resulting matches.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be checked for matches.
    ///
    /// # Returns
    ///
    /// * [`Vec<SimpleResult<'a>>`] - A vector of results, where each result contains the matched
    ///   word ID and the matched word.
    ///
    /// # Detailed Processing:
    ///
    /// 1. If the input text is empty, return an empty vector.
    /// 2. Initialize a map (`word_id_split_bit_map`) to track word configurations during processing.
    /// 3. If `smt_tree` is present:
    ///     a. Process the text using `reduce_text_process_with_tree`.
    ///     b. For each [SimpleMatchType] and corresponding `SimpleAcTable`, process the text.
    ///     c. Use the Aho-Corasick matcher to find overlapping patterns and update the split
    ///        bit matrix based on the configurations.
    /// 4. If `smt_tree` is not present:
    ///     a. Process the text using `reduce_text_process_emit`.
    ///     b. For each [SimpleMatchType] and corresponding `SimpleAcTable`, process the text.
    ///     c. Use the Aho-Corasick matcher to find overlapping patterns and update the split
    ///        bit matrix based on the configurations.
    /// 5. Convert the `word_id_split_bit_map` into a vector of [SimpleResult] instances by filtering
    ///    the results based on the split bit matrix.
    ///
    /// This function ensures detailed and efficient pattern matching using Aho-Corasick algorithms
    /// across transformed versions of the input text and returns precise matching results.
    fn process(&'a self, text: &str) -> Vec<SimpleResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut word_id_split_bit_map = IntMap::default();
        let mut not_word_id_set = IntSet::default();

        if let Some(smt_tree) = &self.smt_tree {
            let (smt_index_set_map, processed_text_list) =
                reduce_text_process_with_tree(smt_tree, text);

            for (&simple_match_type, simple_ac_table) in &self.smt_ac_table_map {
                let processed_index_set =
                    unsafe { smt_index_set_map.get(&simple_match_type).unwrap_unchecked() };
                let processed_times = processed_index_set.len();

                for (index, &processed_index) in processed_index_set.iter().enumerate() {
                    // Guaranteed not failed
                    for ac_dedup_result in unsafe {
                        simple_ac_table
                            .ac_matcher
                            .try_find_overlapping_iter(
                                processed_text_list.get_unchecked(processed_index).as_ref(),
                            )
                            .unwrap_unchecked()
                    } {
                        // Guaranteed not failed
                        for &(word_id, offset) in unsafe {
                            simple_ac_table
                                .ac_dedup_word_conf_list
                                .get_unchecked(ac_dedup_result.pattern().as_usize())
                        } {
                            if not_word_id_set.contains(&word_id) {
                                continue;
                            }

                            // Guaranteed not failed
                            let word_conf = unsafe {
                                self.simple_word_conf_map.get(&word_id).unwrap_unchecked()
                            };

                            let split_bit_matrix =
                                word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                                    word_conf
                                        .split_bit
                                        .iter()
                                        .map(|&bit| {
                                            iter::repeat(bit).take(processed_times).collect()
                                        })
                                        .collect::<Vec<Vec<i32>>>()
                                });

                            // split_bit is i32, so it will not overflow almost 100%
                            unsafe {
                                let split_bit = split_bit_matrix
                                    .get_unchecked_mut(offset)
                                    .get_unchecked_mut(index);
                                *split_bit = split_bit
                                    .unchecked_add((offset < word_conf.not_index) as i32 * -2 + 1);

                                if offset >= word_conf.not_index && *split_bit > 0 {
                                    not_word_id_set.insert(word_id);
                                    word_id_split_bit_map.remove(&word_id);
                                }
                            };
                        }
                    }
                }
            }
        } else {
            for (&simple_match_type, simple_ac_table) in &self.smt_ac_table_map {
                let processed_text_list = reduce_text_process_emit(simple_match_type, text);
                let processed_times = processed_text_list.len();

                for (index, processed_text) in processed_text_list.iter().enumerate() {
                    // Guaranteed not failed
                    for ac_dedup_result in unsafe {
                        simple_ac_table
                            .ac_matcher
                            .try_find_overlapping_iter(processed_text.as_ref())
                            .unwrap_unchecked()
                    } {
                        // Guaranteed not failed
                        for &(word_id, offset) in unsafe {
                            simple_ac_table
                                .ac_dedup_word_conf_list
                                .get_unchecked(ac_dedup_result.pattern().as_usize())
                        } {
                            if not_word_id_set.contains(&word_id) {
                                continue;
                            }

                            // Guaranteed not failed
                            let word_conf = unsafe {
                                self.simple_word_conf_map.get(&word_id).unwrap_unchecked()
                            };

                            let split_bit_matrix =
                                word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                                    word_conf
                                        .split_bit
                                        .iter()
                                        .map(|&bit| {
                                            iter::repeat(bit).take(processed_times).collect()
                                        })
                                        .collect::<Vec<Vec<i32>>>()
                                });

                            // split_bit is i32, so it will not overflow almost 100%
                            unsafe {
                                let split_bit = split_bit_matrix
                                    .get_unchecked_mut(offset)
                                    .get_unchecked_mut(index);
                                *split_bit = split_bit
                                    .unchecked_add((offset < word_conf.not_index) as i32 * -2 + 1);

                                if offset >= word_conf.not_index && *split_bit > 0 {
                                    not_word_id_set.insert(word_id);
                                    word_id_split_bit_map.remove(&word_id);
                                }
                            };
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
