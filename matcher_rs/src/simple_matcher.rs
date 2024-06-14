use std::borrow::Cow;
use std::intrinsics::{likely, unlikely};
use std::iter;
use std::simd::Simd;

use ahash::{AHashMap, AHashSet};
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind::DFA, MatchKind};
use nohash_hasher::{IntMap, IntSet, IsEnabled};
use serde::Serialize;
use tinyvec::ArrayVec;

use super::{MatchResultTrait, StrConvType, TextMatcherTrait};

/// This section includes constant string references to various conversion maps.
///
/// Each constant is assigned the contents of a corresponding text file using
/// `include_str!`. This macro inserts the contents of the given file into the binary as
/// a `&'static str`. These files contain mappings for string transformations used in the
/// text processing implemented by the `SimpleMatcher` struct. These mappings are expected to be found
/// at the relative paths provided in the macro argument.
///
/// Included conversion maps:
///
/// - `FANJIAN`: Maps simplified Chinese characters to traditional Chinese characters.
/// - `CN_SPECIAL`: Maps Chinese special characters to their normalized forms.
/// - `EN_SPECIAL`: Maps English special characters to their normalized forms.
/// - `PUNCTUATION_SPECIAL`: Maps various punctuation marks to an empty string (effectively deleting them).
/// - `EN_VARIATION`: Maps English characters in different variations to their standard forms.
/// - `UNICODE`: Maps various Unicode characters to their equivalent forms.
/// - `NUM_NORM`: Maps numeric characters to their normalized forms.
/// - `UPPER_LOWER`: Maps uppercase English characters to their lowercase equivalents.
/// - `PINYIN`: Maps Chinese characters to Pinyin representations.
/// - `PINYIN_CHAR`: Maps individual Chinese characters to their Pinyin equivalents.
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

/// Type alias for `StrConvType` which is used to represent various string
/// conversion modes in the text matcher. This alias simplifies the
/// representation and usage of `StrConvType` throughout the `SimpleMatcher`
/// implementation.
///
/// `StrConvType` includes different conversion types which determine the
/// preprocessing steps applied to the text before matching, such as
/// normalization, punctuation deletion, and more.
pub type SimpleMatchType = StrConvType;

impl IsEnabled for SimpleMatchType {}

/// Type alias for a mapping between `SimpleMatchType` and an `IntMap` containing word IDs
/// and references to associated words.
///
/// This alias is used within the `SimpleMatcher` struct to define and organize the
/// different word maps associated with each `SimpleMatchType`. The key is a `SimpleMatchType`
/// which indicates the type of string conversion to be applied, and the value is an
/// `IntMap` where the key is a `u64` representing a unique word ID, and the value is
/// a reference to a string slice representing the word.
///
/// The lifetime parameter `'a` ensures that the string slices referenced in the map
/// live at least as long as the map itself.
///
/// # Example
///
/// ```rust
/// use matcher_rs::{SimpleMatchType, SimpleMatchTypeWordMap};
/// use nohash_hasher::IntMap;
///
/// let mut simple_match_type_word_map: SimpleMatchTypeWordMap<'_> = IntMap::default();
/// let mut simple_word_map = IntMap::default();
///
/// simple_word_map.insert(1, "你好");
/// simple_word_map.insert(2, "123");
///
/// simple_match_type_word_map.insert(SimpleMatchType::Fanjian, simple_word_map);
/// ```
///
pub type SimpleMatchTypeWordMap<'a> = IntMap<SimpleMatchType, IntMap<u64, &'a str>>;

/// Configuration for a word used in the `SimpleMatcher`.
///
/// `WordConf` structure stores the configuration associated with a word. This includes the word itself
/// as a `String` and a SIMD vector `split_bit` that helps with efficient matching and processing of
/// word combinations.
///
/// Fields:
///
/// * `word` - A `String` representing the word.
/// * `split_bit` - A `Simd` vector of `u8` with a size of `WORD_COMBINATION_LIMIT`.
///   This vector is used to store bitwise information for word combination matching,
///   allowing optimized and efficient comparisons.
struct WordConf {
    word: String,
    split_bit: Simd<u8, WORD_COMBINATION_LIMIT>,
}

/// A structure used within the `SimpleMatcher` to associate the text processing
/// table (`ac_matcher`) with its corresponding word configurations (`ac_word_conf_list`).
///
/// The `SimpleAcTable` struct is essential for the functioning of the Aho-Corasick
/// automaton, enabling efficient text search and match operations.
///
/// # Fields
///
/// * `ac_matcher` - An instance of the `AhoCorasick` Aho-Corasick automaton for
///   efficiently finding patterns in text. Built from a list of patterns that require
///   processing.
///
/// * `ac_word_conf_list` - A vector containing tuples of word IDs and corresponding
///   offsets. It serves as a map between the patterns used by `ac_matcher` and
///   their respective configurations, indicated by the word ID and its specific offset.
///
/// This structure works by holding the compiled automaton (`ac_matcher`) which
/// quickly searches through text for various patterns. The patterns correspond to
/// word configurations stored in `ac_word_conf_list`, linking matched positions in the
/// text to predefined transformations or actions.
struct SimpleAcTable {
    ac_matcher: AhoCorasick,
    ac_word_conf_list: Vec<(u64, usize)>,
}

#[derive(Debug, Serialize)]
/// A struct representing a result for a matched word in the `SimpleMatcher`.
///
/// `SimpleResult` is used to encapsulate information about a word that has been
/// matched by the `SimpleMatcher` during text processing. It includes the word's
/// unique identifier and the corresponding word itself.
///
/// # Fields
///
/// * `word_id` - A `u64` representing a unique identifier for the matched word.
/// * `word` - A `Cow` (Clone on Write) representing the matched word. Using `Cow`
///   allows the struct to efficiently manage borrowed or owned data.
///
/// # Example
///
/// ```
/// use std::borrow::Cow;
/// use matcher_rs::SimpleResult;
///
/// let result = SimpleResult {
///     word_id: 42,
///     word: Cow::Borrowed("example"),
/// };
///
/// println!("Matched word: ID = {}, word = {}", result.word_id, result.word);
/// ```
pub struct SimpleResult<'a> {
    pub word_id: u64,
    pub word: Cow<'a, str>,
}

impl MatchResultTrait<'_> for SimpleResult<'_> {
    /// Returns the unique identifier of the matched word.
    ///
    /// This method provides the ID associated with a word that has been
    /// matched by the `SimpleMatcher`. The ID is useful for identifying and
    /// differentiating between multiple matched words.
    ///
    /// # Returns
    ///
    /// * `u64` - A 64-bit unsigned integer representing the unique identifier
    ///   for the matched word.
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
    fn word_id(&self) -> u64 {
        self.word_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

/// `SimpleMatcher` is a main structure for performing text matching operations
/// using various pre-defined string transformation rules.
///
/// This structure holds several maps and configurations that facilitate efficient
/// text processing and matching using the Aho-Corasick algorithm. The primary aim is
/// to transform and match input text against a set of rules and patterns efficiently.
///
/// # Fields
///
/// * `simple_match_type_process_map` - A mapping between `SimpleMatchType` and a tuple
///   containing a list of replacement strings and an instance of `AhoCorasick`. It is used
///   for pre-processing input text using specific transformation rules.
///
/// * `simple_match_type_ac_table_map` - A mapping between `SimpleMatchType` and `SimpleAcTable`.
///   This map holds the compiled Aho-Corasick automata (AC tables) and their associated word
///   configurations that are used for the efficient pattern matching.
///
/// * `simple_wordconf_map` - An `IntMap` that maps unique word IDs (`u64`) to their corresponding
///   `WordConf` configurations. This helps in storing custom configurations for words,
///   including SIMD vectors for efficient combination matching.
///
/// * `min_chars_count` - A `usize` value representing the minimum number of characters
///   required for a valid match. It is determined based on the words present in the matcher.
///   This value helps in optimizing the matching process by filtering out text that is too
///   short to contain any valid matches.
pub struct SimpleMatcher {
    simple_match_type_process_map: IntMap<SimpleMatchType, (Vec<&'static str>, AhoCorasick)>,
    simple_match_type_ac_table_map: IntMap<SimpleMatchType, SimpleAcTable>,
    simple_wordconf_map: IntMap<u64, WordConf>,
    min_chars_count: usize,
}

impl SimpleMatcher {
    /// Creates a new instance of `SimpleMatcher` using the provided `SimpleMatchTypeWordMap`.
    ///
    /// This constructor initializes the `SimpleMatcher` by setting up process maps and
    /// Aho-Corasick (AC) tables for efficient text matching. The mappings and configurations
    /// for each `SimpleMatchType` are extracted from the provided word map and stored
    /// within the matcher for later use.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type_word_map` - A reference to a `SimpleMatchTypeWordMap` which
    ///   contains the mappings between various `SimpleMatchType`s and their corresponding
    ///   word maps (`IntMap<u64, &str>`). This serves as the input for initializing the
    ///   matcher with the required configurations and patterns.
    ///
    /// # Returns
    ///
    /// A new instance of `SimpleMatcher` with all the necessary mappings and configurations
    /// set up for text matching.
    ///
    /// # Example
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatchType, SimpleMatchTypeWordMap, SimpleMatcher};
    /// use nohash_hasher::IntMap;
    ///
    /// let mut simple_match_type_word_map: SimpleMatchTypeWordMap<'_> = IntMap::default();
    /// let mut simple_word_map = IntMap::default();
    ///
    /// simple_word_map.insert(1, "你好");
    /// simple_word_map.insert(2, "123");
    ///
    /// simple_match_type_word_map.insert(SimpleMatchType::Fanjian, simple_word_map);
    ///
    /// let matcher = SimpleMatcher::new(simple_match_type_word_map);
    /// ```
    pub fn new<'a, I, M>(simple_match_type_word_map: I) -> SimpleMatcher
    where
        I: IntoIterator<Item = (SimpleMatchType, M)>,
        M: IntoIterator<Item = (u64, &'a str)>,
    {
        // Create a new instance of SimpleMatcher with default values
        let mut simple_matcher = SimpleMatcher {
            simple_match_type_process_map: IntMap::default(),
            simple_match_type_ac_table_map: IntMap::default(),
            simple_wordconf_map: IntMap::default(),
            min_chars_count: usize::MAX,
        };

        // Iterate over each entry in the provided SimpleMatchTypeWordMap
        for (simple_match_type, simple_word_map) in simple_match_type_word_map {
            // Iterate over each bit set in the SimpleMatchType
            for simple_match_type_bit in simple_match_type.iter() {
                // Insert a new process matcher into the process map if it doesn't exist already
                simple_matcher
                    .simple_match_type_process_map
                    .entry(simple_match_type_bit)
                    .or_insert_with(|| Self::_get_process_matcher(&simple_match_type_bit));
            }

            // Build the Aho-Corasick table for the current SimpleMatchType excluding TextDelete
            let simple_ac_table = simple_matcher.build_simple_ac_table(
                &(simple_match_type - SimpleMatchType::TextDelete),
                simple_word_map,
            );

            // Insert the built AC table into the AC table map,
            // using SimpleMatchType excluding WordDelete as the key
            simple_matcher.simple_match_type_ac_table_map.insert(
                simple_match_type - SimpleMatchType::WordDelete,
                simple_ac_table,
            );
        }

        // Return the configured SimpleMatcher instance
        simple_matcher
    }

    /// Generates the process matcher for a given `SimpleMatchType`.
    ///
    /// This function constructs a mapping between input patterns and their corresponding
    /// replacement strings based on the provided `SimpleMatchType`. The patterns and their
    /// replacements are used to build an Aho-Corasick (AC) automaton, which efficiently
    /// matches and replaces text during the preprocessing phase.
    ///
    /// The function considers various string transformation rules, which are classified
    /// under different `SimpleMatchType` values such as `Fanjian`, `WordDelete`, `TextDelete`,
    /// `Normalize`, `PinYin`, and `PinYinChar`. Depending on the type, it loads the
    /// corresponding conversion data, creates a dictionary of patterns and their replacements,
    /// and then builds the AC automaton.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type_bit` - A reference to a `SimpleMatchType` indicating the type
    ///   of string transformation to be applied.
    ///
    /// # Returns
    ///
    /// A tuple containing:
    /// * `process_replace_list` - A vector of replacement strings (&'static str) used for
    ///   transforming the input patterns.
    /// * `process_matcher` - An `AhoCorasick` automaton built using the input patterns,
    ///   which facilitates efficient matching and replacement operations.
    ///
    /// # Example
    ///
    /// ```rust
    /// use matcher_rs::{SimpleMatchType, SimpleMatcher};
    ///
    /// let simple_match_type = SimpleMatchType::Fanjian;
    /// let (process_replace_list, process_matcher) = SimpleMatcher::_get_process_matcher(&simple_match_type);
    ///
    /// // Use the returned process_replace_list and process_matcher for further text processing
    /// ```
    pub fn _get_process_matcher(
        simple_match_type_bit: &SimpleMatchType,
    ) -> (Vec<&'static str>, AhoCorasick) {
        // Create a mutable dictionary (hash map) to store process pairs.
        let mut process_dict = AHashMap::default();

        // Match against the specific string conversion type.
        match *simple_match_type_bit {
            // If no conversion type specified, do nothing.
            SimpleMatchType::None => {}

            // For Fanjian conversion: process FANJIAN and UNICODE data files.
            SimpleMatchType::Fanjian => {
                for str_conv_dat in [FANJIAN, UNICODE] {
                    // Extend the process dictionary with mappings from the conversion data.
                    process_dict.extend(str_conv_dat.trim().lines().map(|pair_str| {
                        let mut pair_str_split = pair_str.split('\t');
                        (
                            // Each line in the conversion data corresponds to a key-value pair.
                            pair_str_split.next().unwrap(),
                            pair_str_split.next().unwrap(),
                        )
                    }));
                }
            }

            // For WordDelete conversion: process punctuation and whitespace characters.
            SimpleMatchType::WordDelete => {
                process_dict.extend(
                    PUNCTUATION_SPECIAL
                        .trim()
                        .lines()
                        // Map each punctuation character to an empty string (deletion).
                        .map(|pair_str| (pair_str, "")),
                );

                // Map each whitespace character to an empty string (deletion).
                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }

            // For TextDelete conversion: process punctuation, Chinese special, and English special characters.
            SimpleMatchType::TextDelete => {
                for str_conv_dat in [PUNCTUATION_SPECIAL, CN_SPECIAL, EN_SPECIAL] {
                    process_dict.extend(
                        str_conv_dat
                            .trim()
                            .lines()
                            // Map each special character to an empty string (deletion).
                            .map(|pair_str| (pair_str, "")),
                    );
                }

                // Map each whitespace character to an empty string (deletion).
                process_dict.extend(WHITE_SPACE.iter().map(|&c| (c, "")));
            }
            // For Normalize conversion: process UPPER_LOWER, EN_VARIATION, and NUM_NORM data files.
            SimpleMatchType::Normalize => {
                for str_conv_dat in [UPPER_LOWER, EN_VARIATION, NUM_NORM] {
                    // Extend the process dictionary with mappings from the conversion data.
                    process_dict.extend(str_conv_dat.trim().lines().map(|pair_str| {
                        let mut pair_str_split = pair_str.split('\t');
                        (
                            // Each line in the conversion data corresponds to a key-value pair.
                            pair_str_split.next().unwrap(),
                            pair_str_split.next().unwrap(),
                        )
                    }));
                }
            }

            // For PinYin conversion: process PINYIN data file.
            SimpleMatchType::PinYin => {
                process_dict.extend(PINYIN.trim().lines().map(|pair_str| {
                    let mut pair_str_split = pair_str.split('\t');
                    (
                        // Each line in the conversion data corresponds to a key-value pair.
                        pair_str_split.next().unwrap(),
                        pair_str_split.next().unwrap(),
                    )
                }));
            }

            // For PinYinChar conversion: process PINYIN_CHAR data file.
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

            // Ignore unknown or unsupported conversion types.
            _ => {}
        }

        // Remove entries where the key starts with '#' (except key "#")
        // or where the key and value are identical.
        process_dict
            .retain(|&key, &mut value| (key == "#" || !key.starts_with('#')) && key != value);

        // Build an Aho-Corasick automaton (process_matcher) for efficient matching.
        let process_matcher = AhoCorasickBuilder::new()
            .kind(Some(DFA))
            .match_kind(MatchKind::LeftmostLongest)
            .build(
                // Collect all keys (patterns) to be matched.
                process_dict
                    .iter()
                    .map(|(&key, _)| key)
                    .collect::<Vec<&str>>(),
            )
            .unwrap();

        // Collect the corresponding replacement values.
        let process_replace_list = process_dict.iter().map(|(_, &val)| val).collect();

        // Return the tuple containing the replacement list and the process matcher.
        (process_replace_list, process_matcher)
    }

    /// Constructs a SimpleAcTable for a given SimpleMatchType and word map.
    ///
    /// This method creates an Aho-Corasick automaton and its corresponding word configurations
    /// based on the provided SimpleMatchType and word map. It processes the word map to generate
    /// split words and their corresponding configuration for efficient matching.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type` - A reference to a `SimpleMatchType` indicating the type of string
    ///   transformation to be applied.
    /// * `simple_word_map` - A reference to an `IntMap<u64, &str>` that maps unique word IDs to
    ///   their corresponding string slices.
    ///
    /// # Returns
    ///
    /// A `SimpleAcTable` instance containing the Aho-Corasick automaton (`ac_matcher`) and a list
    /// of word configurations (`ac_word_conf_list`). The automaton is built from processed split
    /// words, and the configuration list maps word IDs to their respective offsets.
    fn build_simple_ac_table<'a, M>(
        &mut self,
        simple_match_type: &SimpleMatchType,
        simple_word_map: M,
    ) -> SimpleAcTable
    where
        M: IntoIterator<Item = (u64, &'a str)>,
    {
        // Initialize vectors to hold the list of Aho-Corasick words and their configurations.
        let mut ac_wordlist = Vec::new();
        let mut ac_word_conf_list = Vec::new();

        // Iterate over each entry in the simple_word_map.
        for (simple_word_id, simple_word) in simple_word_map.into_iter() {
            // Update the minimum character count required for matching.
            self.min_chars_count = self.min_chars_count.min(
                simple_word
                    .chars()
                    .filter(|&c| c != ',') // Exclude commas from the character count.
                    .collect::<AHashSet<char>>()
                    .len(),
            );

            // Create a counter for split words in the current simple_word.
            let mut ac_split_word_counter = AHashMap::default();
            for ac_split_word in simple_word.split(',').filter(|&x| !x.is_empty()) {
                ac_split_word_counter
                    .entry(ac_split_word)
                    .and_modify(|cnt| *cnt += 1) // Increment the count if the split word already exists.
                    .or_insert(1); // Initialize the count to 1 if it's a new split word.
            }

            // Create a vector of split bits based on the split word counts, up to the WORD_COMBINATION_LIMIT.
            let split_bit_vec = ac_split_word_counter
                .values()
                .take(WORD_COMBINATION_LIMIT)
                .map(|&x| 1 << (x.min(8) - 1)) // Ensure the bit shift does not exceed 8.
                .collect::<ArrayVec<[u8; WORD_COMBINATION_LIMIT]>>();
            let split_bit = Simd::load_or_default(&split_bit_vec); // Load split bits into a SIMD vector.

            // Insert the word configuration into the simple_wordconf_map.
            self.simple_wordconf_map.insert(
                simple_word_id,
                WordConf {
                    word: simple_word.to_owned(), // Convert the borrowed string slice to an owned String.
                    split_bit,
                },
            );

            // Process each split word and add it to the Aho-Corasick word list and configuration list.
            for (offset, split_word) in ac_split_word_counter
                .keys()
                .take(WORD_COMBINATION_LIMIT)
                .enumerate()
            {
                for ac_word in self.reduce_text_process(simple_match_type, split_word.as_bytes()) {
                    ac_wordlist.push(ac_word);
                    ac_word_conf_list.push((simple_word_id, offset)); // Track the word ID and its offset.
                }
            }
        }

        // Return a SimpleAcTable instance with the built Aho-Corasick matcher and word configurations.
        SimpleAcTable {
            ac_matcher: AhoCorasickBuilder::new()
                .kind(Some(DFA))
                .ascii_case_insensitive(true) // Enable case-insensitive matching.
                .build(&ac_wordlist)
                .unwrap(), // Build the Aho-Corasick matcher and handle any errors.
            ac_word_conf_list,
        }
    }

    #[inline]
    /// Processes the text through a sequence of transformations based on the `SimpleMatchType`.
    ///
    /// This method applies various string transformations to the input text bytes, producing
    /// multiple versions of the text according to the rules specified in the `SimpleMatchType`.
    /// Each transformation step is performed using pre-built Aho-Corasick matchers and their
    /// replacement rules, and the processed results are stored in a vector.
    ///
    /// The processing is done by iterating over each bit of the `SimpleMatchType`, fetching
    /// the corresponding processor from the map, and applying the transformations. Depending on the
    /// match type, transformations could involve substitutions, deletions, or other modifications.
    ///
    /// # Arguments
    ///
    /// * `simple_match_type` - A reference to a `SimpleMatchType` that indicates the sequence of
    ///   transformations to be applied.
    /// * `text_bytes` - A byte slice containing the input text to be processed.
    ///
    /// # Returns
    ///
    /// * An `ArrayVec` containing up to 8 versions of the processed text. Each version represents
    ///   a partial or fully transformed state of the original text according to the rules specified
    ///   in the `SimpleMatchType`.
    ///
    /// # Processing Logic
    ///
    /// * For each bit in the `SimpleMatchType`, fetch the corresponding replacement list
    ///   and matcher.
    /// * Check if the current text bytes match any patterns in the matcher.
    /// * Perform the specified transformation if a match is found:
    ///   * For `None`, do nothing.
    ///   * For `Fanjian`, replace all occurrences of patterns.
    ///   * For `TextDelete` and `WordDelete`, delete matched patterns and concatenate
    ///     the remaining text.
    ///   * For other types, replace matched patterns with their corresponding replacements.
    fn reduce_text_process<'a>(
        &self,
        simple_match_type: &SimpleMatchType,
        text_bytes: &'a [u8],
    ) -> ArrayVec<[Cow<'a, [u8]>; 8]> {
        // Initialize an ArrayVec to store processed text byte arrays, starting with the original text bytes.
        let mut processed_text_bytes_list: ArrayVec<[Cow<'a, [u8]>; 8]> = ArrayVec::new();
        processed_text_bytes_list.push(Cow::Borrowed(text_bytes));

        // Iterate over each bit in the SimpleMatchType.
        for simple_match_type_bit in simple_match_type.iter() {
            // Fetch the corresponding replacement list and matcher for the current SimpleMatchType bit.
            let (process_replace_list, process_matcher) = unsafe {
                self.simple_match_type_process_map
                    .get(&simple_match_type_bit)
                    .unwrap_unchecked()
            };
            // Get the last processed text bytes from the list.
            let tmp_processed_text_bytes =
                unsafe { processed_text_bytes_list.last_mut().unwrap_unchecked() };

            // Match against the specific SimpleMatchType bit and perform appropriate transformations.
            match simple_match_type_bit {
                // None type requires no processing.
                SimpleMatchType::None => {}
                // Fanjian type performs replacement for all pattern matches.
                SimpleMatchType::Fanjian => {
                    // If a match is found, replace all occurrences of patterns in the text.
                    if unlikely(process_matcher.is_match(tmp_processed_text_bytes.as_ref())) {
                        *tmp_processed_text_bytes = Cow::Owned(
                            process_matcher.replace_all_bytes(text_bytes, process_replace_list),
                        );
                    }
                }
                // TextDelete and WordDelete types perform deletion of matched patterns.
                SimpleMatchType::TextDelete | SimpleMatchType::WordDelete => {
                    // If a match is likely, proceed with the deletion process.
                    if likely(process_matcher.is_match(tmp_processed_text_bytes.as_ref())) {
                        // Create a vector to store the bytes of the processed text.
                        let mut processed_text_bytes =
                            Vec::with_capacity(tmp_processed_text_bytes.len());
                        let mut last_match = 0;

                        // Iterate over all matches and build the processed text by excluding matched patterns.
                        for mat in process_matcher.find_iter(tmp_processed_text_bytes.as_ref()) {
                            processed_text_bytes.extend(unsafe {
                                tmp_processed_text_bytes.get_unchecked(last_match..mat.start())
                            });
                            last_match = mat.end();
                        }
                        // Add the remaining part of the text after the last match.
                        processed_text_bytes.extend(unsafe {
                            tmp_processed_text_bytes.get_unchecked(last_match..)
                        });

                        // Add the processed text to the list.
                        processed_text_bytes_list.push(Cow::Owned(processed_text_bytes));
                    }
                }
                // For other types, replace occurrences of patterns with corresponding replacements.
                _ => {
                    // If a match is found, replace occurrences of patterns and add the processed text to the list.
                    if process_matcher.is_match(tmp_processed_text_bytes.as_ref()) {
                        let processed_text_bytes = process_matcher
                            .replace_all_bytes(tmp_processed_text_bytes, process_replace_list);
                        processed_text_bytes_list.push(Cow::Owned(processed_text_bytes));
                    }
                }
            }
        }

        // Return the list of processed text byte arrays.
        processed_text_bytes_list
    }
}

impl<'a> TextMatcherTrait<'a, SimpleResult<'a>> for SimpleMatcher {
    /// Determines if there is a match for the input text.
    ///
    /// This method processes the given text through various transformation rules defined
    /// by the `SimpleMatchType`. It uses the Aho-Corasick algorithm to search for overlapping
    /// patterns within the processed text. If any sequence of transformations leads to a
    /// complete match (as indicated by the `split_bit_vec`), the method returns `true`.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be matched.
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if a match is found; otherwise, returns `false`.
    ///
    /// This method follows these steps:
    ///
    /// 1. Convert the input text to a byte slice.
    /// 2. If the number of characters in the byte slice is fewer than the minimum required character count (`min_chars_count`), return `false`.
    /// 3. Initialize a map (`word_id_split_bit_map`) to keep track of the split bit vectors for each word ID.
    /// 4. For each `SimpleMatchType` and its associated `SimpleAcTable` in the `simple_match_type_ac_table_map`:
    ///     a. Process the text according to the transformation rules defined by the `SimpleMatchType`.
    ///     b. Iterate over each processed version of the text.
    ///     c. For each processed text, find overlapping patterns using the Aho-Corasick matcher.
    ///     d. For each pattern found, update the split bit vector corresponding to the word ID.
    ///     e. If any word ID has its split bit vector reduced to all zeros, return `true`.
    /// 5. If no match is found after processing all transformations and patterns, return `false`.
    fn is_match(&self, text: &str) -> bool {
        // Convert the input text to a byte slice.
        let text_bytes = text.as_bytes();

        // Check if the number of characters in the byte slice is fewer than the minimum required character count.
        if unlikely(bytecount::num_chars(text_bytes) < self.min_chars_count) {
            return false; // Return false if the character count is too low.
        }

        // Initialize a map to keep track of the split bit vectors for each word ID.
        let mut word_id_split_bit_map = IntMap::default();

        // Iterate over each SimpleMatchType and its associated SimpleAcTable.
        for (simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            // Process the text according to the transformation rules defined by SimpleMatchType.
            let processed_text_bytes_list = self.reduce_text_process(simple_match_type, text_bytes);
            let processed_times = processed_text_bytes_list.len();

            // Iterate over each processed version of the text.
            for (index, processed_text) in processed_text_bytes_list.iter().enumerate() {
                // Use the Aho-Corasick matcher to find overlapping patterns in the processed text.
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

                    // Get or initialize the split bit vector corresponding to the word ID.
                    let split_bit_vec = word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                        iter::repeat_n(word_conf.split_bit, processed_times)
                            .collect::<ArrayVec<[_; 8]>>()
                    });

                    // Update the split bit vector by shifting the bit to the right.
                    *unsafe {
                        split_bit_vec
                            .get_unchecked_mut(index)
                            .as_mut_array()
                            .get_unchecked_mut(ac_word_conf.1)
                    } >>= 1;

                    // Check if all shifts have reduced the split bit vector to all zeros.
                    if unlikely(
                        split_bit_vec
                            .iter()
                            .fold(Simd::splat(1), |acc, &bit| acc & bit)
                            == ZEROS,
                    ) {
                        return true; // Return true if a complete match is found.
                    }
                }
            }
        }

        // Return false if no match is found after processing all transformations and patterns.
        false
    }

    /// Processes the input text and returns a list of `SimpleResult` instances representing matched words.
    ///
    /// This method processes the input text according to various transformation rules defined by the
    /// `SimpleMatchType`. It uses the Aho-Corasick algorithm to search for overlapping patterns within the
    /// processed text. The matched words are then collected and returned as a vector of `SimpleResult` instances.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    ///
    /// * `Vec<SimpleResult<'a>>` - A vector of `SimpleResult` instances representing matched words, each containing
    ///   the word's unique identifier and the matched word itself.
    ///
    /// # Processing Logic
    ///
    /// * Convert the input text to a byte slice.
    /// * If the number of characters in the byte slice is fewer than the minimum required character count, return an empty result list.
    /// * Initialize a set (`word_id_set`) to keep track of word IDs that have been matched, and a map (`word_id_split_bit_map`)
    ///   to track split bit vectors for each word ID.
    /// * For each `SimpleMatchType` and its associated `SimpleAcTable` in the `simple_match_type_ac_table_map`:
    ///     a. Process the text according to the transformation rules defined by the `SimpleMatchType`.
    ///     b. Iterate over each processed version of the text.
    ///     c. For each processed text, find overlapping patterns using the Aho-Corasick matcher.
    ///     d. For each pattern found, update the split bit vector corresponding to the word ID.
    ///     e. If any word ID has its split bit vector reduced to all zeros, add the word to the result list.
    ///
    /// This method ensures that each matched word is processed efficiently using SIMD vectors and Aho-Corasick automata.
    fn process(&'a self, text: &str) -> Vec<SimpleResult<'a>> {
        let text_bytes = text.as_bytes(); // Convert the input text to a byte slice
        let mut result_list = Vec::new(); // Initialize an empty vector to store the results

        // Check if the number of characters in the byte slice is fewer than the minimum required character count
        if unlikely(bytecount::num_chars(text_bytes) < self.min_chars_count) {
            return result_list; // Return an empty result list if the character count is too low
        }

        let mut word_id_set = IntSet::default(); // Initialize a set to keep track of matched word IDs
        let mut word_id_split_bit_map = IntMap::default(); // Initialize a map to track split bit vectors for each word ID

        // Iterate over each SimpleMatchType and its associated SimpleAcTable
        for (simple_match_type, simple_ac_table) in &self.simple_match_type_ac_table_map {
            // Process the text according to the transformation rules defined by SimpleMatchType
            let processed_text_bytes_list = self.reduce_text_process(simple_match_type, text_bytes);
            let processed_times = processed_text_bytes_list.len(); // Get the number of processed versions of the text

            // Iterate over each processed version of the text
            for (index, processed_text) in processed_text_bytes_list.iter().enumerate() {
                // Use the Aho-Corasick matcher to find overlapping patterns in the processed text
                for ac_result in simple_ac_table
                    .ac_matcher
                    .find_overlapping_iter(processed_text)
                {
                    // Retrieve the word configuration based on the pattern found
                    let ac_word_conf = unsafe {
                        simple_ac_table
                            .ac_word_conf_list
                            .get_unchecked(ac_result.pattern().as_usize())
                    };
                    let word_id = ac_word_conf.0; // Extract the word ID from the word configuration

                    // If the word ID is already in the set, skip further processing
                    if word_id_set.contains(&word_id) {
                        continue;
                    }

                    // Get the word configuration from the simple_wordconf_map
                    let word_conf =
                        unsafe { self.simple_wordconf_map.get(&word_id).unwrap_unchecked() };

                    // Get or initialize the split bit vector corresponding to the word ID
                    let split_bit_vec = word_id_split_bit_map.entry(word_id).or_insert_with(|| {
                        iter::repeat_n(word_conf.split_bit, processed_times)
                            .collect::<ArrayVec<[_; 8]>>()
                    });

                    // Update the split bit vector by shifting the bit to the right
                    *unsafe {
                        split_bit_vec
                            .get_unchecked_mut(index)
                            .as_mut_array()
                            .get_unchecked_mut(ac_word_conf.1)
                    } >>= 1;

                    // Check if all shifts have reduced the split bit vector to all zeros
                    if unlikely(
                        split_bit_vec
                            .iter()
                            .fold(Simd::splat(1), |acc, &bit| acc & bit)
                            == ZEROS,
                    ) {
                        word_id_set.insert(word_id); // Add the word ID to the set of matched word IDs
                                                     // Add the matched word to the result list
                        result_list.push(SimpleResult {
                            word_id,
                            word: Cow::Borrowed(&word_conf.word),
                        });
                    }
                }
            }
        }

        result_list // Return the list of matched words
    }
}
