use std::borrow::Cow;
use std::collections::HashMap;

use id_set::IdSet;
use nohash_hasher::IntMap;
use serde::{Deserialize, Serialize};

use crate::process::process_matcher::{
    build_process_type_tree, reduce_text_process_with_tree, ProcessType, ProcessTypeBitNode,
};
use crate::regex_matcher::{RegexMatchType, RegexMatcher, RegexResult, RegexTable};
use crate::sim_matcher::{SimMatchType, SimMatcher, SimResult, SimTable};
use crate::simple_matcher::{SimpleMatcher, SimpleTable};

/// Trait defining the behavior of text matching.
///
/// This trait is designed to work with various types of match results and provides methods to
/// determine if a text matches certain criteria and process the text to produce match results.
///
/// # Type Parameters
///
/// - `'a`: Lifetime parameter associated with the trait and match results.
/// - `T`: A type that implements [`MatchResultTrait<'a>`] and has the same lifetime as `'a`.
///
/// # Provided Methods
///
/// - `is_match`: Checks if the given text matches the criteria defined by the implementation.
/// - `_is_match_with_processed_text_process_type_set`: Checks if the given processed text and
///   associated [IdSet] matches the criteria defined by the implementation.
/// - `process`: Processes the given text and returns a [Vec] of match results of type `T`.
/// - `_process_with_processed_text_process_type_set`: Processes the given processed text and
///   associated [IdSet] to produce a [Vec] of match results of type `T`.
/// - `process_iter`: Processes the given text and returns an iterator over match results of type `T`.
pub trait TextMatcherTrait<'a, T: MatchResultTrait<'a> + 'a> {
    fn is_match(&'a self, text: &'a str) -> bool;
    fn _is_match_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> bool;
    fn process(&'a self, text: &'a str) -> Vec<T>;
    fn _process_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> Vec<T>;
    fn process_iter(&'a self, text: &'a str) -> Box<dyn Iterator<Item = T> + 'a> {
        Box::new(self.process(text).into_iter())
    }
}

/// A trait defining the required methods for a match result.
///
/// This trait is essential for any match result type used within the [TextMatcherTrait] to ensure
/// a consistent interface for accessing match result properties. The trait includes methods to
/// retrieve the match ID, table ID, word ID, the matched word, and the similarity score. Any type
/// implementing this trait can be seamlessly used as a match result in the text matching operations.
///
/// # Lifetimes
///
/// - `'a`: A lifetime parameter associated with the trait and match result, indicating the lifespan
///   of the references returned by the trait methods.
///
/// # Required Methods
///
/// - `match_id(&self) -> u32`: Returns the match ID associated with the result.
/// - `table_id(&self) -> u32`: Returns the table ID where the match was found.
/// - `word_id(&self) -> u32`: Returns the word ID within the table.
/// - `word(&self) -> &str`: Returns a reference to the matched word.
/// - `similarity(&self) -> f64`: Returns the similarity score of the match.
///
/// # Examples
///
/// Below is an example implementation of the [MatchResultTrait] for a struct [MatchResult]:
///
/// ```rust
/// use std::borrow::Cow;
///
/// use matcher_rs::MatchResultTrait;
///
/// struct MatchResult<'a> {
///     match_id: u32,
///     table_id: u32,
///     word_id: u32,
///     word: Cow<'a, str>,
///     similarity: f64,
/// }
///
/// impl<'a> MatchResultTrait<'a> for MatchResult<'a> {
///     fn match_id(&self) -> u32 {
///         self.match_id
///     }
///     fn table_id(&self) -> u32 {
///         self.table_id
///     }
///     fn word_id(&self) -> u32 {
///         self.word_id
///     }
///     fn word(&self) -> &str {
///         self.word.as_ref()
///     }
///     fn similarity(&self) -> f64 {
///         self.similarity
///     }
/// }
/// ```
pub trait MatchResultTrait<'a> {
    fn match_id(&self) -> u32;
    fn table_id(&self) -> u32;
    fn word_id(&self) -> u32;
    fn word(&self) -> &str;
    fn similarity(&self) -> f64;
}

/// An enumeration representing different types of match tables.
///
/// This enum is used to specify the type of matching strategy along with associated configurations
/// that should be applied to the input text.
///
/// Variants:
///
/// - [MatchTableType::Simple]: Represents a simple text matching strategy.
///   - `process_type`: The type of text processing to apply.
///
/// - [MatchTableType::Regex]: Represents a regex-based matching strategy.
///   - `regex_match_type`: The type of regex matching.
///   - `process_type`: The type of text processing to apply.
///
/// - [MatchTableType::Similar]: Represents a similarity-based matching strategy.
///   - `sim_match_type`: The type of similarity matching.
///   - `threshold`: The similarity threshold that needs to be met.
///   - `process_type`: The type of text processing to apply.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MatchTableType {
    Simple {
        process_type: ProcessType,
    },
    Regex {
        regex_match_type: RegexMatchType,
        process_type: ProcessType,
    },
    Similar {
        sim_match_type: SimMatchType,
        threshold: f64,
        process_type: ProcessType,
    },
}

/// A trait that specifies the required methods for accessing match table configurations.
///
/// This trait is designed to provide a consistent interface for any match table type, allowing
/// access to essential properties such as table IDs, match table types, word lists, and exemption
/// word lists. By implementing this trait, different match table structures can be used
/// interchangeably within the text matching operations.
///
/// # Type Parameters
///
/// - `S`: A type that implements `AsRef<str>`, ensuring the trait can be used with various string-like types.
///
/// # Required Methods
///
/// - `table_id(&self) -> u32`: Returns the unique identifier for the match table.
/// - `match_table_type(&self) -> MatchTableType`: Returns the type of matching strategy used by the table.
/// - `word_list(&self) -> &Vec<S>`: Returns a reference to the vector of words used for matching operations.
/// - `exemption_process_type(&self) -> ProcessType`: Returns the type of text processing applied to the exemption words.
/// - `exemption_word_list(&self) -> &Vec<S>`: Returns a reference to the vector of words exempted from matching operations.
///
/// By implementing this trait, any custom match table structure can ensure compatibility with the
/// text matching system, providing the necessary access to its configuration details.
pub trait MatchTableTrait<S: AsRef<str>> {
    fn table_id(&self) -> u32;
    fn match_table_type(&self) -> MatchTableType;
    fn word_list(&self) -> &Vec<S>;
    fn exemption_process_type(&self) -> ProcessType;
    fn exemption_word_list(&self) -> &Vec<S>;
}

/// A structure representing a match table configuration.
///
/// Match tables are used to define different matching strategies along with associated words and
/// vocabulary exemption lists. Each match table contains an ID, a type specifying the kind of
/// matching strategy, a list of words to match against, and an optional list of words to exempt
/// from matching. Additionally, each table specifies the type of text processing to apply for both
/// regular and exemption word lists.
///
/// # Lifetimes
///
/// - `'a`: A lifetime parameter associated with the match table, indicating the lifespan of the
///   borrowed strings contained in the word lists.
///
/// # Fields
///
/// - `table_id: u32`: A unique identifier for the match table.
/// - `match_table_type: MatchTableType`: The type of matching strategy (e.g., Simple, Regex,
///   Similar) used by this table.
/// - `word_list: Vec<&'a str>`: A vector of words to be used for matching operations. The lifetime
///   `'a` ensures that the borrowed strings live at least as long as the match table.
/// - `exemption_process_type: ProcessType`: The type of text processing to apply to exemption
///   words.
/// - `exemption_word_list: Vec<&'a str>`: A vector of words that should be exempted from matching
///   operations. The lifetime `'a` ensures that the borrowed strings live at least as long as the
///   match table.
///
/// # Serde Attributes
///
/// - `#[derive(Serialize, Deserialize)]`: Automatically implements serialization and
///   deserialization for the [MatchTable] struct, enabling easy conversion to and from different
///   data formats (e.g., JSON).
/// - `#[serde(borrow)]`: This attribute specifies that the deserializer should borrow data for the
///   fields marked with `'a`, reducing unnecessary allocations and improving performance.
///
/// # Examples
///
/// The following is an example of how a [MatchTable] might be instantiated:
///
/// ```rust
/// use matcher_rs::{MatchTable, MatchTableType, ProcessType};
///
/// let match_table = MatchTable {
///     table_id: 1,
///     match_table_type: MatchTableType::Simple {
///         process_type: ProcessType::None,
///     },
///     word_list: vec!["example", "sample"],
///     exemption_process_type: ProcessType::None,
///     exemption_word_list: vec!["ignore", "skip"],
/// };
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MatchTable<'a> {
    pub table_id: u32,
    pub match_table_type: MatchTableType,
    #[serde(borrow)]
    pub word_list: Vec<&'a str>,
    pub exemption_process_type: ProcessType,
    #[serde(borrow)]
    pub exemption_word_list: Vec<&'a str>,
}

impl<'a> MatchTableTrait<&'a str> for MatchTable<'a> {
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn match_table_type(&self) -> MatchTableType {
        self.match_table_type
    }
    fn word_list(&self) -> &Vec<&'a str> {
        &self.word_list
    }
    fn exemption_process_type(&self) -> ProcessType {
        self.exemption_process_type
    }
    fn exemption_word_list(&self) -> &Vec<&'a str> {
        &self.exemption_word_list
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MatchTableSerde<'a> {
    pub table_id: u32,
    pub match_table_type: MatchTableType,
    #[serde(borrow)]
    pub word_list: Vec<Cow<'a, str>>,
    pub exemption_process_type: ProcessType,
    #[serde(borrow)]
    pub exemption_word_list: Vec<Cow<'a, str>>,
}

impl<'a> MatchTableTrait<Cow<'a, str>> for MatchTableSerde<'a> {
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn match_table_type(&self) -> MatchTableType {
        self.match_table_type
    }
    fn word_list(&self) -> &Vec<Cow<'a, str>> {
        &self.word_list
    }
    fn exemption_process_type(&self) -> ProcessType {
        self.exemption_process_type
    }
    fn exemption_word_list(&self) -> &Vec<Cow<'a, str>> {
        &self.exemption_word_list
    }
}

/// A configuration structure representing a word table entry.
///
/// This structure is used to define the configuration for a specific word table entry,
/// including the match table ID, offset, and whether the entry is for exemption words.
///
/// # Fields
///
/// - `match_id: u32`: A unique identifier for the matching operation this word table configuration belongs to.
/// - `table_id: u32`: A unique identifier for the match table to which this configuration applies.
/// - `offset: u32`: The position offset within the word table for this configuration entry.
/// - `is_exemption: bool`: A flag indicating whether this configuration entry is for exemption words (true)
///   or for regular matching words (false).
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct WordTableConf {
    match_id: u32,
    table_id: u32,
    offset: u32,
    is_exemption: bool,
}

/// A structure representing the results of a matching operation.
///
/// The [MatchResult] struct contains detailed information about the results of a single matching
/// operation including the match identifier, table identifier, word identifier, the matched word
/// itself, and a similarity score.
///
/// # Lifetimes
///
/// - `'a`: The lifetime of the borrowed string contained in the `word` field. This ensures that
///   the referenced word lives at least as long as the [MatchResult].
///
/// # Fields
///
/// - `match_id: u32`: A unique identifier for the matching operation.
/// - `table_id: u32`: A unique identifier for the match table associated with this result.
/// - `word_id: u32`: A unique identifier for the matched word.
/// - `word: Cow<'a, str>`: The word that was matched. Uses a [Cow] (Copy on Write) to allow
///   flexibility in whether the word is borrowed or owned.
/// - `similarity: f64`: The similarity score of the matched word. This is typically used for
///   similarity-based matching operations to represent how closely the word matches the criteria.
///
/// # Examples
///
/// The following example demonstrates how to create an instance of [MatchResult]:
///
/// ```rust
/// use std::borrow::Cow;
///
/// use matcher_rs::MatchResult;
///
/// let match_result = MatchResult {
///     match_id: 1,
///     table_id: 101,
///     word_id: 1001,
///     word: Cow::Borrowed("example"),
///     similarity: 0.95,
/// };
/// ```
#[derive(Serialize)]
pub struct MatchResult<'a> {
    pub match_id: u32,
    pub table_id: u32,
    pub word_id: u32,
    pub word: Cow<'a, str>,
    pub similarity: f64,
}

impl MatchResultTrait<'_> for MatchResult<'_> {
    fn match_id(&self) -> u32 {
        self.match_id
    }
    fn word_id(&self) -> u32 {
        self.word_id
    }
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
    fn similarity(&self) -> f64 {
        self.similarity
    }
}

impl<'a, 'b: 'a> From<SimResult<'b>> for MatchResult<'a> {
    fn from(sim_result: SimResult<'b>) -> Self {
        MatchResult {
            match_id: sim_result.match_id,
            table_id: sim_result.table_id,
            word_id: sim_result.word_id,
            word: sim_result.word,
            similarity: sim_result.similarity,
        }
    }
}

impl<'a, 'b: 'a> From<RegexResult<'b>> for MatchResult<'a> {
    fn from(regex_result: RegexResult<'b>) -> Self {
        MatchResult {
            match_id: regex_result.match_id,
            table_id: regex_result.table_id,
            word_id: regex_result.word_id,
            word: regex_result.word,
            similarity: 1.0,
        }
    }
}

/// A type alias for a mapping from match table IDs to their corresponding [MatchTable]s.
///
/// This mapping uses an [IntMap] where:
/// - The key is a `u32` that represents the unique identifier of a match table.
/// - The value is a [Vec] containing the list of [MatchTable] structures associated with the match table ID.
///
/// # Lifetimes
///
/// - `'a`: The lifetime of the borrowed data within the [MatchTable] structures.
///
/// # Example
///
/// ```rust
/// use std::borrow::Cow;
/// use nohash_hasher::IntMap;
///
/// use matcher_rs::{MatchTable, MatchTableMap, MatchTableType, ProcessType, RegexMatchType};
///
/// // Sample match table entries
/// let match_table_1 = MatchTable {
///     table_id: 1,
///     match_table_type: MatchTableType::Simple { process_type: ProcessType::None },
///     word_list: vec!["word1", "word2"],
///     exemption_process_type: ProcessType::None,
///     exemption_word_list: vec!["ignore"],
/// };
///
/// let match_table_2 = MatchTable {
///     table_id: 2,
///     match_table_type: MatchTableType::Regex { process_type: ProcessType::None, regex_match_type: RegexMatchType::Regex },
///     word_list: vec!["regex1", "regex2"],
///     exemption_process_type: ProcessType::None,
///     exemption_word_list: vec!["skip"],
/// };
///
/// // Create a match table map
/// let mut match_table_map: MatchTableMap = IntMap::default();
/// match_table_map.insert(1, vec![match_table_1]);
/// match_table_map.insert(2, vec![match_table_2]);
/// ```
pub type MatchTableMap<'a> = IntMap<u32, Vec<MatchTable<'a>>>;

pub type MatchTableMapSerde<'a> = IntMap<u32, Vec<MatchTableSerde<'a>>>;

/// The [Matcher] struct is responsible for managing and facilitating various types of matching operations
/// utilizing different word processing strategies and match table configurations.
///
/// Fields:
///
/// - `process_type_tree: Vec<ProcessTypeBitNode>`: A vector representing the tree structure of process types,
///   used to manage the hierarchy and relationships between different word processing steps.
///
/// - `simple_word_table_conf_list: Vec<WordTableConf>`: A vector containing configurations for simple word
///   matching tables. Each entry represents the configuration for a specific table, including its match ID,
///   table ID, offset, and whether it is an exemption table.
///
/// - `simple_word_table_conf_index_list: Vec<usize>`: A vector indexing entries in `simple_word_table_conf_list`
///   for efficient look-up and retrieval.
///
/// - `simple_matcher: Option<SimpleMatcher>`: An optional [SimpleMatcher] used to perform simple word matching
///   operations if any such tables are configured.
///
/// - `regex_matcher: Option<RegexMatcher>`: An optional [RegexMatcher] used to perform regular expression based
///   matching operations if any such tables are configured.
///
/// - `sim_matcher: Option<SimMatcher>`: An optional [SimMatcher] used to perform similarity-based matching
///   operations if any such tables are configured.
///
/// The [Matcher] struct is designed to be serialized and deserialized conditionally by leveraging the `serde`
/// feature, ensuring flexibility in its usage and integration with various systems and data transfer scenarios.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Matcher {
    process_type_tree: Vec<ProcessTypeBitNode>,
    simple_word_table_conf_list: Vec<WordTableConf>,
    simple_word_table_conf_index_list: Vec<usize>,
    simple_matcher: Option<SimpleMatcher>,
    regex_matcher: Option<RegexMatcher>,
    sim_matcher: Option<SimMatcher>,
}

impl Matcher {
    /// Constructs a new [Matcher] instance from a given match table map.
    ///
    /// This method initializes the [Matcher] by processing the provided match table map and
    /// configuring various matching components (simple word tables, regex tables, and similarity
    /// tables) based on the match table configurations.
    ///
    /// # Arguments
    ///
    /// * `match_table_map` - A reference to a [HashMap] where:
    ///     - The key (`u32`) represents the unique identifier for a match table.
    ///     - The value ([`Vec<MatchTable<'_>>`]) is a vector of [MatchTable] structs associated with the match table ID.
    ///
    /// # Returns
    ///
    /// Returns an initialized [Matcher] that is capable of performing different types of word matching
    /// operations (simple, regex, similarity) based on the provided match table configurations.
    ///
    /// # Example
    ///
    /// ```rust
    /// use std::borrow::Cow;
    /// use nohash_hasher::IntMap;
    /// use matcher_rs::{MatchTable, MatchTableType, ProcessType, RegexMatchType, Matcher};
    /// use std::collections::HashMap;
    ///
    /// let match_table_1 = MatchTable {
    ///     table_id: 1,
    ///     match_table_type: MatchTableType::Simple { process_type: ProcessType::None },
    ///     word_list: vec!["word1", "word2"],
    ///     exemption_process_type: ProcessType::None,
    ///     exemption_word_list: vec!["ignore"],
    /// };
    ///
    /// let match_table_2 = MatchTable {
    ///     table_id: 2,
    ///     match_table_type: MatchTableType::Regex { process_type: ProcessType::None, regex_match_type: RegexMatchType::Regex },
    ///     word_list: vec!["regex1", "regex2"],
    ///     exemption_process_type: ProcessType::None,
    ///     exemption_word_list: vec!["skip"],
    /// };
    ///
    /// let mut match_table_map: HashMap<u32, Vec<MatchTable>> = HashMap::new();
    /// match_table_map.insert(1, vec![match_table_1]);
    /// match_table_map.insert(2, vec![match_table_2]);
    ///
    /// let matcher = Matcher::new(&match_table_map);
    /// ```
    pub fn new<S, M, T>(match_table_map: &HashMap<u32, Vec<M>, S>) -> Matcher
    where
        M: MatchTableTrait<T>,
        T: AsRef<str>,
    {
        let mut process_type_set = IdSet::new();

        let mut simple_word_id = 0;
        let mut simple_word_table_conf_id = 0;
        let mut simple_word_table_conf_list = Vec::new();
        let mut simple_word_table_conf_index_list = Vec::new();
        let mut simple_table: SimpleTable = IntMap::default();

        let mut regex_table_list = Vec::new();
        let mut sim_table_list = Vec::new();

        for (&match_id, table_list) in match_table_map {
            for table in table_list {
                let table_id = table.table_id();
                let match_table_type = table.match_table_type();
                let word_list = table
                    .word_list()
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<&str>>();
                let exemption_process_type = table.exemption_process_type();
                let exemption_word_list = table
                    .exemption_word_list()
                    .iter()
                    .map(|s| s.as_ref())
                    .collect::<Vec<&str>>();

                if !word_list.is_empty() {
                    match match_table_type {
                        MatchTableType::Simple { process_type } => {
                            process_type_set.insert(process_type.bits() as usize);
                            simple_word_table_conf_list.push(WordTableConf {
                                match_id,
                                table_id,
                                offset: simple_word_id,
                                is_exemption: false,
                            });

                            let simple_word_map = simple_table.entry(process_type).or_default();

                            for word in word_list {
                                simple_word_table_conf_index_list.push(simple_word_table_conf_id);
                                simple_word_map.insert(simple_word_id, word);
                                simple_word_id += 1;
                            }

                            simple_word_table_conf_id += 1
                        }
                        MatchTableType::Similar {
                            process_type,
                            sim_match_type,
                            threshold,
                        } => {
                            process_type_set.insert(process_type.bits() as usize);
                            sim_table_list.push(SimTable {
                                table_id,
                                match_id,
                                process_type,
                                sim_match_type,
                                word_list,
                                threshold,
                            })
                        }
                        MatchTableType::Regex {
                            process_type,
                            regex_match_type,
                        } => {
                            process_type_set.insert(process_type.bits() as usize);
                            regex_table_list.push(RegexTable {
                                table_id,
                                match_id,
                                process_type,
                                regex_match_type,
                                word_list,
                            })
                        }
                    }
                }

                if !exemption_word_list.is_empty() {
                    process_type_set.insert(exemption_process_type.bits() as usize);
                    simple_word_table_conf_list.push(WordTableConf {
                        match_id,
                        table_id,
                        offset: simple_word_id,
                        is_exemption: true,
                    });

                    let simple_word_map = simple_table.entry(exemption_process_type).or_default();

                    for exemption_word in exemption_word_list.iter() {
                        simple_word_table_conf_index_list.push(simple_word_table_conf_id);
                        simple_word_map.insert(simple_word_id, exemption_word);
                        simple_word_id += 1;
                    }

                    simple_word_table_conf_id += 1
                }
            }
        }

        let process_type_tree = build_process_type_tree(&process_type_set);

        Matcher {
            process_type_tree,
            simple_word_table_conf_list,
            simple_word_table_conf_index_list,
            simple_matcher: (!simple_table.is_empty()).then(|| SimpleMatcher::new(&simple_table)),
            regex_matcher: (!regex_table_list.is_empty())
                .then(|| RegexMatcher::new(&regex_table_list)),
            sim_matcher: (!sim_table_list.is_empty()).then(|| SimMatcher::new(&sim_table_list)),
        }
    }

    /// Matches words in the given text based on the configured match tables.
    ///
    /// This function processes the input text through various match tables
    /// configured in the [Matcher] instance. It handles simple word matches,
    /// regex matches, and similarity matches by checking against the processed
    /// text and returning the results in a [HashMap].
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice that holds the text to be matched against the
    ///   match tables.
    ///
    /// # Returns
    ///
    /// * [`HashMap<u32, Vec<MatchResult>>`] - A map where keys are match IDs and
    ///   values are vectors of [MatchResult] items. Each [MatchResult] holds
    ///   information about a match found in the input text.
    ///
    /// If the input text is empty, the function returns an empty [HashMap].
    pub fn word_match<'a>(&'a self, text: &'a str) -> HashMap<u32, Vec<MatchResult<'a>>> {
        if text.is_empty() {
            return HashMap::new();
        }

        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._word_match_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    /// Matches processed text against the configured match tables.
    ///
    /// This function takes a set of processed text pieces, represented by
    /// `processed_text_process_type_set`, and checks them against the various
    /// types of match tables defined in the [Matcher] instance (simple, regex, and
    /// similarity match tables).
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_set` - A reference to a slice of tuples,
    ///   where each tuple contains a processed text piece (as [`Cow<str>`]) and a
    ///   set of process type IDs ([IdSet]).
    ///
    /// # Returns
    ///
    /// * [`HashMap<u32, Vec<MatchResult>>`] - A map where keys are match IDs and
    ///   values are vectors of [MatchResult] items. Each [MatchResult] holds
    ///   information about a match found in the corresponding match table.
    ///   If no matches are found, the function returns an empty [HashMap].
    ///
    /// # Safety
    ///
    /// Unsafe code is used to access elements in `simple_word_table_conf_list`
    /// and `simple_word_table_conf_index_list` without bounds checks for
    /// performance reasons. Ensure these operations remain safe when modifying
    /// the underlying data structures.
    fn _word_match_with_processed_text_process_type_set<'a>(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> HashMap<u32, Vec<MatchResult<'a>>> {
        let mut match_result_dict = HashMap::new();
        let mut failed_match_table_id_set = IdSet::new();

        if let Some(regex_matcher) = &self.regex_matcher {
            for regex_result in regex_matcher
                ._process_with_processed_text_process_type_set(processed_text_process_type_set)
            {
                let result_list: &mut Vec<MatchResult> = match_result_dict
                    .entry(regex_result.match_id)
                    .or_insert(Vec::new());

                result_list.push(regex_result.into());
            }
        }

        if let Some(sim_matcher) = &self.sim_matcher {
            for sim_result in sim_matcher
                ._process_with_processed_text_process_type_set(processed_text_process_type_set)
            {
                let result_list = match_result_dict
                    .entry(sim_result.match_id)
                    .or_insert(Vec::new());

                result_list.push(sim_result.into());
            }
        }

        if let Some(simple_matcher) = &self.simple_matcher {
            for simple_result in simple_matcher
                ._process_with_processed_text_process_type_set(processed_text_process_type_set)
            {
                // Guaranteed not failed
                let word_table_conf = unsafe {
                    self.simple_word_table_conf_list.get_unchecked(
                        *self
                            .simple_word_table_conf_index_list
                            .get_unchecked(simple_result.word_id as usize),
                    )
                };
                let match_table_id = ((word_table_conf.match_id as usize) << 32)
                    | (word_table_conf.table_id as usize);

                if failed_match_table_id_set.contains(match_table_id) {
                    continue;
                }

                let result_list = match_result_dict
                    .entry(word_table_conf.match_id)
                    .or_insert(Vec::new());
                if word_table_conf.is_exemption {
                    failed_match_table_id_set.insert(match_table_id);
                    result_list
                        .retain(|match_result| match_result.table_id != word_table_conf.table_id);
                } else {
                    result_list.push(MatchResult {
                        match_id: word_table_conf.match_id,
                        table_id: word_table_conf.table_id,
                        word_id: unsafe {
                            simple_result.word_id.unchecked_sub(word_table_conf.offset)
                        },
                        word: simple_result.word,
                        similarity: 1.0,
                    });
                }
            }
        }

        match_result_dict.retain(|_, match_result_list| !match_result_list.is_empty());
        match_result_dict
    }
}

impl<'a> TextMatcherTrait<'a, MatchResult<'a>> for Matcher {
    /// Checks if the given text matches any pattern in the match tables.
    ///
    /// This function processes the input text using the `process_type_tree`
    /// defined for the [Matcher] instance and then checks if any matches
    /// are found using the underlying match tables (simple, regex, and
    /// similarity match tables).
    ///
    /// # Arguments
    ///
    /// * `text` - A reference to the input text string to be matched.
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if any matches are found, otherwise returns `false`.
    fn is_match(&self, text: &str) -> bool {
        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._is_match_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    /// Checks if there are any matches for the processed text within the configured match tables.
    ///
    /// This function takes a reference to a processed text set and determines if any matches
    /// exist within the match tables of the [Matcher] instance. The function prioritizes
    /// checking the simple matcher first. If the simple matcher is not configured or
    /// doesn't find any matches, it proceeds to check the regex matcher and then the
    /// similarity matcher, in that order.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_set` - A reference to a list of tuples where each tuple
    ///   contains a processed text (as a [Cow<'a, str>]) and an associated [IdSet].
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if any matches are found within any of the matchers, otherwise `false`.
    ///
    /// # Safety
    ///
    /// This function is safe to use under normal circumstances but depends on the reliability
    /// of the underlying matchers and the integrity of the `processed_text_process_type_set`
    /// input. Ensure the input data is correctly processed and the matchers are properly
    /// initialized before calling this function.
    fn _is_match_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> bool {
        match &self.simple_matcher {
            Some(_) => !self
                ._word_match_with_processed_text_process_type_set(processed_text_process_type_set)
                .is_empty(),
            None => {
                if let Some(regex_matcher) = &self.regex_matcher {
                    if regex_matcher._is_match_with_processed_text_process_type_set(
                        processed_text_process_type_set,
                    ) {
                        return true;
                    }
                }
                if let Some(sim_matcher) = &self.sim_matcher {
                    if sim_matcher._is_match_with_processed_text_process_type_set(
                        processed_text_process_type_set,
                    ) {
                        return true;
                    }
                }
                false
            }
        }
    }

    /// Processes the input text to generate a list of match results.
    ///
    /// This function takes an input text string, processes it according to the
    /// [Matcher] instance's configured process type tree, and then generates a
    /// list of match results by applying the processed text against the configured
    /// match tables.
    ///
    /// The process involves reducing the input text based on the type tree, transforming
    /// it into a structured format (`processed_text_process_type_set`) suitable for
    /// matching operations. The results are then aggregated into a single list of
    /// [MatchResult] instances.
    ///
    /// # Arguments
    ///
    /// * `text` - A reference to the input text string to be processed.
    ///
    /// # Returns
    ///
    /// * [`Vec<MatchResult<'a>>`] - A vector containing match results corresponding to
    ///   the patterns defined in the match tables.
    fn process(&'a self, text: &'a str) -> Vec<MatchResult<'a>> {
        let processed_text_process_type_set =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._process_with_processed_text_process_type_set(&processed_text_process_type_set)
    }

    /// Aggregates match results by processing the pre-processed text with the configured matchers.
    ///
    /// This function takes a reference to a pre-processed text set (a list of tuples containing
    /// processed text and associated [IdSet]) and generates match results using the instance's
    /// configured matchers. The function focuses on word-level matching and aggregates the
    /// results into a single list of [MatchResult] instances.
    ///
    /// The process involves invoking the appropriate matcher to obtain match results for the
    /// provided pre-processed text and then flattening the results into a single vector.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_set` - A reference to a list of tuples where each tuple
    ///   contains a pre-processed text (as a [`Cow<'a, str>`]) and an associated [IdSet].
    ///
    /// # Returns
    ///
    /// * [`Vec<MatchResult<'a>>`] - A vector containing aggregated match results generated
    ///   from the match IDs.
    fn _process_with_processed_text_process_type_set(
        &'a self,
        processed_text_process_type_set: &[(Cow<'a, str>, IdSet)],
    ) -> Vec<MatchResult<'a>> {
        self._word_match_with_processed_text_process_type_set(processed_text_process_type_set)
            .into_iter()
            .flat_map(|(_, result_list)| result_list) // Flatten the result lists from all match IDs into a single iterator.
            .collect()
    }
}
