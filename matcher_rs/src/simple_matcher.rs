use std::fmt::Display;
use std::iter;
use std::simd::Simd;
use std::{borrow::Cow, collections::HashMap};

use ahash::AHashMap;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind::DFA};
use bitflags::bitflags;
use nohash_hasher::{IntMap, IntSet, IsEnabled};
use serde::{Deserializer, Serializer};
use sonic_rs::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use crate::process::process_matcher::reduce_text_process;
use crate::{MatchResultTrait, TextMatcherTrait};

/// The maximum limit of word combinations that are considered for matches.
/// This value is used to limit the number of different word combinations the algorithm evaluates.
const WORD_COMBINATION_LIMIT: usize = 32;
const ZEROS: Simd<u8, WORD_COMBINATION_LIMIT> = Simd::from_array([0; WORD_COMBINATION_LIMIT]);

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
    #[derive(Hash, PartialEq, Eq, Clone, Copy, Debug)]
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

pub type SimpleMatchTypeWordMap<'a> = IntMap<SimpleMatchType, IntMap<u64, &'a str>>;

#[derive(Debug, Clone)]
/// [WordConf] is a structure that holds configuration details for a word used
/// within the [SimpleMatcher].
///
/// This structure is designed to store both the textual representation of a word
/// and a SIMD (Single Instruction, Multiple Data) vector that represents the split
/// bits for efficient text matching and transformation operations.
///
/// The `split_bit` vector is used to keep track of the various segments or parts
/// of the word that may be transformed or matched against. It allows for efficient
/// bitwise operations to quickly identify matching patterns based on pre-defined rules.
///
/// # Fields
///
/// * `word` - A [String] representing the word that is to be configured for matching.
/// * `split_bit` - A SIMD vector ([Simd<u8, WORD_COMBINATION_LIMIT>]) representing the
///   split bits for the word. This vector aids in performing efficient combination
///   matching by storing bitwise information about the word's segments.
///
/// This structure plays a critical role in facilitating efficient text processing
/// and matching within the [SimpleMatcher] by combining textual and SIMD vector data.
struct WordConf {
    word: String,
    split_bit: Simd<u8, WORD_COMBINATION_LIMIT>,
}

#[derive(Debug, Clone)]
/// `SimpleAcTable` is a structure that encapsulates the Aho-Corasick matcher and a list of word configurations.
///
/// This structure is used within the [SimpleMatcher] to hold the compiled Aho-Corasick automaton (`ac_matcher`)
/// and the corresponding configurations for words (`ac_word_conf_list`). The configurations assist in efficient
/// pattern matching and transformation operations by mapping each pattern to its unique word identifier and offset.
///
/// # Fields
///
/// * `ac_matcher` - An instance of the [AhoCorasick] matcher, which is used to perform efficient pattern matching.
/// * `ac_word_conf_list` - A vector of tuples, where each tuple contains:
///     * [u64] - A unique identifier for the word.
///     * [usize] - An offset representing the position or segment of the word within the matcher.
struct SimpleAcTable {
    ac_matcher: AhoCorasick,
    ac_word_conf_list: Vec<(u64, usize)>,
}

#[derive(Debug, Serialize)]
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
/// * `word_id` - A [u64] value representing the unique identifier of the matched word.
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
pub struct SimpleResult<'a> {
    pub word_id: u64,
    pub word: Cow<'a, str>,
}

impl MatchResultTrait<'_> for SimpleResult<'_> {
    fn word_id(&self) -> u64 {
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
/// * `simple_match_type_ac_table_map` - A mapping of [SimpleMatchType] to `SimpleAcTable`, which contains
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
#[derive(Clone)]
pub struct SimpleMatcher {
    simple_match_type_ac_table_map: IntMap<SimpleMatchType, SimpleAcTable>,
    simple_wordconf_map: IntMap<u64, WordConf>,
}

impl SimpleMatcher {
    /// Constructs a new [SimpleMatcher] from the provided word map.
    ///
    /// This function initializes a [SimpleMatcher] structure using the mappings defined in the
    /// provided word map. It processes each entry in the map to set up the necessary mappings and
    /// configurations for pattern matching and text transformations.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type_word_map` - A reference to a [HashMap] where:
    ///   * The key is a [SimpleMatchType] representing a specific matching and transformation rule.
    ///   * The value is another [HashMap] containing word mappings with:
    ///     * A word identifier ([u64]).
    ///     * The actual word as a string slice (`&'a str`).
    ///
    /// # Returns
    ///
    /// * [SimpleMatcher] - An instance of [SimpleMatcher] initialized with the provided word mappings and configurations.
    ///
    /// # Detailed Processing:
    ///
    /// 1. Initialize an empty [SimpleMatcher] with default mappings for process map, AC table map, and word config map.
    /// 2. Iterate through the `simple_match_type_word_map`:
    ///     a. For each [SimpleMatchType], iterate through its bit flags.
    ///     b. For each bit flag, insert or update its corresponding process matcher in the process map.
    /// 3. Construct a `SimpleAcTable` for each [SimpleMatchType], adjusted for text and word deletion.
    /// 4. Insert the constructed `SimpleAcTable` into the AC table map with the adjusted [SimpleMatchType] as the key.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use matcher_rs::{SimpleMatcher, SimpleMatchType, TextMatcherTrait};
    ///
    /// let word_maps = HashMap::from([
    ///     (SimpleMatchType::Fanjian, HashMap::from([(1, "ChineseWord1"), (2, "ChineseWord2")])),
    ///     (SimpleMatchType::Normalize, HashMap::from([(3, "NormalizationExample1"), (4, "NormalizationExample2")]))
    /// ]);
    ///
    /// let simple_matcher = SimpleMatcher::new(&word_maps);
    ///
    /// let text = "ExampleText";
    /// let is_match = simple_matcher.is_match(text);
    /// let results = simple_matcher.process(text);
    /// ```
    pub fn new<I, S1, S2>(
        simple_match_type_word_map: &HashMap<SimpleMatchType, HashMap<u64, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str>,
    {
        let mut simple_matcher = SimpleMatcher {
            simple_match_type_ac_table_map: IntMap::default(),
            simple_wordconf_map: IntMap::default(),
        };

        for (simple_match_type, simple_word_map) in simple_match_type_word_map {
            let simple_ac_table = simple_matcher.build_simple_ac_table(
                *simple_match_type - SimpleMatchType::TextDelete,
                simple_word_map,
            );

            simple_matcher.simple_match_type_ac_table_map.insert(
                *simple_match_type - SimpleMatchType::WordDelete,
                simple_ac_table,
            );
        }

        simple_matcher
    }

    /// Builds a `SimpleAcTable` from the provided word map and [SimpleMatchType].
    ///
    /// This function constructs a `SimpleAcTable` by iterating through the provided `simple_word_map`,
    /// processing each word according to the specified `simple_match_type`. It collects words to be
    /// matched (using Aho-Corasick algorithm) and their corresponding configurations into vectors.
    /// The resulting `SimpleAcTable` contains both the matcher and word configuration list.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type` - A [SimpleMatchType] bit flags that define specific text transformation rules.
    /// * `simple_word_map` - An iterable of tuples, where each tuple contains:
    ///     * A word identifier (u64).
    ///     * The actual word as a string slice.
    ///
    /// # Returns
    ///
    /// * `SimpleAcTable` - A structure containing the Aho-Corasick matcher and word configuration list.
    ///
    /// # Detailed Processing:
    ///
    /// 1. Initialize vectors to hold words (`ac_wordlist`) and word configurations (`ac_word_conf_list`).
    /// 2. Iterate through each word in the `simple_word_map`:
    ///     a. Split the word by commas and count occurrences of each split segment using `ac_split_word_counter`.
    ///     b. Create a SIMD vector (`split_bit_vec`) representing the count of each split segment.
    ///     c. Store the word and its SIMD split bit in `simple_wordconf_map`.
    ///     d. For each unique split segment, reduce the text based on `simple_match_type` and add to `ac_wordlist`.
    /// 3. Construct and return a `SimpleAcTable` by building an Aho-Corasick matcher from `ac_wordlist`,
    ///    and pairing it with the collected word configurations (`ac_word_conf_list`).
    ///
    fn build_simple_ac_table<I, S2>(
        &mut self,
        simple_match_type: SimpleMatchType,
        simple_word_map: &HashMap<u64, I, S2>,
    ) -> SimpleAcTable
    where
        I: AsRef<str>,
    {
        let mut ac_wordlist = Vec::new();
        let mut ac_word_conf_list = Vec::new();

        for (&simple_word_id, simple_word) in simple_word_map {
            let mut ac_split_word_counter = AHashMap::default();
            for ac_split_word in simple_word.as_ref().split(',').filter(|&x| !x.is_empty()) {
                ac_split_word_counter
                    .entry(ac_split_word)
                    .and_modify(|cnt| *cnt += 1)
                    .or_insert(1);
            }

            let split_bit_vec = ac_split_word_counter
                .values()
                .take(WORD_COMBINATION_LIMIT)
                .map(|&x| 1 << (x.min(8) - 1))
                .collect::<ArrayVec<[u8; WORD_COMBINATION_LIMIT]>>();
            let split_bit = Simd::load_or_default(&split_bit_vec);

            self.simple_wordconf_map.insert(
                simple_word_id,
                WordConf {
                    word: simple_word.as_ref().to_owned(),
                    split_bit,
                },
            );

            for (offset, &split_word) in ac_split_word_counter
                .keys()
                .take(WORD_COMBINATION_LIMIT)
                .enumerate()
            {
                for ac_word in reduce_text_process(simple_match_type, split_word) {
                    ac_wordlist.push(ac_word);
                    ac_word_conf_list.push((simple_word_id, offset));
                }
            }
        }

        SimpleAcTable {
            ac_matcher: AhoCorasickBuilder::new()
                .kind(Some(DFA))
                .ascii_case_insensitive(true)
                .build(
                    ac_wordlist
                        .iter()
                        .map(|ac_word| ac_word.as_ref().as_bytes()),
                )
                .unwrap(),
            ac_word_conf_list,
        }
    }
}

impl<'a> TextMatcherTrait<'a, SimpleResult<'a>> for SimpleMatcher {
    /// Determines if any patterns match the input text after applying transformations.
    ///
    /// This function checks if any patterns from the provided [SimpleMatchType] transformations
    /// match the input text using Aho-Corasick pattern matching. It ensures that the input text
    /// undergoes all specified transformations and then verifies the presence of overlapping
    /// patterns. This allows flexible and efficient text matching with transformations.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be checked for matches.
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if a match is found after applying transformations, otherwise `false`.
    ///
    /// # Detailed Processing:
    ///
    /// 1. If the input text is empty, return `false`.
    /// 2. Initialize a map to track word IDs and their split bit vectors during processing.
    /// 3. Iterate over each [SimpleMatchType] and its associated `SimpleAcTable`:
    ///     a. Process the text according to the transformation rules defined by the [SimpleMatchType].
    ///     b. Iterate over each processed version of the text.
    ///     c. Use the Aho-Corasick matcher to find overlapping patterns in the processed text.
    ///     d. Retrieve the word configuration based on the pattern found.
    ///     e. Initialize or update the split bit vector corresponding to the word ID.
    ///     f. Update the split bit vector by shifting the bit to the right.
    ///     g. Check if all shifts have reduced the split bit vector to all zeros.
    ///     h. If so, return `true` indicating a match is found.
    /// 4. If no matches are found after all transformations and checks, return `false`.
    ///
    /// This function ensures efficient text matching using SIMD and Aho-Corasick algorithms
    /// while accounting for various transformations specified by the [SimpleMatchType].
    fn is_match(&self, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }

        let mut word_id_split_bit_map = IntMap::default();

        for (&simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            let processed_text_list = reduce_text_process(simple_match_type, text);
            let processed_times = processed_text_list.len();

            for (index, processed_text) in processed_text_list.iter().enumerate() {
                for ac_result in simple_ac_table
                    .ac_matcher
                    .find_overlapping_iter(processed_text.as_ref())
                {
                    let ac_word_id = ac_result.pattern().as_usize();
                    let ac_word_conf =
                        unsafe { simple_ac_table.ac_word_conf_list.get_unchecked(ac_word_id) };

                    let word_id = ac_word_conf.0;
                    let word_conf =
                        unsafe { self.simple_wordconf_map.get(&word_id).unwrap_unchecked() };

                    let split_bit_vec = word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                        iter::repeat_n(word_conf.split_bit, processed_times)
                            .collect::<ArrayVec<[_; 8]>>()
                    });

                    *unsafe {
                        split_bit_vec
                            .get_unchecked_mut(index)
                            .as_mut_array()
                            .get_unchecked_mut(ac_word_conf.1)
                    } >>= 1;

                    if split_bit_vec
                        .iter()
                        .fold(Simd::splat(1), |acc, &bit| acc & bit)
                        == ZEROS
                    {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Processes the input text to find matches and returns a list of results.
    ///
    /// This function goes through multiple transformation stages to process the input text
    /// and uses Aho-Corasick pattern matching to identify text segments that match the
    /// specified patterns across these transformations.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be processed and checked for matches.
    ///
    /// # Returns
    ///
    /// * [Vec<SimpleResult<'a>>] - A vector containing the results of the match, each result includes
    ///   the word id and the word itself.
    ///
    /// # Detailed Processing:
    ///
    /// 1. If the input text is empty, return an empty list of results.
    /// 2. Initialize a set to track word IDs that have already been matched.
    /// 3. Initialize a map to keep track of word IDs and their corresponding split bit vectors
    ///    during processing.
    /// 4. For each [SimpleMatchType] and its corresponding `SimpleAcTable`:
    ///     a. Apply the transformation rules defined by the [SimpleMatchType] to process the text.
    ///     b. Iterate over each processed version of the text.
    ///     c. Use the Aho-Corasick matcher to find overlapping patterns in the processed text.
    ///     d. Retrieve the word configuration based on the pattern found.
    ///     e. Skip if the word ID has already been matched.
    ///     f. Initialize or update the split bit vector corresponding to the word ID.
    ///     g. Update the split bit vector by shifting the bit to the right.
    ///     h. If all shifts have reduced the split bit vector to all zeros, add the word ID to the set
    ///        and include the result in the result list.
    ///
    /// This ensures efficient text matching across transformed versions of the input text using
    /// SIMD and Aho-Corasick algorithms.
    fn process(&'a self, text: &str) -> Vec<SimpleResult<'a>> {
        let mut result_list = Vec::new();

        if text.is_empty() {
            return result_list;
        }

        let mut word_id_set = IntSet::default();
        let mut word_id_split_bit_map = IntMap::default();

        for (&simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            let processed_text_list = reduce_text_process(simple_match_type, text);
            let processed_times = processed_text_list.len(); // Get the number of processed versions of the text

            for (index, processed_text) in processed_text_list.iter().enumerate() {
                for ac_result in simple_ac_table
                    .ac_matcher
                    .find_overlapping_iter(processed_text.as_ref())
                {
                    let ac_word_conf = unsafe {
                        simple_ac_table
                            .ac_word_conf_list
                            .get_unchecked(ac_result.pattern().as_usize())
                    };
                    let word_id = ac_word_conf.0;

                    if word_id_set.contains(&word_id) {
                        continue;
                    }

                    let word_conf =
                        unsafe { self.simple_wordconf_map.get(&word_id).unwrap_unchecked() };

                    let split_bit_vec = word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                        iter::repeat_n(word_conf.split_bit, processed_times)
                            .collect::<ArrayVec<[_; 8]>>()
                    });

                    *unsafe {
                        split_bit_vec
                            .get_unchecked_mut(index)
                            .as_mut_array()
                            .get_unchecked_mut(ac_word_conf.1)
                    } >>= 1;

                    if split_bit_vec
                        .iter()
                        .fold(Simd::splat(1), |acc, &bit| acc & bit)
                        == ZEROS
                    {
                        word_id_set.insert(word_id);
                        result_list.push(SimpleResult {
                            word_id,
                            word: Cow::Borrowed(&word_conf.word),
                        });
                    }
                }
            }
        }

        result_list
    }
}
