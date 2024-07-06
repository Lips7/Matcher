use std::fmt::Display;
use std::iter;
use std::{borrow::Cow, collections::HashMap};

use ahash::AHashMap;
use aho_corasick_unsafe::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind::DFA};
use bitflags::bitflags;
use nohash_hasher::{IntMap, IntSet, IsEnabled};
use serde::{Deserializer, Serializer};
use sonic_rs::{Deserialize, Serialize};

use crate::matcher::{MatchResultTrait, TextMatcherTrait};
use crate::process::process_matcher::reduce_text_process_emit;

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
///     * [u32] - A unique identifier for the word.
///     * [usize] - An offset representing the position or segment of the word within the matcher.
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
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SimpleMatcher {
    simple_match_type_ac_table_map: IntMap<SimpleMatchType, SimpleAcTable>,
    simple_word_conf_map: IntMap<u32, WordConf>,
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
    ///     * A word identifier ([u32]).
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
        simple_match_type_word_map: &HashMap<SimpleMatchType, HashMap<u32, I, S1>, S2>,
    ) -> SimpleMatcher
    where
        I: AsRef<str>,
    {
        let mut simple_matcher = SimpleMatcher {
            simple_match_type_ac_table_map: IntMap::default(),
            simple_word_conf_map: IntMap::default(),
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

    /// Builds a `SimpleAcTable` for a given `SimpleMatchType` and a word map.
    ///
    /// This function generates an Aho-Corasick table structured for efficient pattern matching,
    /// based on the specified `SimpleMatchType` and a supplied mapping of words. It processes
    /// the word map to split words into sub-patterns based on specified delimiters ('&' and '~'),
    /// constructs the Aho-Corasick matcher, and sets up the internal configuration for each word.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type` - The `SimpleMatchType` specifying the type of text transformation/matching rule to apply.
    /// * `simple_word_map` - A reference to a `HashMap<u32, I, S2>` where:
    ///   * The key is a word identifier (`u32`).
    ///   * The value is a word itself, which is a type that implements `AsRef<str>`.
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
        let mut ac_dedup_word_conf_list = Vec::new();
        let mut ac_dedup_word_list = Vec::new();
        let mut ac_dedup_word_id_map = AHashMap::default();

        for (&simple_word_id, simple_word) in simple_word_map {
            let mut ac_split_word_and_counter = AHashMap::default();
            let mut ac_split_word_not_counter = AHashMap::default();

            let mut start = 0;
            let mut is_and = false;
            let mut is_not = false;

            for (index, char) in simple_word.as_ref().match_indices(['&', '~']) {
                if (is_and || start == 0) && start != index {
                    ac_split_word_and_counter
                        .entry(unsafe { simple_word.as_ref().get_unchecked(start..index) })
                        .and_modify(|cnt| *cnt += 1)
                        .or_insert(1);
                }
                if is_not && start != index {
                    ac_split_word_not_counter
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
                    .entry(unsafe { simple_word.as_ref().get_unchecked(start..) })
                    .and_modify(|cnt| *cnt += 1)
                    .or_insert(1);
            }
            if is_not && start != simple_word.as_ref().len() {
                ac_split_word_not_counter
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
                .kind(Some(DFA))
                .ascii_case_insensitive(true)
                .build(ac_dedup_word_list.iter().map(|ac_word| ac_word.as_ref()))
                .unwrap(),
            #[cfg(feature = "serde")]
            ac_matcher: AhoCorasickBuilder::new()
                .kind(Some(DFA))
                .ascii_case_insensitive(true)
                .prefilter(false)
                .build(
                    ac_wordlist
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
    /// uses the Aho-Corasick algorithm to determine if any patterns from the `simple_match_type_ac_table_map`
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

        for (&simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            let processed_text_list = reduce_text_process_emit(simple_match_type, text);
            let processed_times = processed_text_list.len();

            for (index, processed_text) in processed_text_list.iter().enumerate() {
                for ac_dedup_result in simple_ac_table
                    .ac_matcher
                    .find_overlapping_iter(processed_text.as_ref())
                {
                    for ac_word_conf in unsafe {
                        simple_ac_table
                            .ac_dedup_word_conf_list
                            .get_unchecked(ac_dedup_result.pattern().as_usize())
                    } {
                        let word_id = ac_word_conf.0;
                        let word_conf =
                            unsafe { self.simple_word_conf_map.get(&word_id).unwrap_unchecked() };

                        let split_bit_vec =
                            word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                                word_conf
                                    .split_bit
                                    .iter()
                                    .map(|&bit| iter::repeat(bit).take(processed_times).collect())
                                    .collect::<Vec<Vec<i32>>>()
                            });

                        unsafe {
                            let bit = split_bit_vec
                                .get_unchecked_mut(ac_word_conf.1)
                                .get_unchecked_mut(index);
                            *bit = bit.unchecked_add(
                                (ac_word_conf.1 < word_conf.not_index) as i32 * -2 + 1,
                            );
                        }
                    }
                }
            }
        }

        word_id_split_bit_map.into_iter().any(|(_, split_bit_vec)| {
            split_bit_vec
                .into_iter()
                .all(|bit_vec| bit_vec.into_iter().any(|bit| bit <= 0))
        })
    }

    /// Processes the input text and returns a vector of [SimpleResult] containing matches found.
    ///
    /// This function is responsible for processing the input text using various transformation rules
    /// defined by [SimpleMatchType] and then utilizing the Aho-Corasick algorithm to find overlapping patterns
    /// within the processed text. It leverages a bit vector technique to determine matched patterns and keep
    /// track of their configurations.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be checked for matches.
    ///
    /// # Returns
    ///
    /// * [`Vec<SimpleResult>`] - A vector of [SimpleResult] structs, each containing the `word_id` and the `word`
    ///   associated with the matched pattern.
    ///
    /// # Detailed Processing:
    ///
    /// 1. If the input text is empty, return an empty vector.
    /// 2. Initialize a map (`word_id_split_bit_map`) to track word configurations during processing.
    /// 3. Iterate through each `SimpleMatchType` and its corresponding `SimpleAcTable`:
    ///     a. Apply the transformation rules to the input text.
    ///     b. For each processed version of the text:
    ///         i. Use the Aho-Corasick matcher to find overlapping patterns.
    ///         ii. Retrieve configuration of the matched pattern.
    ///         iii. Initialize or update the split bit vector corresponding to the word ID.
    ///         iv. Update the bit according to the configuration.
    /// 4. Filter patterns that are fully matched based on the split bit vector and create a [SimpleResult] for each.
    /// 5. Return a vector of [SimpleResult] containing matched patterns.
    fn process(&'a self, text: &str) -> Vec<SimpleResult<'a>> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut word_id_split_bit_map = IntMap::default();
        let mut not_word_id_set = IntSet::default();

        for (&simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            let processed_text_list = reduce_text_process_emit(simple_match_type, text);
            let processed_times = processed_text_list.len(); // Get the number of processed versions of the text

            for (index, processed_text) in processed_text_list.iter().enumerate() {
                for ac_dedup_result in simple_ac_table
                    .ac_matcher
                    .find_overlapping_iter(processed_text.as_ref())
                {
                    for ac_word_conf in unsafe {
                        simple_ac_table
                            .ac_dedup_word_conf_list
                            .get_unchecked(ac_dedup_result.pattern().as_usize())
                    } {
                        let word_id = ac_word_conf.0;

                        if not_word_id_set.contains(&word_id) {
                            continue;
                        }

                        let word_conf =
                            unsafe { self.simple_word_conf_map.get(&word_id).unwrap_unchecked() };

                        let split_bit_vec =
                            word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                                word_conf
                                    .split_bit
                                    .iter()
                                    .map(|&bit| iter::repeat(bit).take(processed_times).collect())
                                    .collect::<Vec<Vec<i32>>>()
                            });

                        unsafe {
                            let bit = split_bit_vec
                                .get_unchecked_mut(ac_word_conf.1)
                                .get_unchecked_mut(index);
                            *bit = bit.unchecked_add(
                                (ac_word_conf.1 < word_conf.not_index) as i32 * -2 + 1,
                            );
                            if ac_word_conf.1 >= word_conf.not_index && *bit > 0 {
                                not_word_id_set.insert(word_id);
                                word_id_split_bit_map.remove(&word_id);
                            }
                        };
                    }
                }
            }
        }

        word_id_split_bit_map
            .into_iter()
            .filter_map(|(word_id, split_bit_vec)| {
                split_bit_vec
                    .into_iter()
                    .all(|bit_vec| bit_vec.into_iter().any(|bit| bit <= 0))
                    .then_some(SimpleResult {
                        word_id,
                        word: Cow::Borrowed(
                            &unsafe { self.simple_word_conf_map.get(&word_id).unwrap_unchecked() }
                                .word,
                        ),
                    })
            })
            .collect()
    }
}
