use std::borrow::Cow;
use std::intrinsics::{likely, unlikely};
use std::iter;
use std::simd::Simd;

use ahash::AHashMap;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind::DFA, MatchKind};
use bitflags::bitflags;
use nohash_hasher::{IntMap, IntSet, IsEnabled};
use serde::{Deserializer, Serializer};
use sonic_rs::{Deserialize, Serialize};
use tinyvec::ArrayVec;

use super::{MatchResultTrait, TextMatcherTrait};

/// A collection of constant string slices that include various string conversion mappings.
///
/// Each constant below is loaded from a corresponding text file using the `include_str!` macro.
/// These files contain mappings used for different conversion and normalization processes,
/// such as simplifying characters, handling punctuation, and converting between upper and lower case.
///
/// These mappings are utilized in text processing to apply transformations based on different
/// `SimpleMatchType` values. They facilitate efficient text matching and replacement operations
/// by providing a predefined set of conversion rules.
///
/// # Constants
///
/// * `FANJIAN` - Simplifies traditional Chinese characters to simplified ones.
/// * `CN_SPECIAL` - Contains special Chinese characters.
/// * `EN_SPECIAL` - Contains special English characters.
/// * `PUNCTUATION_SPECIAL` - Contains special punctuation characters.
/// * `EN_VARIATION` - Contains variations of English characters.
/// * `UNICODE` - Contains unicode specific mappings.
/// * `NUM_NORM` - Normalizes numeric characters.
/// * `UPPER_LOWER` - Maps between upper and lower case characters.
/// * `PINYIN` - Converts Chinese characters to Pinyin.
/// * `PINYIN_CHAR` - Converts individual Chinese characters to Pinyin.
const FANJIAN: &str = include_str!("../str_conv_map/FANJIAN.txt");
const CN_SPECIAL: &str = include_str!("../str_conv_map/CN-SPECIAL.txt");
const EN_SPECIAL: &str = include_str!("../str_conv_map/EN-SPECIAL.txt");
const PUNCTUATION_SPECIAL: &str = include_str!("../str_conv_map/PUNCTUATION-SPECIAL.txt");
const EN_VARIATION: &str = include_str!("../str_conv_map/EN-VARIATION.txt");
const UNICODE: &str = include_str!("../str_conv_map/UNICODE.txt");
const NUM_NORM: &str = include_str!("../str_conv_map/NUM-NORM.txt");
const UPPER_LOWER: &str = include_str!("../str_conv_map/UPPER-LOWER.txt");
const PINYIN: &str = include_str!("../str_conv_map/PINYIN.txt");
const PINYIN_CHAR: &str = include_str!("../str_conv_map/PINYIN-CHAR.txt");

/// A constant slice containing string references to various Unicode whitespace characters.
///
/// These characters include:
///
/// - Horizontal tab (`\u{0009}`).
/// - Line feed (`\u{000A}`).
/// - Vertical tab (`\u{000B}`).
/// - Form feed (`\u{000C}`).
/// - Carriage return (`\u{000D}`).
/// - Space (`\u{0020}`).
/// - Next line (`\u{0085}`).
/// - No-break space (`\u{00A0}`).
/// - Ogham space mark (`\u{1680}`).
/// - En quad (`\u{2000}`).
/// - Em quad (`\u{2001}`).
/// - En space (`\u{2002}`).
/// - Em space (`\u{2003}`).
/// - Three-per-em space (`\u{2004}`).
/// - Four-per-em space (`\u{2005}`).
/// - Six-per-em space (`\u{2006}`).
/// - Figure space (`\u{2007}`).
/// - Punctuation space (`\u{2008}`).
/// - Thin space (`\u{2009}`).
/// - Hair space (`\u{200A}`).
/// - Line separator (`\u{2028}`).
/// - Paragraph separator (`\u{2029}`).
/// - Narrow no-break space (`\u{202F}`).
/// - Medium mathematical space (`\u{205F}`).
/// - Ideographic space (`\u{3000}`).
const WHITE_SPACE: &[&str] = &[
    "\u{0009}", "\u{000A}", "\u{000B}", "\u{000C}", "\u{000D}", "\u{0020}", "\u{0085}", "\u{00A0}",
    "\u{1680}", "\u{2000}", "\u{2001}", "\u{2002}", "\u{2003}", "\u{2004}", "\u{2005}", "\u{2006}",
    "\u{2007}", "\u{2008}", "\u{2009}", "\u{200A}", "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}",
    "\u{3000}",
];

/// The maximum limit of word combinations that are considered for matches.
/// This value is used to limit the number of different word combinations the algorithm evaluates.
const WORD_COMBINATION_LIMIT: usize = 32;
const ZEROS: Simd<u8, WORD_COMBINATION_LIMIT> = Simd::from_array([0; WORD_COMBINATION_LIMIT]);

bitflags! {
    /// `SimpleMatchType` is a set of flags used to specify various text transformation rules.
    ///
    /// Each flag represents a specific type of string conversion or deletion operation.
    /// The flags can be combined using bitwise operations to create complex transformation rules.
    ///
    /// # Flags
    ///
    /// * `None` (0b00000001) - No transformation is applied.
    /// * `Fanjian` (0b00000010) - Simplifies traditional Chinese characters to simplified ones.
    /// * `WordDelete` (0b00000100) - Deletes word-level components based on predefined rules.
    /// * `TextDelete` (0b00001000) - Deletes text-level components, including special characters and whitespace.
    /// * `Delete` (0b00001100) - Combines `WordDelete` and `TextDelete` transformations.
    /// * `Normalize` (0b00010000) - Normalizes the text, including case normalization and removing variations.
    /// * `DeleteNormalize` (0b00011100) - Combines `Delete` and `Normalize` transformations.
    /// * `FanjianDeleteNormalize` (0b00011110) - Combines `Fanjian`, `Delete`, and `Normalize` transformations.
    /// * `PinYin` (0b00100000) - Converts Chinese characters to their Pinyin representation.
    /// * `PinYinChar` (0b01000000) - Converts individual Chinese characters to their Pinyin representation.
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

impl IsEnabled for SimpleMatchType {}

pub type SimpleMatchTypeWordMap<'a> = IntMap<SimpleMatchType, IntMap<u64, &'a str>>;

/// Constructs a process matcher for the given SimpleMatchType bit.
///
/// This function generates a tuple containing:
/// 1. A vector of replacement strings (&'static str).
/// 2. An `AhoCorasick` automaton used for pattern matching and replacement based on the specific `SimpleMatchType`.
///
/// The function handles different types of `SimpleMatchType` bits and applies various string conversion or deletion operations:
///
/// # Arguments
///
/// * `simple_match_type_bit` - A reference to a `SimpleMatchType` bit that indicates the type of transformation to create a matcher for.
///
/// # Returns
///
/// * `(Vec<&'static str>, AhoCorasick)` - A tuple containing the list of replacement strings and the configured `AhoCorasick` matcher.
///
/// # Matching and Replacement Logic
///
/// * `SimpleMatchType::None` - No transformation is applied.
/// * `SimpleMatchType::Fanjian` - Simplifies traditional Chinese characters to simplified ones based on `FANJIAN` and `UNICODE` conversion mappings.
/// * `SimpleMatchType::WordDelete` - Deletes word-level components including punctuation and whitespace based on `PUNCTUATION_SPECIAL` and `WHITE_SPACE` mappings.
/// * `SimpleMatchType::TextDelete` - Deletes text-level components including special characters and whitespace from `PUNCTUATION_SPECIAL`, `CN_SPECIAL`, `EN_SPECIAL`, and `WHITE_SPACE`.
/// * `SimpleMatchType::Normalize` - Normalizes text using case and variation normalization based on `UPPER_LOWER`, `EN_VARIATION`, and `NUM_NORM` mappings.
/// * `SimpleMatchType::PinYin` - Converts Chinese characters to Pinyin based on the `PINYIN` mapping.
/// * `SimpleMatchType::PinYinChar` - Converts individual Chinese characters to Pinyin based on the `PINYIN_CHAR` mapping.
///
/// After preparing the process dictionary, the function retains only the relevant entries and constructs an `AhoCorasick` matcher with leftmost-longest match kind.
/// The list of replacement strings is also generated from the process dictionary.
///
/// # Example
///
/// ```
/// use matcher_rs::{get_process_matcher, SimpleMatchType};
///
/// let (process_replace_list, process_matcher) = get_process_matcher(SimpleMatchType::TextDelete);
/// ```
pub fn get_process_matcher(
    simple_match_type_bit: SimpleMatchType,
) -> (Vec<&'static str>, AhoCorasick) {
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

    let process_matcher = AhoCorasickBuilder::new()
        .kind(Some(DFA))
        .match_kind(MatchKind::LeftmostLongest)
        .build(
            process_dict
                .iter()
                .map(|(&key, _)| key)
                .collect::<Vec<&str>>(),
        )
        .unwrap();

    let process_replace_list = process_dict.iter().map(|(_, &val)| val).collect();

    (process_replace_list, process_matcher)
}

#[derive(Debug, Clone)]
/// `WordConf` is a structure that holds configuration details for a word used
/// within the `SimpleMatcher`.
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
/// * `word` - A `String` representing the word that is to be configured for matching.
/// * `split_bit` - A SIMD vector (`Simd<u8, WORD_COMBINATION_LIMIT>`) representing the
///   split bits for the word. This vector aids in performing efficient combination
///   matching by storing bitwise information about the word's segments.
///
/// This structure plays a critical role in facilitating efficient text processing
/// and matching within the `SimpleMatcher` by combining textual and SIMD vector data.
struct WordConf {
    word: String,
    split_bit: Simd<u8, WORD_COMBINATION_LIMIT>,
}

#[derive(Debug, Clone)]
/// `SimpleAcTable` is a structure that encapsulates the Aho-Corasick matcher and a list of word configurations.
///
/// This structure is used within the `SimpleMatcher` to hold the compiled Aho-Corasick automaton (`ac_matcher`)
/// and the corresponding configurations for words (`ac_word_conf_list`). The configurations assist in efficient
/// pattern matching and transformation operations by mapping each pattern to its unique word identifier and offset.
///
/// # Fields
///
/// * `ac_matcher` - An instance of the `AhoCorasick` matcher, which is used to perform efficient pattern matching.
/// * `ac_word_conf_list` - A vector of tuples, where each tuple contains:
///     * `u64` - A unique identifier for the word.
///     * `usize` - An offset representing the position or segment of the word within the matcher.
struct SimpleAcTable {
    ac_matcher: AhoCorasick,
    ac_word_conf_list: Vec<(u64, usize)>,
}

#[derive(Debug, Serialize)]
/// `SimpleResult` represents the result of a matching operation.
///
/// This structure is used to store the outcome of a text matching operation performed
/// by the `SimpleMatcher`. It holds details about the matched word, including its
/// unique identifier (`word_id`) and the matched text (`word`). The `SimpleResult`
/// structure is designed to provide a consistent and accessible interface for retrieving
/// the results of text matching operations.
///
/// # Fields
///
/// * `word_id` - A `u64` value representing the unique identifier of the matched word.
/// * `word` - A `Cow<'a, str>` representing the matched text. This allows the text to be
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

/// `SimpleMatcher` is a structure that encapsulates the logic for text matching and transformation
/// based on various `SimpleMatchType` rules.
///
/// This structure holds mappings and configurations for text processing, enabling efficient
/// pattern matching and transformation operations. It includes a mapping of `SimpleMatchType`
/// to process mappings, a mapping of `SimpleMatchType` to Aho-Corasick tables, and a mapping
/// of word IDs to word configurations.
///
/// # Fields
///
/// * `simple_match_type_process_map` - A mapping of `SimpleMatchType` to process mappings, where each
///   process mapping consists of a list of replacement strings and an `AhoCorasick` matcher.
/// * `simple_match_type_ac_table_map` - A mapping of `SimpleMatchType` to `SimpleAcTable`, which contains
///   the Aho-Corasick matcher and word configurations for efficient text matching.
/// * `simple_wordconf_map` - A mapping of word IDs to `WordConf` structures, which hold the textual
///   representation of a word and a SIMD vector representing the split bits for the word.
///
/// # Example
///
/// ```
/// use matcher_rs::{SimpleMatcher, SimpleMatchType, TextMatcherTrait};
///
/// // Initialize word maps and SimpleMatchType instances.
/// let word_maps = vec![
///     (SimpleMatchType::Fanjian, vec![(1, "ChineseWord1"), (2, "ChineseWord2")]),
///     (SimpleMatchType::Normalize, vec![(3, "NormalizationExample1"), (4, "NormalizationExample2")])
/// ];
///
/// // Create a SimpleMatcher instance using the provided word maps.
/// let simple_matcher = SimpleMatcher::new(word_maps);
///
/// // Check if a text matches any patterns based on the configured SimpleMatcher.
/// let text = "ExampleText";
/// let is_match = simple_matcher.is_match(text);
///
/// // Process the input text and return a list of matching results.
/// let results = simple_matcher.process(text);
/// ```
#[derive(Debug, Clone)]
pub struct SimpleMatcher {
    simple_match_type_process_map: IntMap<SimpleMatchType, (Vec<&'static str>, AhoCorasick)>,
    simple_match_type_ac_table_map: IntMap<SimpleMatchType, SimpleAcTable>,
    simple_wordconf_map: IntMap<u64, WordConf>,
}

impl SimpleMatcher {
    /// Constructs a new `SimpleMatcher` from the provided mapping of `SimpleMatchType` to word maps.
    ///
    /// This function initializes a `SimpleMatcher` instance by populating its internal mappings
    /// and configurations based on the provided word maps. Each map entry consists of a pair where:
    ///
    /// - The key is a `SimpleMatchType`, denoting the types of text transformations.
    /// - The value is a word map, where each key-value pair represents a word identifier and its corresponding text.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type_word_map` - An iterator over pairs where each pair contains:
    ///     * `SimpleMatchType` - The bitflags defining specific text transformation rules.
    ///     * An iterable of `(u64, &'a str)` - A word map consisting of unique word identifiers and their corresponding words.
    ///
    /// # Returns
    ///
    /// * `SimpleMatcher` - A configured `SimpleMatcher` instance ready for text matching operations.
    ///
    /// # Example
    ///
    /// ```
    /// use matcher_rs::{SimpleMatcher, SimpleMatchType};
    ///
    /// // Initialize word maps and SimpleMatchType instances.
    /// let word_maps = vec![
    ///     (SimpleMatchType::Fanjian, vec![(1, "ChineseWord1"), (2, "ChineseWord2")]),
    ///     (SimpleMatchType::Normalize, vec![(3, "NormalizationExample1"), (4, "NormalizationExample2")])
    /// ];
    ///
    /// // Create a SimpleMatcher instance using the provided word maps.
    /// let simple_matcher = SimpleMatcher::new(word_maps);
    /// ```
    pub fn new<'a, I, M>(simple_match_type_word_map: I) -> SimpleMatcher
    where
        I: IntoIterator<Item = (SimpleMatchType, M)>,
        M: IntoIterator<Item = (u64, &'a str)>,
    {
        let mut simple_matcher = SimpleMatcher {
            simple_match_type_process_map: IntMap::default(),
            simple_match_type_ac_table_map: IntMap::default(),
            simple_wordconf_map: IntMap::default(),
        };

        for (simple_match_type, simple_word_map) in simple_match_type_word_map {
            for simple_match_type_bit in simple_match_type.iter() {
                simple_matcher
                    .simple_match_type_process_map
                    .entry(simple_match_type_bit)
                    .or_insert_with(|| get_process_matcher(simple_match_type_bit));
            }

            let simple_ac_table = simple_matcher.build_simple_ac_table(
                simple_match_type - SimpleMatchType::TextDelete,
                simple_word_map,
            );

            simple_matcher.simple_match_type_ac_table_map.insert(
                simple_match_type - SimpleMatchType::WordDelete,
                simple_ac_table,
            );
        }

        // Return the configured SimpleMatcher instance
        simple_matcher
    }

    /// Builds a `SimpleAcTable` for the provided `SimpleMatchType` and word map.
    ///
    /// This method constructs a `SimpleAcTable` by taking a `SimpleMatchType` and a word map,
    /// where each entry in the map consists of a word identifier (u64) and its corresponding
    /// text (&'a str). The resulting table includes configurations for Aho-Corasick pattern matching
    /// and word transformations.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type` - A `SimpleMatchType` bit flags that define specific text transformation rules.
    /// * `simple_word_map` - An iterable of `(u64, &'a str)` representing a map of word IDs to their corresponding words.
    ///
    /// # Returns
    ///
    /// * `SimpleAcTable` - A table containing the Aho-Corasick matcher and word configurations for efficient text matching.
    ///
    /// # Detailed Processing:
    ///
    /// 1. Initialize vectors `ac_wordlist` and `ac_word_conf_list` to store words and their configurations.
    /// 2. For each entry in `simple_word_map`:
    ///     - Split the word string by commas and count occurrences of each non-empty segment.
    ///     - Create a vector (`split_bit_vec`) to hold split bit information for up to `WORD_COMBINATION_LIMIT` segments.
    ///     - Calculate the `split_bit` for the word using SIMD and store the word configuration in `simple_wordconf_map`.
    ///     - For each segment in the word, process it through text transformations and add results to `ac_wordlist` and configurations to `ac_word_conf_list`.
    /// 3. Construct the Aho-Corasick matcher using the accumulated word list with ASCII case-insensitivity.
    ///
    /// This method facilitates efficient text processing by building necessary mappings and configurations into the `SimpleAcTable`, which is then used for pattern matching and transformations.
    fn build_simple_ac_table<'a, M>(
        &mut self,
        simple_match_type: SimpleMatchType,
        simple_word_map: M,
    ) -> SimpleAcTable
    where
        M: IntoIterator<Item = (u64, &'a str)>,
    {
        let mut ac_wordlist = Vec::new();
        let mut ac_word_conf_list = Vec::new();

        for (simple_word_id, simple_word) in simple_word_map.into_iter() {
            let mut ac_split_word_counter = AHashMap::default();
            for ac_split_word in simple_word.split(',').filter(|&x| !x.is_empty()) {
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
                    word: simple_word.to_owned(),
                    split_bit,
                },
            );

            for (offset, split_word) in ac_split_word_counter
                .keys()
                .take(WORD_COMBINATION_LIMIT)
                .enumerate()
            {
                for ac_word in self.reduce_text_process(simple_match_type, split_word.as_bytes()) {
                    ac_wordlist.push(ac_word);
                    ac_word_conf_list.push((simple_word_id, offset));
                }
            }
        }

        SimpleAcTable {
            ac_matcher: AhoCorasickBuilder::new()
                .kind(Some(DFA))
                .ascii_case_insensitive(true)
                .build(&ac_wordlist)
                .unwrap(),
            ac_word_conf_list,
        }
    }

    #[inline]
    /// Processes the input text according to the specified `SimpleMatchType` transformations.
    ///
    /// This method takes the input text as a byte slice and applies a sequence of transformations
    /// defined by the `SimpleMatchType`. Each transformation step utilizes a specific Aho-Corasick
    /// matcher and replacement rules to alter the text. The result is a list of processed text
    /// byte sequences, each corresponding to a stage in the transformation pipeline.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type` - A `SimpleMatchType` bit flags that define specific text transformation rules.
    /// * `text_bytes` - A byte slice representing the input text to be processed.
    ///
    /// # Returns
    ///
    /// * `ArrayVec<[Cow<'a, [u8]>; 8]>` - A vector containing the processed text byte slices at various stages of the transformation.
    ///
    /// # Detailed Processing:
    ///
    /// 1. Initialize an `ArrayVec` to hold the processed text byte slices.
    /// 2. Push the original text bytes (borrowed) as the first element in the vector.
    /// 3. Iterate through each bit in the `SimpleMatchType` to apply corresponding transformation rules.
    /// 4. For each `SimpleMatchType` bit:
    ///     a. Retrieve the relevant `process_replace_list` and `process_matcher` from the `simple_match_type_process_map`.
    ///     b. Get the last processed text bytes from the vector for transformation.
    ///     c. Apply transformation rules based on the `SimpleMatchType` bit:
    ///         - `None`: No transformation.
    ///         - `Fanjian`: Replace bytes using the Aho-Corasick matcher if a match is found.
    ///         - `TextDelete` or `WordDelete`: Delete matching segments from the text.
    ///         - Otherwise, replace bytes using the Aho-Corasick matcher.
    /// 5. Append each transformed text bytes sequence to the vector if a modification occurred.
    ///
    /// This method ensures that each text byte sequence goes through the specified transformations
    /// efficiently, using SIMD and Aho-Corasick automata.
    ///
    fn reduce_text_process<'a>(
        &self,
        simple_match_type: SimpleMatchType,
        text_bytes: &'a [u8],
    ) -> ArrayVec<[Cow<'a, [u8]>; 8]> {
        let mut processed_text_bytes_list: ArrayVec<[Cow<'a, [u8]>; 8]> = ArrayVec::new();
        processed_text_bytes_list.push(Cow::Borrowed(text_bytes));

        for simple_match_type_bit in simple_match_type.iter() {
            let (process_replace_list, process_matcher) = unsafe {
                self.simple_match_type_process_map
                    .get(&simple_match_type_bit)
                    .unwrap_unchecked()
            };
            let tmp_processed_text_bytes =
                unsafe { processed_text_bytes_list.last_mut().unwrap_unchecked() };

            match simple_match_type_bit {
                SimpleMatchType::None => {}
                SimpleMatchType::Fanjian => {
                    if unlikely(process_matcher.is_match(tmp_processed_text_bytes.as_ref())) {
                        *tmp_processed_text_bytes = Cow::Owned(
                            process_matcher.replace_all_bytes(text_bytes, process_replace_list),
                        );
                    }
                }
                SimpleMatchType::TextDelete | SimpleMatchType::WordDelete => {
                    if likely(process_matcher.is_match(tmp_processed_text_bytes.as_ref())) {
                        let mut processed_text_bytes =
                            Vec::with_capacity(tmp_processed_text_bytes.len());
                        let mut last_match = 0;

                        for mat in process_matcher.find_iter(tmp_processed_text_bytes.as_ref()) {
                            processed_text_bytes.extend(unsafe {
                                tmp_processed_text_bytes.get_unchecked(last_match..mat.start())
                            });
                            last_match = mat.end();
                        }
                        processed_text_bytes.extend(unsafe {
                            tmp_processed_text_bytes.get_unchecked(last_match..)
                        });

                        processed_text_bytes_list.push(Cow::Owned(processed_text_bytes));
                    }
                }
                _ => {
                    if process_matcher.is_match(tmp_processed_text_bytes.as_ref()) {
                        let processed_text_bytes = process_matcher
                            .replace_all_bytes(tmp_processed_text_bytes, process_replace_list);
                        processed_text_bytes_list.push(Cow::Owned(processed_text_bytes));
                    }
                }
            }
        }

        processed_text_bytes_list
    }
}

impl<'a> TextMatcherTrait<'a, SimpleResult<'a>> for SimpleMatcher {
    /// Checks if the input text matches any patterns based on the configured `SimpleMatcher`.
    ///
    /// This method processes the input text according to the transformation rules defined by the
    /// `SimpleMatchType`, using Aho-Corasick pattern matching to search for overlapping matches.
    /// The method returns `true` if any pattern matches the text, otherwise it returns `false`.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be checked for matches.
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if a match is found, otherwise returns `false`.
    ///
    /// # Detailed Processing:
    ///
    /// 1. Convert the input text to a byte slice (`text_bytes`).
    /// 2. If the byte slice is empty, return `false`.
    /// 3. Initialize a map (`word_id_split_bit_map`) to track split bit vectors for each word ID.
    /// 4. Iterate over each `SimpleMatchType` and its associated `SimpleAcTable`:
    ///     a. Process the text according to the transformation rules defined by the `SimpleMatchType`.
    ///     b. Iterate over each processed version of the text.
    ///     c. Use the Aho-Corasick matcher to find overlapping patterns in the processed text.
    ///     d. Retrieve the word configuration based on the pattern found.
    ///     e. Initialize or update the split bit vector corresponding to the word ID.
    ///     f. Update the split bit vector by shifting the bit to the right.
    ///     g. Check if all shifts have reduced the split bit vector to all zeros.
    ///     h. If so, return `true` indicating a match is found.
    ///
    /// This method efficiently checks for text matches using SIMD and Aho-Corasick algorithms.
    fn is_match(&self, text: &str) -> bool {
        let text_bytes = text.as_bytes();

        if unlikely(text_bytes.is_empty()) {
            return false;
        }

        let mut word_id_split_bit_map = IntMap::default();

        for (&simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            let processed_text_bytes_list = self.reduce_text_process(simple_match_type, text_bytes);
            let processed_times = processed_text_bytes_list.len();

            for (index, processed_text) in processed_text_bytes_list.iter().enumerate() {
                for ac_result in simple_ac_table
                    .ac_matcher
                    .find_overlapping_iter(processed_text)
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

                    if unlikely(
                        split_bit_vec
                            .iter()
                            .fold(Simd::splat(1), |acc, &bit| acc & bit)
                            == ZEROS,
                    ) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Processes the input text and returns a list of matching results.
    ///
    /// This function performs text matching by applying various `SimpleMatchType` transformations
    /// to the input text and using Aho-Corasick pattern matching to find overlapping matches. It
    /// maintains a set of identified word IDs to ensure each word is only added once to the result
    /// list, regardless of how many transformations or pattern matches occur.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    ///
    /// * `Vec<SimpleResult<'a>>` - A vector containing the matching results as `SimpleResult` instances.
    ///
    /// # Detailed Processing:
    ///
    /// 1. Convert the input text to a byte slice (`text_bytes`).
    /// 2. Initialize an empty vector (`result_list`) to store the matching results.
    /// 3. Return an empty result list if the input text is empty.
    /// 4. Initialize sets and maps to track word IDs and their split bit vectors during processing.
    /// 5. Iterate over each `SimpleMatchType` and its associated `SimpleAcTable`:
    ///     a. Process the text according to the transformation rules defined by the `SimpleMatchType`.
    ///     b. Iterate over each processed version of the text.
    ///     c. Use the Aho-Corasick matcher to find overlapping patterns in the processed text.
    ///     d. Retrieve the word configuration based on the pattern found.
    ///     e. Skip further processing if the word ID is already in the result set.
    ///     f. Initialize or update the split bit vector corresponding to the word ID.
    ///     g. Update the split bit vector by shifting the bit to the right.
    ///     h. Check if all shifts have reduced the split bit vector to all zeros.
    ///     i. If so, add the word ID to the result set and append the matching word to the result list.
    ///
    /// This function ensures that text matching and transformation are performed efficiently
    /// using SIMD and Aho-Corasick algorithms, and it returns a list of unique matching results.
    fn process(&'a self, text: &str) -> Vec<SimpleResult<'a>> {
        let text_bytes = text.as_bytes();
        let mut result_list = Vec::new();

        if unlikely(text_bytes.is_empty()) {
            return result_list;
        }

        let mut word_id_set = IntSet::default();
        let mut word_id_split_bit_map = IntMap::default();

        for (&simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            let processed_text_bytes_list = self.reduce_text_process(simple_match_type, text_bytes);
            let processed_times = processed_text_bytes_list.len(); // Get the number of processed versions of the text

            for (index, processed_text) in processed_text_bytes_list.iter().enumerate() {
                for ac_result in simple_ac_table
                    .ac_matcher
                    .find_overlapping_iter(processed_text)
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

                    if unlikely(
                        split_bit_vec
                            .iter()
                            .fold(Simd::splat(1), |acc, &bit| acc & bit)
                            == ZEROS,
                    ) {
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
