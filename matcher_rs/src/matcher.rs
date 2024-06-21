use std::borrow::Cow;
use std::collections::HashMap;

use nohash_hasher::{IntMap, IntSet};
use sonic_rs::{to_string, Deserialize, Serialize};

use crate::regex_matcher::{RegexMatchType, RegexMatcher, RegexTable};
use crate::sim_matcher::{SimMatchType, SimMatcher, SimTable};
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
    fn similarity(&self) -> f64 {
        1.0
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(untagged)]
/// An enumeration representing the different types of matching strategies available for a match table.
///
/// This enum defines the various strategies that can be applied when attempting to match text
/// within a table. Each variant encapsulates the specific configuration required for that type of matching.
///
/// # Variants
///
/// * `Simple { simple_match_type }` - Indicates the use of a simple matching strategy. Contains a `simple_match_type` field of type [SimpleMatchType].
/// * `Regex { regex_match_type }` - Indicates the use of a regular expression matching strategy. Contains a `regex_match_type` field of type [RegexMatchType].
/// * `Similar { sim_match_type, threshold }` - Indicates the use of a similarity-based matching strategy. Contains a `sim_match_type` field of type [SimMatchType] and a `threshold` field of type [f64].
///
/// # Serde Attributes
///
/// The `snake_case` renaming strategy is used for serialization and deserialization to ensure
/// that the field names in the serialized output conform to the snake_case convention.
///
/// # Example
///
/// ```
/// use matcher_rs::{MatchTableType, SimpleMatchType, RegexMatchType, SimMatchType};
///
/// let simple_match = MatchTableType::Simple {
///     simple_match_type: SimpleMatchType::None,
/// };
///
/// let regex_match = MatchTableType::Regex {
///     regex_match_type: RegexMatchType::Regex,
/// };
///
/// let similar_match = MatchTableType::Similar {
///     sim_match_type: SimMatchType::Levenshtein,
///     threshold: 0.8,
/// };
/// ```
pub enum MatchTableType {
    Simple {
        simple_match_type: SimpleMatchType,
    },
    Regex {
        regex_match_type: RegexMatchType,
    },
    Similar {
        sim_match_type: SimMatchType,
        threshold: f64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
/// A structure representing a match table configuration used for text matching operations.
///
/// This structure defines the necessary fields and types required for configuring a match
/// table. Each match table has an associated matching strategy, a list of words to be matched,
/// and a list of exemptions. The match table configuration is essential for initializing matchers
/// and performing text matching processes.
///
/// The structure supports serialization and deserialization through the `serde` library,
/// allowing it to be easily converted to and from various data formats like JSON.
///
/// # Fields
///
/// * `table_id` - A [u64] that uniquely identifies the match table in the system.
/// * `match_table_type` - A [MatchTableType] enumeration that specifies the matching strategy to be used.
/// * `word_list` - A [`Vec<&'a str>`] containing the list of words for matching. The use of `&'a str`
///   allows the words to be borrowed, which can optimize memory usage.
/// * `exemption_simple_match_type` - A [SimpleMatchType] indicating the matching strategy for the exemption words.
/// * `exemption_word_list` - A [`Vec<&'a str>`] containing the list of words to be exempted from matching. Like `word_list`,
///   this is also a borrowed vector to allow efficient memory use.
///
/// # Lifetimes
///
/// * `'a` - The lifetime associated with the `word_list` and `exemption_word_list` fields, ensuring that the data
///   for the words can be borrowed for efficiency.
///
/// # Serde Attributes
///
/// The `borrow` attribute on `word_list` and `exemption_word_list` fields ensures that the deserialized
/// data can borrow from the input data, providing better performance by avoiding unnecessary allocations.
pub struct MatchTable<'a> {
    pub table_id: u64,
    pub match_table_type: MatchTableType,
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
/// * `match_id` - A [u64] representing the identifier of the match within the system.
/// * `table_id` - A [u64] representing the identifier of the table within the system.
/// * `is_exemption` - A [bool] flag that indicates whether the word table is an exemption.
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
/// * `table_id` - A [u64] that uniquely identifies the table in which the match was found.
/// * `word` - A [Cow<'a, str>] that holds the matched word. The [Cow] type allows the word
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

#[derive(Clone)]
/// The [Matcher] struct encapsulates various matching strategies and their configurations used for text processing.
///
/// This structure holds configurations for simple, regex, and similarity-based matchers. It manages
/// different maps and matchers necessary to perform text matching operations.
///
/// # Fields
///
/// * `simple_word_table_conf_map` - An [IntMap<u64, WordTableConf>] that maps word table configuration IDs to their configurations.
/// * `simple_word_table_conf_id_map` - An [IntMap<u64, u64>] that maps word IDs to their corresponding word table configuration IDs.
/// * `simple_matcher` - An [`Option<SimpleMatcher>`] that holds the simple matcher if it exists.
/// * `regex_matcher` - An [`Option<RegexMatcher>`] that holds the regex matcher if it exists.
/// * `sim_matcher` - An [`Option<SimMatcher>`] that holds the similarity matcher if it exists.
///
/// The [Matcher] struct is typically instantiated through the [new](Matcher::new) method, which processes an input map of match tables
/// and initializes the appropriate matchers and data structures.
///
/// # Example
///
/// ```
/// use matcher_rs::{Matcher, MatchTable, MatchTableType, SimpleMatchType};
/// use std::collections::HashMap;
///
/// let mut match_table_map = HashMap::new();
/// match_table_map.insert(
///     1,
///     vec![MatchTable {
///         table_id: 1,
///         match_table_type: MatchTableType::Simple { simple_match_type: SimpleMatchType::None },
///         word_list: vec!["apple", "banana"],
///         exemption_simple_match_type: SimpleMatchType::None,
///         exemption_word_list: vec!["orange"],
///     }],
/// );
///
/// let matcher = Matcher::new(&match_table_map);
/// ```
pub struct Matcher {
    simple_word_table_conf_map: IntMap<u64, WordTableConf>,
    simple_word_table_conf_id_map: IntMap<u64, u64>,
    simple_matcher: Option<SimpleMatcher>,
    regex_matcher: Option<RegexMatcher>,
    sim_matcher: Option<SimMatcher>,
}

impl Matcher {
    /// Creates a new [Matcher] instance from the provided match table map.
    ///
    /// This function processes the input map of match tables to initialize the various
    /// components of the [Matcher] including simple, regex, and similarity-based matchers.
    ///
    /// # Arguments
    ///
    /// * `match_table_map` - A reference to a [HashMap] where the keys are [u64] identifiers
    ///   and the values are vectors of [MatchTable] instances representing different types of match tables.
    ///
    /// # Returns
    ///
    /// A [Matcher] instance initialized with the configurations derived from the provided match table map.
    ///
    /// The construction process involves:
    ///
    /// 1. Iterating through the provided match table map.
    /// 2. Extracting table configurations and populating the corresponding matcher-specific data structures:
    ///     - Simple match type word map
    ///     - Regex table list
    ///     - Similarity table list
    /// 3. Handling exemptions by updating the word table configurations.
    ///
    /// The word and table identifiers are incremented as new entries are processed and added.
    ///
    /// # Example
    ///
    /// ```
    /// use matcher_rs::{Matcher, MatchTable, MatchTableType, SimpleMatchType};
    /// use std::collections::HashMap;
    ///
    /// let mut match_table_map = HashMap::new();
    /// match_table_map.insert(
    ///     1,
    ///     vec![MatchTable {
    ///         table_id: 1,
    ///         match_table_type: MatchTableType::Simple { simple_match_type: SimpleMatchType::None },
    ///         word_list: vec!["apple", "banana"],
    ///         exemption_simple_match_type: SimpleMatchType::None,
    ///         exemption_word_list: vec!["orange"],
    ///     }],
    /// );
    ///
    /// let matcher = Matcher::new(&match_table_map);
    /// ```
    pub fn new<'a, S>(match_table_map: &HashMap<u64, Vec<MatchTable<'a>>, S>) -> Matcher {
        let mut word_id: u64 = 0;
        let mut word_table_conf_id: u64 = 0;

        let mut simple_word_table_conf_map = IntMap::default();
        let mut simple_word_table_conf_id_map = IntMap::default();

        let mut simple_match_type_word_map: IntMap<SimpleMatchType, IntMap<u64, &'a str>> =
            IntMap::default();

        let mut regex_table_list: Vec<RegexTable> = Vec::new();
        let mut sim_table_list: Vec<SimTable> = Vec::new();

        for (&match_id, table_list) in match_table_map {
            for table in table_list {
                let table_id = table.table_id;
                let match_table_type = table.match_table_type;
                let word_list = &table.word_list;
                let exemption_word_list = &table.exemption_word_list;

                if !word_list.is_empty() {
                    match match_table_type {
                        MatchTableType::Simple { simple_match_type } => {
                            simple_word_table_conf_map.insert(
                                word_table_conf_id,
                                WordTableConf {
                                    match_id,
                                    table_id,
                                    is_exemption: false,
                                },
                            );

                            let simple_word_map = simple_match_type_word_map
                                .entry(simple_match_type)
                                .or_default();

                            for word in word_list.iter() {
                                simple_word_table_conf_id_map.insert(word_id, word_table_conf_id);
                                simple_word_map.insert(word_id, word);
                                word_id += 1;
                            }

                            word_table_conf_id += 1
                        }
                        MatchTableType::Similar {
                            sim_match_type,
                            threshold,
                        } => sim_table_list.push(SimTable {
                            table_id,
                            match_id,
                            sim_match_type,
                            word_list,
                            threshold,
                        }),
                        MatchTableType::Regex { regex_match_type } => {
                            regex_table_list.push(RegexTable {
                                table_id,
                                match_id,
                                regex_match_type,
                                word_list,
                            })
                        }
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
                .then(|| SimpleMatcher::new(&simple_match_type_word_map)),
            regex_matcher: (!regex_table_list.is_empty())
                .then(|| RegexMatcher::new(&regex_table_list)),
            sim_matcher: (!sim_table_list.is_empty()).then(|| SimMatcher::new(&sim_table_list)),
        }
    }

    /// Matches the provided text and returns the raw results as a [HashMap] with match identifiers and vectors of [MatchResult]s.
    ///
    /// This function takes a string slice representing the text to be matched and processes it using the available
    /// matchers (simple, regex, and similarity matchers). It gathers the matching results into a [HashMap] where
    /// the keys are match identifiers and the values are vectors of [MatchResult] instances.
    ///
    /// The function proceeds through the following steps:
    ///
    /// 1. **Regex Matching**: If a regex matcher is available, processes the text with it and collects the results.
    /// 2. **Similarity Matching**: If a similarity matcher is available, processes the text with it and collects the results.
    /// 3. **Simple Matching**: If a simple matcher is available, processes the text with it. It also checks for exemptions
    ///    and updates the match results accordingly.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be matched.
    ///
    /// # Returns
    ///
    /// A [`HashMap<u64, Vec<MatchResult>>`] where the keys are match identifiers and the values are vectors of [MatchResult]
    /// instances containing the matching results for each identifier.
    ///
    /// If the provided text is empty, the function returns an empty [HashMap].
    pub fn word_match(&self, text: &str) -> HashMap<u64, Vec<MatchResult>> {
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

    /// Matches the provided text and returns the raw results as a serialized JSON string.
    ///
    /// This function takes a string slice representing the text to be matched and processes it using the available
    /// matchers (simple, regex, and similarity matchers). It gathers the matching results into a [HashMap] where
    /// the keys are match identifiers and the values are vectors of [MatchResult] instances. The results are then
    /// serialized into a JSON string using the [to_string] function from the [sonic_rs] crate.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be matched.
    ///
    /// # Returns
    ///
    /// A [String] containing the serialized JSON representation of the raw matching results.
    ///
    /// # Safety
    ///
    /// The function uses an `unsafe` block to call [unwrap_unchecked](Result::unwrap_unchecked) on the [to_string] function, which skips
    /// the error checking for performance optimization. It is important to ensure that the serialization process
    /// does not fail, as [unwrap_unchecked](Result::unwrap_unchecked) will cause undefined behavior if an error occurs.
    pub fn word_match_as_string(&self, text: &str) -> String {
        unsafe { to_string(&self.word_match(text)).unwrap_unchecked() }
    }
}

impl<'a> TextMatcherTrait<'a, MatchResult<'a>> for Matcher {
    fn is_match(&self, text: &str) -> bool {
        !self.word_match(text).is_empty()
    }

    /// Processes the provided text and returns a vector of [MatchResult] instances.
    ///
    /// This function takes a string slice representing the text to be processed and matches it using the available
    /// matchers (simple, regex, and similarity matchers). It gathers the matching results and organizes them
    /// by their respective match identifiers. The results for each match identifier are then flattened into a single
    /// vector of [MatchResult] instances.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice representing the text to be processed.
    ///
    /// # Returns
    ///
    /// A [Vec] of [MatchResult] instances containing the matching results for all match identifiers.
    fn process(&'a self, text: &str) -> Vec<MatchResult<'a>> {
        self.word_match(text)
            .into_iter()
            .flat_map(|(_, result_list)| result_list) // Flatten the result lists from all match IDs into a single iterator.
            .collect()
    }
}
