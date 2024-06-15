use std::borrow::Cow;
use std::collections::HashMap;

use ahash::AHashMap;
use nohash_hasher::IntMap;
use sonic_rs::{to_string, Deserialize, Serialize};

use crate::regex_matcher::{RegexMatcher, RegexTable};
use crate::sim_matcher::{SimMatcher, SimTable};
use crate::simple_matcher::{SimpleMatchType, SimpleMatcher};

/// A trait that defines the behavior of a text matcher.
///
/// This trait provides methods for checking if a given text matches certain criteria, processing
/// a single text to obtain matching results, and processing multiple texts in batch.
///
/// # Type Parameters
///
/// - `'a`: The lifetime associated with the matcher and its results.
/// - `T`: A type that implements the `MatchResultTrait<'a>` which defines the properties of a
///   match result.
///
/// # Provided Methods
///
/// - `is_match(&self, text: &str) -> bool`: Checks if the given text matches the criteria
///   defined by the matcher. Returns `true` if there is a match, otherwise returns `false`.
/// - `process(&'a self, text: &str) -> Vec<T>`: Processes the given text and returns a vector of
///   match results of type `T`.
/// - `batch_process(&'a self, text_array: &[&str]) -> Vec<Vec<T>>`: Processes an array of texts
///   in batch and returns a vector where each element is a vector of match results for the
///   corresponding text.
///
/// # Example
///
/// ```
/// use matcher_rs::{TextMatcherTrait, MatchResultTrait};
///
/// struct DummyMatcher;
/// impl TextMatcherTrait<'_, DummyMatchResult> for DummyMatcher {
///     fn is_match(&self, _text: &str) -> bool { true }
///     fn process(&self, _text: &str) -> Vec<DummyMatchResult> { vec![DummyMatchResult] }
/// }
///
/// struct DummyMatchResult;
/// impl<'a> MatchResultTrait<'a> for DummyMatchResult {
///     fn word_id(&self) -> u64 { 1 }
///     fn table_id(&self) -> u64 { 1 }
///     fn word(&self) -> &str { "dummy" }
/// }
///
/// let matcher = DummyMatcher;
/// let results = matcher.process("text");
/// ```
pub trait TextMatcherTrait<'a, T: MatchResultTrait<'a>> {
    fn is_match(&self, text: &str) -> bool;
    fn process(&'a self, text: &str) -> Vec<T>;
    fn batch_process(&'a self, text_array: &[&str]) -> Vec<Vec<T>> {
        text_array.iter().map(|&text| self.process(text)).collect()
    }
}

/// A trait that defines the properties of a match result.
///
/// This trait provides methods to retrieve specific details about a match result. Implementing
/// types will define how these methods extract and return relevant data such as the identifier of
/// the matched word, the identifier of the match table, and the matched word itself.
///
/// # Required Methods
///
/// - `word_id(&self) -> u64`: Returns the identifier of the matched word. By default, it returns `0`.
/// - `word(&self) -> &str`: Returns a reference to the matched word as a string slice.
///
/// # Example
///
/// ```
/// use matcher_rs::MatchResultTrait;
///
/// struct ExampleMatchResult {
///     word_id: u64,
///     table_id: u64,
///     word: String,
/// }
///
/// impl<'a> MatchResultTrait<'a> for ExampleMatchResult {
///     fn word_id(&self) -> u64 {
///         self.word_id
///     }
///
///     fn table_id(&self) -> u64 {
///         self.table_id
///     }
///
///     fn word(&self) -> &str {
///         &self.word
///     }
/// }
///
/// let match_result = ExampleMatchResult { word_id: 1, table_id: 2, word: "example".to_string() };
/// assert_eq!(match_result.word_id(), 1);
/// assert_eq!(match_result.table_id(), 2);
/// assert_eq!(match_result.word(), "example");
/// ```
pub trait MatchResultTrait<'a> {
    fn word_id(&self) -> u64 {
        0
    }
    fn table_id(&self) -> u64 {
        0
    }
    fn word(&self) -> &str;
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
/// Enum representing different types of matching tables.
///
/// Each variant of the enum corresponds to a specific type of text matching strategy that can be
/// employed. This enum can be serialized and deserialized, and the variants' names will be
/// automatically converted to snake_case during this process.
///
/// # Variants
///
/// - `Simple`: Represents a basic simple matching strategy.
/// - `SimilarChar`: Represents a matching strategy based on character similarity.
/// - `Acrostic`: Represents a matching strategy for acrostic text.
/// - `SimilarTextLevenshtein`: Represents a matching strategy based on the Levenshtein distance for text similarity.
/// - `Regex`: Represents a matching strategy using regular expressions.
///
/// # Example
///
/// ```
/// use matcher_rs::MatchTableType;
/// use sonic_rs;
///
/// let match_type = MatchTableType::Regex;
/// let serialized = sonic_rs::to_string(&match_type).unwrap();
/// assert_eq!(serialized, "\"regex\"");
///
/// let deserialized: MatchTableType = sonic_rs::from_str(&serialized).unwrap();
/// assert_eq!(deserialized, MatchTableType::Regex);
/// ```
pub enum MatchTableType {
    Simple,
    SimilarChar,
    Acrostic,
    SimilarTextLevenshtein,
    Regex,
}

#[derive(Serialize, Deserialize, Clone)]
/// A structure representing a matching table for text processing.
///
/// This structure holds the information describing a matching table, which includes various
/// types and lists of words for matching purposes. It supports serialization and
/// deserialization using the `serde` crate.
///
/// # Fields
///
/// * `table_id` - An identifier for the matching table.
/// * `match_table_type` - The type of the matching table, represented by the `MatchTableType` enum.
/// * `simple_match_type` - The match type used when the `match_table_type` is `Simple`. It is represented by the `SimpleMatchType` enum.
/// * `word_list` - A vector of word references (`&'a str`) that belong to the table. This list is borrowed.
/// * `exemption_simple_match_type` - The match type used for exemption words, also represented by the `SimpleMatchType` enum.
/// * `exemption_word_list` - A vector of exempted word references (`&'a str`). This list is borrowed.
///
/// # Specialization
/// When `,` is in a word and match_table_type is simple, or word is in exemption_word_list
/// this word then become a combined word, which means it will be split into multiple words, and the multiple words will be matched separately.
///
/// For example, 'a,b' will match 'aaaabbb', and 'a,a,b,b' will match 'aaaabbbb' but won't match 'abb', this is because the recurrence of a split word matters.
///
/// # Limitation
/// Currently, matcher can only handle words contains no more than 32 combined words and no more than 8 repeated word.
/// More than 32 combined word will be discarded, and more then 8 repeated word will be limited to 8.
///
/// # Example
///
/// ```
/// use matcher_rs::{MatchTable, MatchTableType, SimpleMatchType};
///
/// let table = MatchTable {
///     table_id: 1,
///     match_table_type: MatchTableType::Simple,
///     simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
///     word_list: vec!["example", "test"],
///     exemption_simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
///     exemption_word_list: vec!["exempt", "exclude"],
/// };
/// ```
pub struct MatchTable<'a> {
    pub table_id: u64,
    pub match_table_type: MatchTableType,
    pub simple_match_type: SimpleMatchType,
    #[serde(borrow)]
    pub word_list: Vec<&'a str>,
    pub exemption_simple_match_type: SimpleMatchType,
    #[serde(borrow)]
    pub exemption_word_list: Vec<&'a str>,
}

#[derive(Debug)]
/// Structure representing the configuration of a word table.
///
/// This struct holds information about a specific word table configuration which includes:
/// - The match identifier associated with the word table.
/// - The table identifier.
/// - A flag indicating whether this configuration is for an exemption.
///
/// # Fields
///
/// * `match_id` - A `String` that uniquely identifies the matching criteria associated with the word table.
/// * `table_id` - A `u64` that signifies the identifier of the table within the system.
/// * `is_exemption` - A `bool` flag indicating if the word table configuration is for exempted words (`true`) or not (`false`).
struct WordTableConf {
    match_id: String,
    table_id: u64,
    is_exemption: bool,
}

/// A structure representing a match result for text processing.
///
/// This structure is used to store the result of a text matching operation,
/// containing information about the matched word and the identifier of the table
/// from which the word originated. This helps in tracing which table's criteria
/// was met by the given word.
///
/// # Fields
///
/// * `table_id` - A `u64` that signifies the identifier of the table within the system.
/// * `word` - A `Cow<'a, str>` that holds the matched word. The use of `Cow` allows for
///   efficient representation of either borrowed or owned data, optimizing for performance.
#[derive(Serialize)]
pub struct MatchResult<'a> {
    table_id: u64,
    word: Cow<'a, str>,
}

impl MatchResultTrait<'_> for MatchResult<'_> {
    /// Implements the `MatchResultTrait` for the `MatchResult` struct.
    ///
    /// This implementation provides methods to retrieve specific details about a match result.
    /// It specifies how to extract and return relevant data such as the identifier of
    /// the matched word, the identifier of the match table, and the matched word itself.
    ///
    /// # Methods
    ///
    /// - `word_id(&self) -> u64`: This method returns the identifier of the matched word.
    ///   For this implementation, it always returns `0`.
    /// - `table_id(&self) -> u64`: This method returns the identifier of the table within the system.
    /// - `word(&self) -> &str`: This method returns a reference to the matched word as a string slice.
    fn word_id(&self) -> u64 {
        0
    }
    fn table_id(&self) -> u64 {
        self.table_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

/// A structure to hold results of text matching along with exemption status.
///
/// This structure is used to store results from different text matching operations
/// and a flag indicating if any of the matched results are exemptions. This helps
/// in filtering out exempted matches during processing. Specifically, it is used
/// in conjunction with the `Matcher` struct to organize and handle match results.
///
/// # Fields
///
/// * `result_list` - A vector of `MatchResult` holding the individual match results.
/// * `exemption_flag` - A boolean flag indicating if the results include any exemptions.
struct ResultDict<'a> {
    result_list: Vec<MatchResult<'a>>,
    exemption_flag: bool,
}

/// A type alias for a hash map where the keys are string slices and the values are vectors of `MatchTable` references.
///
/// This type alias simplifies the representation of the mapping between match identifiers and
/// their corresponding match tables. Each entry in the map associates a match identifier (a string slice)
/// with a list of `MatchTable` instances, which define the different criteria for matching texts.
///
/// # Type Parameters
///
/// - `'a`: The lifetime associated with the borrowed string slices and match tables.
///
/// # Example
///
/// ```
/// use matcher_rs::{MatchTableMap, MatchTable, MatchTableType, SimpleMatchType};
/// use ahash::AHashMap;
///
/// let mut match_table_map: MatchTableMap = AHashMap::default();
/// let example_table = MatchTable {
///     table_id: 1,
///     match_table_type: MatchTableType::Simple,
///     simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
///     word_list: vec!["example", "test"],
///     exemption_simple_match_type: SimpleMatchType::FanjianDeleteNormalize,
///     exemption_word_list: vec!["exempt", "exclude"],
/// };
///
/// match_table_map.insert("example_key", vec![example_table]);
/// ```
pub type MatchTableMap<'a> = AHashMap<&'a str, Vec<MatchTable<'a>>>;

/// A structure that represents a text matcher with multiple matching strategies.
///
/// The `Matcher` structure is designed to hold and process text using various matching strategies.
/// It supports simple word matching, regex-based matching, and similarity-based matching. This
/// structure organizes and manages the different matchers and the configuration associated with
/// each text matching strategy.
///
/// # Fields
///
/// * `simple_word_table_conf_map` - An `IntMap` that maps a unique identifier to a `WordTableConf`
///   structure. This configuration map is used specifically for simple word matching tables.
/// * `simple_word_table_conf_id_map` - An `IntMap` that maps word identifiers to word table
///   configuration identifiers. This helps in quickly retrieving the configuration associated
///   with a specific word.
/// * `simple_matcher` - An `Option<SimpleMatcher>` that holds a `SimpleMatcher` instance if
///   simple word matching rules are defined. Otherwise, it holds `None`.
/// * `regex_matcher` - An `Option<RegexMatcher>` that holds a `RegexMatcher` instance if
///   regex-based matching rules are defined. Otherwise, it holds `None`.
/// * `sim_matcher` - An `Option<SimMatcher>` that holds a `SimMatcher` instance if similarity-based
///   matching rules are defined (e.g., using Levenshtein distance). Otherwise, it holds `None`.
///
/// # Example
///
/// ```
/// use matcher_rs::{Matcher, MatchTableMap};
/// use ahash::AHashMap;
///
/// let match_table_map: MatchTableMap = AHashMap::default();
/// let matcher = Matcher::new(match_table_map);
/// ```
/// The example demonstrates how to initialize a `Matcher` instance using an empty `MatchTableMap`.
/// The `Matcher` will be configured based on the provided match table map.
pub struct Matcher {
    simple_word_table_conf_map: IntMap<u64, WordTableConf>,
    simple_word_table_conf_id_map: IntMap<u64, u64>,
    simple_matcher: Option<SimpleMatcher>,
    regex_matcher: Option<RegexMatcher>,
    sim_matcher: Option<SimMatcher>,
}

impl Matcher {
    /// Performs raw text matching using the available matchers and returns the results.
    ///
    /// This function processes the input text through the available simple, regex, and similarity
    /// matchers. For each matcher, it gathers the matching results and organizes them by their
    /// respective match identifiers. It also considers exemption flags to filter out exempted matches.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be matched.
    ///
    /// # Returns
    ///
    /// A `AHashMap` where the keys are match identifiers and the values are vectors of `MatchResult`
    /// containing the details of each matched result.
    ///
    /// # Behavior
    ///
    /// - If the input text is not empty, the function initializes an empty dictionary `match_result_dict`
    ///   to store results. Each potential match is processed and added to the dictionary.
    /// - The function checks each of the available matchers (`simple_matcher`, `regex_matcher`, `sim_matcher`)
    ///   to find matches in the text. These matchers are applied sequentially.
    /// - For `simple_matcher`, the associated configuration `WordTableConf` is found using unsafe code to
    ///   improve performance by avoiding bound checks. The results are stored and organized in `match_result_dict`.
    /// - For `regex_matcher` and `sim_matcher`, the results are directly added to `match_result_dict`.
    /// - If a match result corresponds to an exempted word (determined by `is_exemption` flag), an
    ///   `exemption_flag` is set for that match identifier.
    /// - After collecting all the results, the function filters out match identifiers that have exemption flags
    ///   set and returns the remaining results as a `AHashMap`.
    /// - If the input text is empty, the function returns an empty `AHashMap`.
    ///
    /// # Safety
    ///
    /// Uses unsafe code to access internal maps for performance optimization. The use of `unsafe` is justified
    pub fn new<'a, I, M>(match_table_map: I) -> Matcher
    where
        I: IntoIterator<Item = (&'a str, M)>,
        M: IntoIterator<Item = MatchTable<'a>>,
    {
        // Initialize word ID and word table configuration ID counters.
        let mut word_id: u64 = 0;
        let mut word_table_conf_id: u64 = 0;

        // Initialize maps to hold word table configurations and their IDs.
        let mut simple_word_table_conf_map = IntMap::default();
        let mut simple_word_table_conf_id_map = IntMap::default();

        // Initialize map to associate simple match types with word maps.
        let mut simple_match_type_word_map: IntMap<SimpleMatchType, IntMap<u64, &'a str>> =
            IntMap::default();

        // Initialize lists to accumulate regex and similarity tables.
        let mut regex_table_list: Vec<RegexTable> = Vec::new();
        let mut sim_table_list: Vec<SimTable> = Vec::new();

        // Iterate over the match table map to parse and organize the data.
        for (match_id, table_list) in match_table_map.into_iter() {
            for table in table_list.into_iter() {
                let table_id = table.table_id;
                let match_table_type = table.match_table_type;
                let word_list = table.word_list;
                let exemption_word_list = &table.exemption_word_list;

                // Process non-empty word lists based on their match table type.
                if !word_list.is_empty() {
                    match match_table_type {
                        // Handle simple match types.
                        MatchTableType::Simple => {
                            // Insert regular word table configuration.
                            simple_word_table_conf_map.insert(
                                word_table_conf_id,
                                WordTableConf {
                                    match_id: match_id.to_owned(),
                                    table_id,
                                    is_exemption: false,
                                },
                            );

                            // Get or create the simple word map for the specific match type.
                            let simple_word_map = simple_match_type_word_map
                                .entry(table.simple_match_type)
                                .or_default();

                            // Insert each word into the appropriate maps and increment word ID.
                            for word in word_list.iter() {
                                simple_word_table_conf_id_map.insert(word_id, word_table_conf_id);
                                simple_word_map.insert(word_id, word);
                                word_id += 1;
                            }

                            // Increment the word table configuration ID counter.
                            word_table_conf_id += 1
                        }
                        // Handle similarity match types.
                        MatchTableType::SimilarTextLevenshtein => sim_table_list.push(SimTable {
                            table_id,
                            match_id,
                            word_list,
                        }),
                        // Handle all other match types as regex tables.
                        _ => regex_table_list.push(RegexTable {
                            table_id,
                            match_id,
                            match_table_type,
                            word_list,
                        }),
                    }
                }

                // Process non-empty exemption word lists.
                if !exemption_word_list.is_empty() {
                    // Insert exemption word table configuration.
                    simple_word_table_conf_map.insert(
                        word_table_conf_id,
                        WordTableConf {
                            match_id: match_id.to_owned(),
                            table_id,
                            is_exemption: true,
                        },
                    );

                    // Get or create a simple word map for the exemption match type.
                    let simple_word_map = simple_match_type_word_map
                        .entry(table.exemption_simple_match_type)
                        .or_default();

                    // Insert each exemption word into the appropriate maps and increment word ID.
                    for exemption_word in exemption_word_list.iter() {
                        simple_word_table_conf_id_map.insert(word_id, word_table_conf_id);
                        simple_word_map.insert(word_id, exemption_word);
                        word_id += 1;
                    }

                    // Increment the word table configuration ID counter.
                    word_table_conf_id += 1
                }
            }
        }

        // Create and return the Matcher instance with initialized matchers based on the tables.
        Matcher {
            simple_word_table_conf_map,
            simple_word_table_conf_id_map,
            simple_matcher: (!simple_match_type_word_map.is_empty())
                .then(|| SimpleMatcher::new(simple_match_type_word_map)),
            regex_matcher: (!regex_table_list.is_empty())
                .then(|| RegexMatcher::new(&regex_table_list)),
            sim_matcher: (!sim_table_list.is_empty()).then(|| SimMatcher::new(&sim_table_list)),
        }
    }

    /// Performs raw text matching using the available matchers and returns the results.
    ///
    /// This function processes the input text through the available simple, regex, and similarity
    /// matchers. For each matcher, it gathers the matching results and organizes them by their
    /// respective match identifiers. It also considers exemption flags to filter out exempted matches.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be matched.
    ///
    /// # Returns
    ///
    /// A `AHashMap` where the keys are match identifiers and the values are vectors of `MatchResult`
    /// containing the details of each matched result.
    ///
    /// # Behavior
    ///
    /// - If the input text is not empty, the function initializes a `match_result_dict` to store results.
    /// - The function processes the text using each available matcher (`simple_matcher`, `regex_matcher`, `sim_matcher`).
    /// - For `simple_matcher`, it retrieves the associated `WordTableConf` and stores the results in `match_result_dict`.
    /// - For `regex_matcher` and `sim_matcher`, the results are directly added to `match_result_dict`.
    /// - Results that correspond to exempted words are flagged, and exempted results are filtered out before returning.
    /// - If the input text is empty, the function returns an empty `AHashMap`.
    ///
    /// # Safety
    ///
    /// Uses unsafe code to access internal maps for performance optimization. The use of `unsafe` is justified
    /// for better performance by avoiding bound checks.
    fn word_match_raw(&self, text: &str) -> AHashMap<&str, Vec<MatchResult>> {
        if !text.is_empty() {
            // Initialize a dictionary to hold match results keyed by match identifier.
            let mut match_result_dict: AHashMap<&str, ResultDict> = AHashMap::default();

            // Check if the simple matcher is available.
            if let Some(simple_matcher) = &self.simple_matcher {
                // Process the text through the simple matcher and iterate over the results.
                for simple_result in simple_matcher.process(text) {
                    // Unsafe block to retrieve the associated WordTableConf for the result.
                    let word_table_conf = unsafe {
                        self.simple_word_table_conf_map
                            .get(
                                self.simple_word_table_conf_id_map
                                    .get(&simple_result.word_id)
                                    .unwrap_unchecked(),
                            )
                            .unwrap_unchecked()
                    };

                    // Get or insert the ResultDict for the current match identifier.
                    let result_dict = match_result_dict
                        .entry(&word_table_conf.match_id)
                        .or_insert(ResultDict {
                            result_list: Vec::new(),
                            exemption_flag: false,
                        });

                    // Check if the result is an exemption and set the flag accordingly.
                    if word_table_conf.is_exemption {
                        result_dict.exemption_flag = true;
                    }

                    // Add the simple match result to the result_list.
                    result_dict.result_list.push(MatchResult {
                        table_id: word_table_conf.table_id,
                        word: simple_result.word,
                    });
                }
            }

            // Check if the regex matcher is available.
            if let Some(regex_matcher) = &self.regex_matcher {
                // Process the text through the regex matcher and iterate over the results.
                for regex_result in regex_matcher.process(text) {
                    // Get or insert the ResultDict for the current match identifier.
                    let result_dict =
                        match_result_dict
                            .entry(regex_result.match_id)
                            .or_insert(ResultDict {
                                result_list: Vec::new(),
                                exemption_flag: false,
                            });

                    // Add the regex match result to the result_list.
                    result_dict.result_list.push(MatchResult {
                        table_id: regex_result.table_id,
                        word: regex_result.word,
                    });
                }
            }

            // Check if the similarity matcher is available.
            if let Some(sim_matcher) = &self.sim_matcher {
                // Process the text through the similarity matcher and iterate over the results.
                for sim_result in sim_matcher.process(text) {
                    // Get or insert the ResultDict for the current match identifier.
                    let result_dict =
                        match_result_dict
                            .entry(sim_result.match_id)
                            .or_insert(ResultDict {
                                result_list: Vec::new(),
                                exemption_flag: false,
                            });

                    // Add the similarity match result to the result_list.
                    result_dict.result_list.push(MatchResult {
                        table_id: sim_result.table_id,
                        word: sim_result.word,
                    });
                }
            }

            // Filter out exempted match results and return the collected match results.
            match_result_dict
                .into_iter()
                .filter_map(|(match_id, result_dict)| {
                    (!result_dict.exemption_flag).then_some((match_id, result_dict.result_list))
                })
                .collect()
        } else {
            // Return an empty dictionary if the input text is empty.
            AHashMap::default()
        }
    }

    /// Matches the provided text against the available matchers and returns the results as a `HashMap`.
    ///
    /// This function takes a string slice representing the text to be matched and processes it using the available
    /// matchers (simple, regex, and similarity matchers). It gathers the matching results and organizes them
    /// by their respective match identifiers. The results for each match identifier are serialized into a `String`
    /// using the `to_string` function from the `sonic_rs` crate.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be matched.
    ///
    /// # Returns
    ///
    /// A `HashMap` where the keys are match identifiers (`&str`) and the values are serialized `String` representations
    /// of the matching results (`result_list`).
    ///
    /// # Safety
    ///
    /// The function uses `unsafe` blocks to call `unwrap_unchecked` on the `to_string` function, which skips the error
    /// checking for performance optimization. It is important to ensure that the serialization process does not fail,
    /// as `unwrap_unchecked` will cause undefined behavior if an error occurs.
    pub fn word_match(&self, text: &str) -> HashMap<&str, String> {
        // Call word_match_raw to get the raw matching results as a GxHashMap.
        self.word_match_raw(text)
            .into_iter()
            // Transform each entry (match_id, result_list) in the GxHashMap.
            .map(|(match_id, result_list)| {
                // Serialize the result list into a JSON string,
                // skipping error checking for performance optimization.
                // This uses unsafe code to call unwrap_unchecked.
                (match_id, unsafe {
                    to_string(&result_list).unwrap_unchecked()
                })
            })
            // Collect the transformed entries back into a HashMap.
            .collect()
    }

    /// Matches the provided text and returns the results as a serialized JSON string.
    ///
    /// This function takes a string slice representing the text to be matched and processes it using the available
    /// matchers (simple, regex, and similarity matchers). It gathers the matching results and organizes them
    /// by their respective match identifiers. The results for each match identifier are then serialized into
    /// a JSON string using the `to_string` function from the `sonic_rs` crate.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be matched.
    ///
    /// # Returns
    ///
    /// A `String` that contains the serialized JSON representation of the matching results. Each entry in the
    /// resulting JSON string corresponds to a match identifier and its associated list of match results.
    ///
    /// # Safety
    ///
    /// The function uses an `unsafe` block to call `unwrap_unchecked` on the `to_string` function, which skips
    /// the error checking for performance optimization. It is important to ensure that the serialization process
    /// does not fail, as `unwrap_unchecked` will cause undefined behavior if an error occurs.
    pub fn word_match_as_string(&self, text: &str) -> String {
        // Serialize the match results obtained from `word_match` to a JSON string.
        // Directly call `to_string` on the results without error checking.
        unsafe { to_string(&self.word_match(text)).unwrap_unchecked() }
    }
}

impl<'a> TextMatcherTrait<'a, MatchResult<'a>> for Matcher {
    /// Checks if a given text matches any criteria defined by the matchers.
    ///
    /// This function sequentially checks the text against available matchers (simple, regex,
    /// and similarity matchers) to determine if there is a match.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be matched.
    ///
    /// # Returns
    ///
    /// Returns `true` if any of the matchers find a match for the provided text. Otherwise, returns `false`.
    ///
    /// # Behavior
    ///
    /// - The function first checks the `simple_matcher`. If it finds a match, it returns `true`.
    /// - If the `simple_matcher` does not find a match, the function then checks the `regex_matcher`.
    /// - If the `regex_matcher` finds a match, it returns `true`.
    /// - If both the `simple_matcher` and `regex_matcher` do not find matches, it finally checks the `sim_matcher`.
    /// - If the `sim_matcher` finds a match, it returns `true`.
    /// - If none of the matchers find a match, the function returns `false`.
    fn is_match(&self, text: &str) -> bool {
        // Check if the simple_matcher is available and if it matches the text.
        if let Some(simple_matcher) = &self.simple_matcher {
            if simple_matcher.is_match(text) {
                // Return true if there is a match in the simple_matcher.
                return true;
            }
        }

        // Check if the regex_matcher is available and if it matches the text.
        if let Some(regex_matcher) = &self.regex_matcher {
            if regex_matcher.is_match(text) {
                // Return true if there is a match in the regex_matcher.
                return true;
            }
        }

        // Check if the sim_matcher is available and if it matches the text.
        if let Some(sim_matcher) = &self.sim_matcher {
            if sim_matcher.is_match(text) {
                // Return true if there is a match in the sim_matcher.
                return true;
            }
        }

        // Return false if none of the matchers find a match.
        false
    }

    /// Processes the given text and returns a vector of match results.
    ///
    /// This function performs raw text matching by invoking `word_match_raw` on the provided text
    /// and flattens the resulting `GxHashMap` into a single vector of `MatchResult`. Each element
    /// in the resulting vector represents an individual match result from any of the matchers
    /// (simple, regex, or similarity).
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be processed for matching.
    ///
    /// # Returns
    ///
    /// Returns a `Vec<MatchResult>` containing the details of each matched result found in the text.
    fn process(&'a self, text: &str) -> Vec<MatchResult<'a>> {
        // Call `word_match_raw` to get the raw matching results as a GxHashMap.
        self.word_match_raw(text)
            .into_iter() // Iterate over the entries in the GxHashMap.
            .flat_map(|(_, result_list)| result_list) // Flatten the result lists from all match IDs into a single iterator.
            .collect() // Collect the flattened results into a vector.
    }
}
