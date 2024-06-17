use std::borrow::Cow;
use std::intrinsics::unlikely;
use std::iter;
use std::simd::Simd;

use ahash::AHashMap;
use aho_corasick::{
    AhoCorasick, AhoCorasickBuilder, AhoCorasickKind::DFA, MatchKind as AhoCorasickMatchKind,
};
use bitflags::bitflags;
use daachorse::{
    CharwiseDoubleArrayAhoCorasick, CharwiseDoubleArrayAhoCorasickBuilder,
    MatchKind as DoubleArrayAhoCorasickMatchKind,
};
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

#[derive(Clone)]
pub enum ProcessMatcher {
    Chinese(CharwiseDoubleArrayAhoCorasick<u64>),
    Others(AhoCorasick),
}

impl ProcessMatcher {
    // #[inline(always)]
    // fn is_match(&self, text: &str) -> bool {
    //     match self {
    //         ProcessMatcher::Chinese(ac) => ac.find_iter(text).next().is_some(),
    //         ProcessMatcher::Others(ac) => ac.is_match(text),
    //     }
    // }

    #[inline(always)]
    fn replace_all<'a>(
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
            result.push_str(&text[last_end..]);
            (true, Cow::Owned(result))
        } else {
            (false, Cow::Borrowed(text))
        }
    }

    #[inline(always)]
    fn delete_all<'a>(&self, text: &'a str) -> (bool, Cow<'a, str>) {
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
            result.push_str(&text[last_end..]);
            (true, Cow::Owned(result))
        } else {
            (false, Cow::Borrowed(text))
        }
    }
}

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
        SimpleMatchType::Fanjian | SimpleMatchType::PinYin | SimpleMatchType::PinYinChar => {
            let process_matcher = CharwiseDoubleArrayAhoCorasickBuilder::new()
                .match_kind(DoubleArrayAhoCorasickMatchKind::Standard)
                .build(
                    process_dict
                        .iter()
                        .map(|(&key, _)| key)
                        .collect::<Vec<&str>>(),
                )
                .unwrap();
            (
                process_replace_list,
                ProcessMatcher::Chinese(process_matcher),
            )
        }
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
#[derive(Clone)]
pub struct SimpleMatcher {
    simple_match_type_process_map: IntMap<SimpleMatchType, (Vec<&'static str>, ProcessMatcher)>,
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

    /// Builds a `SimpleAcTable` from the provided word map and `SimpleMatchType`.
    ///
    /// This function constructs a `SimpleAcTable` by iterating through the provided `simple_word_map`,
    /// processing each word according to the specified `simple_match_type`. It collects words to be
    /// matched (using Aho-Corasick algorithm) and their corresponding configurations into vectors.
    /// The resulting `SimpleAcTable` contains both the matcher and word configuration list.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type` - A `SimpleMatchType` bit flags that define specific text transformation rules.
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

            for (offset, &split_word) in ac_split_word_counter
                .keys()
                .take(WORD_COMBINATION_LIMIT)
                .enumerate()
            {
                for ac_word in self.reduce_text_process(simple_match_type, split_word) {
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

    #[inline(always)]
    /// Applies various transformations to the input text based on the specified `SimpleMatchType`.
    ///
    /// This function processes the input text according to the bit flags defined in `SimpleMatchType`.
    /// It iterates over each bit flag and applies the corresponding transformation, utilizing pattern
    /// matching and replacement rules if defined. The resulting list of processed texts includes the
    /// original text and any transformed versions.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type` - A `SimpleMatchType` bit flags that define specific text transformation rules.
    /// * `text` - A string slice representing the input text to be processed.
    ///
    /// # Returns
    ///
    /// * `ArrayVec<[Cow<'a, str>; 8]>` - A vector containing the original and transformed text versions.
    ///
    /// # Detailed Processing:
    ///
    /// 1. Initialize the `processed_text_list` with the original input text.
    /// 2. Iterate through each bit flag in `simple_match_type`:
    ///     a. Retrieve corresponding replacement list and matcher from `simple_match_type_process_map`.
    ///     b. Obtain the last processed text from `processed_text_list`.
    ///     c. Apply the transformation based on the current bit flag:
    ///         - If `SimpleMatchType::None`, do nothing.
    ///         - If `SimpleMatchType::Fanjian`, replace matching patterns if any.
    ///         - If `SimpleMatchType::TextDelete` or `SimpleMatchType::WordDelete`, delete matching segments.
    ///         - Otherwise, apply the replacement if a match is found.
    /// 3. Add the processed text to `processed_text_list` if a transformation was applied.
    ///
    /// This function ensures that all specified text transformations are applied sequentially,
    /// and the resulting list of texts is returned for further processing.
    fn reduce_text_process<'a>(
        &self,
        simple_match_type: SimpleMatchType,
        text: &'a str,
    ) -> ArrayVec<[Cow<'a, str>; 8]> {
        let mut processed_text_list: ArrayVec<[Cow<'a, str>; 8]> = ArrayVec::new();
        processed_text_list.push(Cow::Borrowed(text));

        for simple_match_type_bit in simple_match_type.iter() {
            let (process_replace_list, process_matcher) = unsafe {
                self.simple_match_type_process_map
                    .get(&simple_match_type_bit)
                    .unwrap_unchecked()
            };
            let tmp_processed_text = unsafe { processed_text_list.last_mut().unwrap_unchecked() };

            match (simple_match_type_bit, process_matcher) {
                (SimpleMatchType::None, _) => {}
                (SimpleMatchType::Fanjian, pm) => {
                    match pm.replace_all(tmp_processed_text.as_ref(), process_replace_list) {
                        (true, Cow::Owned(tx)) => {
                            *tmp_processed_text = Cow::Owned(tx);
                        }
                        (false, _) => {}
                        (_, _) => unreachable!(),
                    }
                }
                (SimpleMatchType::TextDelete | SimpleMatchType::WordDelete, pm) => {
                    match pm.delete_all(tmp_processed_text.as_ref()) {
                        (true, Cow::Owned(tx)) => {
                            processed_text_list.push(Cow::Owned(tx));
                        }
                        (false, _) => {}
                        (_, _) => unreachable!(),
                    }
                }
                (_, pm) => {
                    match pm.replace_all(tmp_processed_text.as_ref(), process_replace_list) {
                        (true, Cow::Owned(tx)) => {
                            processed_text_list.push(Cow::Owned(tx));
                        }
                        (false, _) => {}
                        (_, _) => unreachable!(),
                    }
                }
            }
        }

        processed_text_list
    }
}

impl<'a> TextMatcherTrait<'a, SimpleResult<'a>> for SimpleMatcher {
    /// Determines if any patterns match the input text after applying transformations.
    ///
    /// This function checks if any patterns from the provided `SimpleMatchType` transformations
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
    /// 3. Iterate over each `SimpleMatchType` and its associated `SimpleAcTable`:
    ///     a. Process the text according to the transformation rules defined by the `SimpleMatchType`.
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
    /// while accounting for various transformations specified by the `SimpleMatchType`.
    fn is_match(&self, text: &str) -> bool {
        if unlikely(text.is_empty()) {
            return false;
        }

        let mut word_id_split_bit_map = IntMap::default();

        for (&simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            let processed_text_list = self.reduce_text_process(simple_match_type, text);
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
    /// * `Vec<SimpleResult<'a>>` - A vector containing the results of the match, each result includes
    ///   the word id and the word itself.
    ///
    /// # Detailed Processing:
    ///
    /// 1. If the input text is empty, return an empty list of results.
    /// 2. Initialize a set to track word IDs that have already been matched.
    /// 3. Initialize a map to keep track of word IDs and their corresponding split bit vectors
    ///    during processing.
    /// 4. For each `SimpleMatchType` and its corresponding `SimpleAcTable`:
    ///     a. Apply the transformation rules defined by the `SimpleMatchType` to process the text.
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

        if unlikely(text.is_empty()) {
            return result_list;
        }

        let mut word_id_set = IntSet::default();
        let mut word_id_split_bit_map = IntMap::default();

        for (&simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            let processed_text_list = self.reduce_text_process(simple_match_type, text);
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
