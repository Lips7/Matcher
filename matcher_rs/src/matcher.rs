use std::borrow::Cow;
use std::collections::HashMap;

use nohash_hasher::{IntMap, IntSet};
use sonic_rs::{to_string, Deserialize, Serialize};

use crate::regex_matcher::{RegexMatcher, RegexTable};
use crate::sim_matcher::{SimMatcher, SimTable};
use crate::simple_matcher::{SimpleMatchType, SimpleMatcher};

pub trait TextMatcherTrait<'a, T: MatchResultTrait<'a>> {
    fn is_match(&self, text: &str) -> bool;
    fn process(&'a self, text: &str) -> Vec<T>;
    fn batch_process(&'a self, text_array: &[&str]) -> Vec<Vec<T>> {
        text_array.iter().map(|&text| self.process(text)).collect()
    }
}

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
/// Enum defining different types of matching tables used for text processing.
///
/// This enum is used to specify the type of matching table when performing text matching operations.
/// Each variant represents a distinct matching strategy, enabling the selection of the most appropriate
/// method based on the required use case. The enum variants support Serde serialization and deserialization,
/// making them easy to work with in contexts where data persistence or configuration might be necessary.
///
/// # Variants
///
/// * `Simple` - Represents a basic word matching strategy.
/// * `SimilarChar` - Represents a matching strategy based on similar characters.
/// * `Acrostic` - Represents a matching strategy based on acrostic patterns.
/// * `SimilarTextLevenshtein` - Represents a matching strategy using Levenshtein distance to find similar texts.
/// * `Regex` - Represents a matching strategy using regular expressions.
///
/// # Serde Attributes
///
/// * `rename_all = "snake_case"` - Ensures that the serialized/deserialized variant names are in snake_case format.
///
/// # Example
///
/// ```
/// use matcher_rs::MatchTableType;
///
/// let match_type = MatchTableType::Simple;
/// ```
pub enum MatchTableType {
    Simple,
    SimilarChar,
    Acrostic,
    SimilarTextLevenshtein,
    Regex,
}

#[derive(Serialize, Deserialize, Clone)]
/// A structure representing a table configuration for matching words in text processing.
///
/// This structure defines the configuration for a specific matching table used in text
/// matching operations. It includes details about the table's identifier, the type of
/// matching strategy, and lists of words and exemption words associated with simple
/// match types. The use of lifetimes ensures that word lists can borrow data, optimizing
/// memory usage.
///
/// # Fields
///
/// * `table_id` - A unique identifier for the matching table.
/// * `match_table_type` - The type of matching strategy used in this table (e.g., simple, regex, similarity).
/// * `simple_match_type` - The simple word matching strategy used in this table.
/// * `word_list` - A list of words configured for matching, borrowed for efficiency.
/// * `exemption_simple_match_type` - The matching strategy for exemption words in this table.
/// * `exemption_word_list` - A list of exemption words, borrowed for efficiency.
///
/// # Serde Attributes
///
/// * `borrow` - Ensures that the deserialized `word_list` and `exemption_word_list` fields
///   can borrow data from the input, rather than owning it.
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

#[derive(Debug, Clone)]
/// A structure representing the configuration of a word table used in text matching.
///
/// This structure holds the details of a specific word table and its configuration within
/// the text matching system. It includes a unique identifier for the match, the table's
/// identifier, and a flag indicating whether the word table represents an exemption.
///
/// # Fields
///
/// * `match_id` - A `u64` representing the identifier of the match within the system.
/// * `table_id` - A `u64` representing the identifier of the table within the system.
/// * `is_exemption` - A `bool` flag that indicates whether the word table is an exemption.
struct WordTableConf {
    match_id: u64,
    table_id: u64,
    is_exemption: bool,
}

#[derive(Serialize)]
/// A structure representing the result of a matching operation.
///
/// This structure contains details about an individual matching result,
/// including the identifier of the matching table and the matched word itself.
///
/// # Fields
///
/// * `table_id` - A `u64` that uniquely identifies the table in which the match was found.
/// * `word` - A `Cow<'a, str>` that holds the matched word. The `Cow` type allows the word
///    to be either borrowed from the original data or owned, optimizing for efficient memory use.
///
/// # Lifetimes
///
/// * `'a` - The lifetime associated with the `word` field, ensuring that the data
///    for the word can be borrowed for efficiency.
pub struct MatchResult<'a> {
    pub table_id: u64,
    pub word: Cow<'a, str>,
}

impl MatchResultTrait<'_> for MatchResult<'_> {
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

pub type MatchTableMap<'a> = IntMap<u64, Vec<MatchTable<'a>>>;

#[derive(Debug, Clone)]
/// A structure representing a text matcher that can utilize various matching strategies.
///
/// The `Matcher` structure is responsible for managing and coordinating the use of different
/// matching strategies (simple, regex, and similarity) to process and match text. It maintains
/// internal data structures to efficiently store and retrieve matching configurations and results.
///
/// # Fields
///
/// * `simple_word_table_conf_map` - A mapping from `u64` word IDs to `WordTableConf` structures,
///   which hold information about the configuration of word tables used in simple matching.
/// * `simple_word_table_conf_id_map` - A mapping from `u64` word IDs to `u64` table configuration IDs,
///   allowing for quick lookup of the associated `WordTableConf` structure.
/// * `simple_matcher` - An optional `SimpleMatcher` instance, which performs simple word matching.
/// * `regex_matcher` - An optional `RegexMatcher` instance, which performs regular expression matching.
/// * `sim_matcher` - An optional `SimMatcher` instance, which performs similarity-based matching.
///
/// # Behavior
///
/// The `Matcher` structure provides methods for initializing a new `Matcher` instance, performing
/// raw text matching, and matching text while returning results as a `HashMap` or a serialized JSON string.
/// It also implements the `TextMatcherTrait` for the `MatchResult` type, allowing for easy text
/// matching operations.
pub struct Matcher {
    simple_word_table_conf_map: IntMap<u64, WordTableConf>,
    simple_word_table_conf_id_map: IntMap<u64, u64>,
    simple_matcher: Option<SimpleMatcher>,
    regex_matcher: Option<RegexMatcher>,
    sim_matcher: Option<SimMatcher>,
}

impl Matcher {
    /// Creates a new `Matcher` instance from the provided match table map.
    ///
    /// This function initializes a new `Matcher` instance by processing the input `match_table_map`.
    /// The `match_table_map` is an iterator that yields tuples, where the first element is a match identifier
    /// and the second element is an iterator over `MatchTable` instances.
    ///
    /// The function iterates over the `match_table_map` and constructs the necessary data structures for
    /// the `Matcher` instance. It handles different matching strategies (simple, regex, and similarity)
    /// and populates the corresponding data structures based on the match table configurations.
    ///
    /// # Arguments
    ///
    /// * `match_table_map` - An iterator that yields tuples, where the first element is a match identifier
    ///   and the second element is an iterator over `MatchTable` instances.
    ///
    /// # Returns
    ///
    /// A new `Matcher` instance that is configured based on the input `match_table_map`.
    ///
    /// # Type Parameters
    ///
    /// * `I` - The type of the input `match_table_map` iterator.
    /// * `M` - The type of the iterator over `MatchTable` instances.
    ///
    /// # Lifetimes
    ///
    /// * `'a` - The lifetime associated with the borrowed string slices in the `MatchTable` instances.
    ///
    /// # Example
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use matcher_rs::{Matcher, MatchTable, MatchTableType, SimpleMatchType};
    ///
    /// let mut match_table_map = HashMap::new();
    /// match_table_map.insert(
    ///     1,
    ///     vec![MatchTable {
    ///         table_id: 1,
    ///         match_table_type: MatchTableType::Simple,
    ///         simple_match_type: SimpleMatchType::None,
    ///         word_list: vec!["apple", "banana"],
    ///         exemption_simple_match_type: SimpleMatchType::None,
    ///         exemption_word_list: vec!["orange"],
    ///     }],
    /// );
    ///
    /// let matcher = Matcher::new(match_table_map);
    /// ```
    pub fn new<'a, I, M>(match_table_map: I) -> Matcher
    where
        I: IntoIterator<Item = (u64, M)>,
        M: IntoIterator<Item = MatchTable<'a>>,
    {
        let mut word_id: u64 = 0;
        let mut word_table_conf_id: u64 = 0;

        let mut simple_word_table_conf_map = IntMap::default();
        let mut simple_word_table_conf_id_map = IntMap::default();

        let mut simple_match_type_word_map: IntMap<SimpleMatchType, IntMap<u64, &'a str>> =
            IntMap::default();

        let mut regex_table_list: Vec<RegexTable> = Vec::new();
        let mut sim_table_list: Vec<SimTable> = Vec::new();

        for (match_id, table_list) in match_table_map.into_iter() {
            for table in table_list.into_iter() {
                let table_id = table.table_id;
                let match_table_type = table.match_table_type;
                let word_list = table.word_list;
                let exemption_word_list = &table.exemption_word_list;

                if !word_list.is_empty() {
                    match match_table_type {
                        MatchTableType::Simple => {
                            simple_word_table_conf_map.insert(
                                word_table_conf_id,
                                WordTableConf {
                                    match_id,
                                    table_id,
                                    is_exemption: false,
                                },
                            );

                            let simple_word_map = simple_match_type_word_map
                                .entry(table.simple_match_type)
                                .or_default();

                            for word in word_list.iter() {
                                simple_word_table_conf_id_map.insert(word_id, word_table_conf_id);
                                simple_word_map.insert(word_id, word);
                                word_id += 1;
                            }

                            word_table_conf_id += 1
                        }
                        MatchTableType::SimilarTextLevenshtein => sim_table_list.push(SimTable {
                            table_id,
                            match_id,
                            word_list,
                        }),
                        _ => regex_table_list.push(RegexTable {
                            table_id,
                            match_id,
                            match_table_type,
                            word_list,
                        }),
                    }
                }

                if !exemption_word_list.is_empty() {
                    simple_word_table_conf_map.insert(
                        word_table_conf_id,
                        WordTableConf {
                            match_id,
                            table_id,
                            is_exemption: true,
                        },
                    );

                    let simple_word_map = simple_match_type_word_map
                        .entry(table.exemption_simple_match_type)
                        .or_default();

                    for exemption_word in exemption_word_list.iter() {
                        simple_word_table_conf_id_map.insert(word_id, word_table_conf_id);
                        simple_word_map.insert(word_id, exemption_word);
                        word_id += 1;
                    }

                    word_table_conf_id += 1
                }
            }
        }

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

    /// Matches the provided text against the available matchers and returns the raw matching results as a `GxHashMap`.
    ///
    /// This function takes a string slice representing the text to be matched and processes it using the available
    /// matchers (simple, regex, and similarity matchers). It gathers the matching results and organizes them
    /// by their respective match identifiers. The results for each match identifier are stored in a `ResultDict`
    /// structure, which contains a list of `MatchResult` instances and an exemption flag.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be matched.
    ///
    /// # Returns
    ///
    /// A `GxHashMap` where the keys are match identifiers (`&str`) and the values are `ResultDict` structures
    /// containing the matching results (`result_list`) and the exemption flag (`exemption_flag`).
    ///
    /// # Behavior
    ///
    /// - If the input text is empty, the function returns an empty `GxHashMap`.
    /// - The function iterates over the available matchers (simple, regex, and similarity) and processes the text
    ///   using each matcher.
    /// - For each matcher, the function collects the matching results and updates the `match_result_dict` accordingly.
    /// - If a simple matcher is used, the function retrieves the corresponding `WordTableConf` structure to determine
    ///   the table ID and the exemption flag.
    /// - The function filters out the results that correspond to an exemption and returns the remaining results
    ///   as a `GxHashMap`.
    ///
    /// # Safety
    ///
    /// The function uses `unsafe` blocks to call `unwrap_unchecked` on the `get` methods of the `IntMap` and
    /// `GxHashMap` data structures. This is done for performance optimization, assuming that the keys used for
    /// lookup exist in the data structures. It is important to ensure that this assumption holds true to avoid
    /// undefined behavior.
    pub fn word_match_raw(&self, text: &str) -> HashMap<u64, Vec<MatchResult>> {
        if !text.is_empty() {
            let mut match_result_dict = HashMap::default();
            let mut failed_match_id_set = IntSet::default();

            if let Some(regex_matcher) = &self.regex_matcher {
                for regex_result in regex_matcher.process(text) {
                    let result_list = match_result_dict
                        .entry(regex_result.match_id)
                        .or_insert(Vec::new());

                    result_list.push(MatchResult {
                        table_id: regex_result.table_id,
                        word: regex_result.word,
                    })
                }
            }

            if let Some(sim_matcher) = &self.sim_matcher {
                for sim_result in sim_matcher.process(text) {
                    let result_list = match_result_dict
                        .entry(sim_result.match_id)
                        .or_insert(Vec::new());

                    result_list.push(MatchResult {
                        table_id: sim_result.table_id,
                        word: sim_result.word,
                    })
                }
            }

            if let Some(simple_matcher) = &self.simple_matcher {
                for simple_result in simple_matcher.process(text) {
                    let word_table_conf = unsafe {
                        self.simple_word_table_conf_map
                            .get(
                                self.simple_word_table_conf_id_map
                                    .get(&simple_result.word_id)
                                    .unwrap_unchecked(),
                            )
                            .unwrap_unchecked()
                    };

                    if word_table_conf.is_exemption {
                        failed_match_id_set.insert(word_table_conf.match_id);
                        match_result_dict.remove(&word_table_conf.match_id);
                    }

                    if failed_match_id_set.contains(&word_table_conf.match_id) {
                        continue;
                    }

                    let result_list = match_result_dict
                        .entry(word_table_conf.match_id)
                        .or_insert(Vec::new());

                    result_list.push(MatchResult {
                        table_id: word_table_conf.table_id,
                        word: simple_result.word,
                    })
                }
            }

            match_result_dict
        } else {
            HashMap::default()
        }
    }

    /// Matches the provided text and returns the results as a `HashMap` with match identifiers and serialized JSON strings.
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
    /// A `HashMap` where the keys are match identifiers (`u64`) and the values are `String` instances
    /// containing the serialized JSON representation of the matching results for each identifier.
    ///
    /// # Safety
    ///
    /// The function uses an `unsafe` block to call `unwrap_unchecked` on the `to_string` function, which skips
    /// the error checking for performance optimization. It is important to ensure that the serialization process
    /// does not fail, as `unwrap_unchecked` will cause undefined behavior if an error occurs.
    pub fn word_match(&self, text: &str) -> HashMap<u64, String> {
        self.word_match_raw(text)
            .into_iter()
            .map(|(match_id, result_list)| {
                (match_id, unsafe {
                    to_string(&result_list).unwrap_unchecked()
                })
            })
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
    /// A `String` containing the serialized JSON representation of the matching results for all match identifiers.
    ///
    /// # Safety
    ///
    /// The function uses an `unsafe` block to call `unwrap_unchecked` on the `to_string` function, which skips
    /// the error checking for performance optimization. It is important to ensure that the serialization process
    /// does not fail, as `unwrap_unchecked` will cause undefined behavior if an error occurs.
    pub fn word_match_as_string(&self, text: &str) -> String {
        unsafe { to_string(&self.word_match(text)).unwrap_unchecked() }
    }
}

impl<'a> TextMatcherTrait<'a, MatchResult<'a>> for Matcher {
    fn is_match(&self, text: &str) -> bool {
        !self.word_match_raw(text).is_empty()
    }

    /// Processes the provided text and returns a vector of `MatchResult` instances.
    ///
    /// This function takes a string slice representing the text to be processed and matches it using the available
    /// matchers (simple, regex, and similarity matchers). It gathers the matching results and organizes them
    /// by their respective match identifiers. The results for each match identifier are then flattened into a single
    /// vector of `MatchResult` instances.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be processed.
    ///
    /// # Returns
    ///
    /// A `Vec` of `MatchResult` instances containing the matching results for all match identifiers.
    fn process(&'a self, text: &str) -> Vec<MatchResult<'a>> {
        self.word_match_raw(text)
            .into_iter()
            .flat_map(|(_, result_list)| result_list) // Flatten the result lists from all match IDs into a single iterator.
            .collect()
    }
}
