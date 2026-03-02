use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::process::process_matcher::{
    ProcessType, ProcessTypeBitNode, ProcessedTextMasks, build_process_type_tree,
    reduce_text_process_with_tree,
};
use crate::regex_matcher::{RegexMatchType, RegexMatcher, RegexResult, RegexTable};
use crate::sim_matcher::{SimMatchType, SimMatcher, SimResult, SimTable};
use crate::simple_matcher::{SimpleMatcher, SimpleTable};

/// Text-matching trait shared by all matcher types.
///
/// # Type Parameters
/// * `'a` - Lifetime parameter associated with the trait and match results.
/// * `T` - A type that implements [`MatchResultTrait<'a>`] and has the same lifetime as `'a`.
///
/// # Public API
///
/// External code should call [`is_match`](TextMatcherTrait::is_match),
/// [`process`](TextMatcherTrait::process), and
/// [`process_iter`](TextMatcherTrait::process_iter).
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement text matching",
    label = "this type cannot be used as a matcher",
    note = "implement `TextMatcherTrait` or use one of the built-in matchers: `SimpleMatcher`, `RegexMatcher`, `SimMatcher`, or `Matcher`"
)]
pub trait TextMatcherTrait<'a, T: MatchResultTrait<'a> + 'a> {
    fn is_match(&'a self, text: &'a str) -> bool {
        self.process_iter(text).next().is_some()
    }
    fn process(&'a self, text: &'a str) -> Vec<T> {
        self.process_iter(text).collect()
    }
    fn process_iter(&'a self, text: &'a str) -> impl Iterator<Item = T> + 'a;
}

/// Internal trait for preprocessed-text matching. Not part of the public API.
///
/// These methods accept already-reduced text (a [`ProcessedTextMasks`]) rather than
/// raw input, avoiding redundant preprocessing when the same reduced text is reused
/// across multiple matchers (e.g. inside [`Matcher`]).
///
/// # Type Parameters
/// * `'a` - Lifetime parameter associated with the trait and match results.
/// * `T` - A type that implements [`MatchResultTrait<'a>`] and has the same lifetime as `'a`.
pub(crate) trait TextMatcherInternal<'a, T: MatchResultTrait<'a> + 'a> {
    fn is_match_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> bool;
    fn process_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<T>;
}

/// A trait defining the required methods for a match result.
///
/// This trait is essential for any match result type used within the [`TextMatcherTrait`] to ensure
/// a consistent interface for accessing match result properties. The trait includes methods to
/// retrieve the match ID, table ID, word ID, the matched word, and the similarity score. Any type
/// implementing this trait can be seamlessly used as a match result in the text matching operations.
///
/// # Type Parameters
/// * `'a` - A lifetime parameter associated with the trait and match result, indicating the lifespan
///   of the references returned by the trait methods.
///
/// # Required Methods
/// * `match_id(&self) -> u32` - Returns the match ID associated with the result.
/// * `table_id(&self) -> u32` - Returns the table ID where the match was found.
/// * `word_id(&self) -> u32` - Returns the word ID within the table.
/// * `word(&self) -> &str` - Returns a reference to the matched word.
/// * `similarity(&self) -> Option<f64>` - Returns the similarity score of the match.
///
/// # Examples
///
/// Below is an example implementation of the [`MatchResultTrait`] for a struct `MatchResult`:
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
///     similarity: Option<f64>,
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
///     fn similarity(&self) -> Option<f64> {
///         self.similarity
///     }
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` does not implement `MatchResultTrait`",
    label = "this type cannot be used as a match result",
    note = "implement `MatchResultTrait` with `match_id`, `table_id`, `word_id`, `word`, and `similarity` methods"
)]
pub trait MatchResultTrait<'a> {
    fn match_id(&self) -> u32;
    fn table_id(&self) -> u32;
    fn word_id(&self) -> u32;
    fn word(&self) -> &str;
    fn similarity(&self) -> Option<f64>;
}

/// An enumeration representing different types of match tables.
///
/// This enum is used to specify the type of matching strategy along with associated configurations
/// that should be applied to the input text.
///
/// # Variants
/// * `Simple` - Represents a simple text matching strategy, holding a `process_type`.
/// * `Regex` - Represents a regex-based matching strategy, holding a `regex_match_type` and `process_type`.
/// * `Similar` - Represents a similarity-based matching strategy, holding a `sim_match_type`, `threshold`, and `process_type`.
///
/// # Serialization
///
/// When using the `serde` feature, this enum serializes as a tagged union using `snake_case`. For example, in JSON:
/// - `{"simple": {"process_type": 1}}`
/// - `{"similar": {"sim_match_type": "levenshtein", "threshold": 0.8, "process_type": 1}}`
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
/// * `S` - A type that implements `AsRef<str>`, ensuring the trait can be used with various string-like types.
///
/// # Required Methods
/// * `table_id(&self) -> u32` - Returns the unique identifier for the specific matching table.
/// * `match_table_type(&self) -> MatchTableType` - Returns the type of matching strategy used by the table.
/// * `word_list(&self) -> &[S]` - Returns a reference to the slice of words used for matching operations.
/// * `exemption_process_type(&self) -> ProcessType` - Returns the type of text processing applied to the exemption words.
/// * `exemption_word_list(&self) -> &[S]` - Returns a reference to the slice of words exempted from matching operations.
pub trait MatchTableTrait<S: AsRef<str>> {
    fn table_id(&self) -> u32;
    fn match_table_type(&self) -> MatchTableType;
    fn word_list(&self) -> &[S];
    fn exemption_process_type(&self) -> ProcessType;
    fn exemption_word_list(&self) -> &[S];
}

/// A structure representing a match table configuration.
///
/// Match tables are used to define different matching strategies along with associated words and
/// vocabulary exemption lists. Each match table contains an ID, a type specifying the kind of
/// matching strategy, a list of words to match against, and an optional list of words to exempt
/// from matching. Additionally, each table specifies the type of text processing to apply for both
/// regular and exemption word lists.
///
/// # Type Parameters
/// * `'a` - A lifetime parameter associated with the match table, indicating the lifespan of the
///   borrowed strings contained in the word lists.
///
/// # Fields
/// * `table_id` - A unique identifier for the specific matching table.
/// * `match_table_type` - The type of matching strategy (e.g., Simple, Regex, Similar) used by this table.
/// * `word_list` - A list of words to be used in the matching process.
/// * `exemption_process_type` - The type of text processing to apply to exemption words.
/// * `exemption_word_list` - A list of words that should be exempted from matching operations.
///
/// # Examples
///
/// ```rust
/// use matcher_rs::{MatchTable, MatchTableBuilder, MatchTableType, ProcessType};
///
/// // Recommended: Using MatchTableBuilder
/// let match_table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
///     .add_words(["example", "sample"])
///     .exemption_process_type(ProcessType::None)
///     .add_exemption_words(["ignore", "skip"])
///     .build();
///
/// // Or manually
/// let match_table_manual = MatchTable {
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
    fn word_list(&self) -> &[&'a str] {
        &self.word_list
    }
    fn exemption_process_type(&self) -> ProcessType {
        self.exemption_process_type
    }
    fn exemption_word_list(&self) -> &[&'a str] {
        &self.exemption_word_list
    }
}

/// A structure representing a serializable match table configuration.
///
/// This serves exactly the same role as [`MatchTable`] but internally owns its
/// text references using a copy-on-write `Cow<'a, str>` string format, making it
/// suitable for dynamic parsing pipelines where strings lack static lifetimes (e.g., from network JSON requests).
///
/// # Type Parameters
/// * `'a` - A lifetime parameter associated with the match table.
///
/// # Fields
/// * `table_id` - A unique identifier for the specific matching table.
/// * `match_table_type` - The specific matching strategy enum used by this rule block.
/// * `word_list` - A list of words to be used in the matching process.
/// * `exemption_process_type` - The preprocessing rules enforced on exemption words.
/// * `exemption_word_list` - A list of exemption (blocking) words stored as `Cow<'a, str>` references.
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
    fn word_list(&self) -> &[Cow<'a, str>] {
        &self.word_list
    }
    fn exemption_process_type(&self) -> ProcessType {
        self.exemption_process_type
    }
    fn exemption_word_list(&self) -> &[Cow<'a, str>] {
        &self.exemption_word_list
    }
}

/// A configuration structure representing a word table entry.
///
/// This structure is used to define the configuration for a specific word table entry,
/// including the match table ID, offset, and whether the entry is for exemption words.
///
/// # Fields
/// * `match_id` - A unique identifier for the match operation.
/// * `table_id` - A unique identifier for the specific matching table.
/// * `offset` - The position offset within the word table for this configuration entry.
/// * `is_exemption` - A flag indicating whether this configuration entry is for exemption words (true) or for regular matching words (false).
#[derive(Debug, Clone)]
struct WordTableConf {
    match_id: u32,
    table_id: u32,
    offset: u32,
    is_exemption: bool,
}

/// A structure representing the results of a matching operation.
///
/// The [`MatchResult`] struct contains detailed information about the results of a single matching
/// operation including the match identifier, table identifier, word identifier, the matched word
/// itself, and a similarity score.
///
/// # Type Parameters
/// * `'a` - The lifetime of the borrowed string contained in the `word` field.
///
/// # Fields
/// * `match_id` - A unique identifier for the match operation.
/// * `table_id` - A unique identifier for the specific matching table.
/// * `word_id` - A unique identifier for the word within the table.
/// * `word` - The word that was matched, using a [`Cow`] for efficiency.
/// * `similarity` - The optional similarity score of the matched word.
///
/// # Examples
///
/// ```rust
/// use std::borrow::Cow;
/// use matcher_rs::MatchResult;
///
/// let match_result = MatchResult {
///     match_id: 1,
///     table_id: 101,
///     word_id: 1001,
///     word: Cow::Borrowed("example"),
///     similarity: Some(0.95),
/// };
/// ```
#[derive(Serialize, Debug)]
pub struct MatchResult<'a> {
    pub match_id: u32,
    pub table_id: u32,
    pub word_id: u32,
    pub word: Cow<'a, str>,
    pub similarity: Option<f64>,
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
    fn similarity(&self) -> Option<f64> {
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
            similarity: Some(sim_result.similarity),
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
            similarity: None,
        }
    }
}

/// A type alias for a mapping from match table IDs to their corresponding [`MatchTable`]s.
///
/// This mapping uses a [`HashMap`] mapping to [`Vec<MatchTable>`].
///
/// # Type Parameters
/// * `'a` - The lifetime of the borrowed data within the [`MatchTable`] structures.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use matcher_rs::{MatchTable, MatchTableMap, MatchTableType, ProcessType, RegexMatchType};
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
/// let mut match_table_map: MatchTableMap = HashMap::new();
/// match_table_map.insert(1, vec![match_table_1]);
/// match_table_map.insert(2, vec![match_table_2]);
/// ```
pub type MatchTableMap<'a> = HashMap<u32, Vec<MatchTable<'a>>>;

/// A type alias for a mapping from match table IDs to their corresponding [`MatchTableSerde`] objects.
///
/// Semantically identical to [`MatchTableMap`], but accommodates dynamically parsed struct mappings built via `serde`.
///
/// # Type Parameters
/// * `'a` - The lifetime of the borrowed or owned data encapsulated within the string values.
pub type MatchTableMapSerde<'a> = HashMap<u32, Vec<MatchTableSerde<'a>>>;

/// The [`Matcher`] struct is responsible for managing and facilitating various types of matching operations
/// utilizing different word processing strategies and match table configurations.
///
/// # Algorithm
/// 1. Collects all `ProcessType` requirements from incoming `Regex`, `Simple`, and `Similar` tables.
/// 2. Compiles a unified `ProcessTypeBitNode` DAG (`process_type_tree`), deduplicating any overlapping sequence requirements across tables.
/// 3. Retains structured mappers (`simple_word_table_conf_list`, `simple_word_table_conf_index_list`) to correctly remap flat internal simple execution IDs back into externally valid table and match IDs.
/// 4. Dispatches text blocks natively to underlying `RegexMatcher`, `SimMatcher`, or `SimpleMatcher` engines.
///
/// # Fields
/// * `process_type_tree` - The compiled workflow tree ensuring text transforms happen exactly once per distinct branch sequence.
/// * `simple_word_table_conf_list` - A flattened catalog mapping aggregated simple string offset hits back into user-defined IDs.
/// * `simple_word_table_conf_index_list` - Provides O(1) projection translating raw substring bounds directly to table offsets.
/// * `simple_matcher` - Stores the core Aho-Corasick DAG optimized for exact sub-word overlapping.
/// * `regex_matcher` - Stores an optionally bundled fallback for advanced character logic regex operations.
/// * `sim_matcher` - Implements Rapidfuzz caching systems enabling near-instant threshold tolerance text evaluations.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
/// use matcher_rs::{Matcher, MatchTableMap, MatchTable, MatchTableType, ProcessType, RegexMatchType};
///
/// let match_table_1 = MatchTable {
///     table_id: 1,
///     match_table_type: MatchTableType::Simple { process_type: ProcessType::None },
///     word_list: vec!["word1", "word2"],
///     exemption_process_type: ProcessType::None,
///     exemption_word_list: vec!["ignore"],
/// };
///
/// let mut match_table_map: MatchTableMap = HashMap::new();
/// match_table_map.insert(1, vec![match_table_1]);
/// let matcher = Matcher::new(&match_table_map);
/// ```
#[derive(Debug, Clone)]
pub struct Matcher {
    process_type_tree: Box<[ProcessTypeBitNode]>,
    simple_word_table_conf_list: Box<[WordTableConf]>,
    simple_word_table_conf_index_list: Box<[usize]>,
    simple_matcher: Option<SimpleMatcher>,
    regex_matcher: Option<RegexMatcher>,
    sim_matcher: Option<SimMatcher>,
}

impl Matcher {
    /// Constructs a new [`Matcher`] instance from a given match table map.
    ///
    /// This method initializes the [`Matcher`] by processing the provided match table map and
    /// configuring various matching components (simple word tables, regex tables, and similarity
    /// tables) based on the match table configurations.
    ///
    /// Note: It is highly recommended to use [`MatcherBuilder`](crate::MatcherBuilder) to
    /// easily build a Matcher without manually instantiating `Vec` and `HashMap`s.
    ///
    /// # Type Parameters
    /// * `S` - The build hasher type for the `HashMap` (inferred).
    /// * `M` - The match table type that implements `MatchTableTrait<T>`.
    /// * `T` - String reference type that implements `AsRef<str>`.
    ///
    /// # Arguments
    /// * `match_table_map` - A reference to a [`HashMap`] linking `match_id` keys to a [`Vec`] of match tables.
    ///
    /// # Returns
    /// An initialized [`Matcher`].
    pub fn new<S, M, T>(match_table_map: &HashMap<u32, Vec<M>, S>) -> Matcher
    where
        M: MatchTableTrait<T>,
        T: AsRef<str>,
    {
        let mut process_type_set = HashSet::new();

        let mut simple_word_id = 0;
        let mut simple_word_table_conf_id = 0;
        let mut simple_word_table_conf_list = Vec::new();
        let mut simple_word_table_conf_index_list = Vec::new();
        let mut simple_table: SimpleTable = HashMap::new();

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
                            process_type_set.insert(process_type.bits());
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
                            process_type_set.insert(process_type.bits());
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
                            process_type_set.insert(process_type.bits());
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
                    process_type_set.insert(exemption_process_type.bits());
                    simple_word_table_conf_list.push(WordTableConf {
                        match_id,
                        table_id,
                        offset: simple_word_id,
                        is_exemption: true,
                    });

                    let simple_word_map = simple_table.entry(exemption_process_type).or_default();

                    for exemption_word in exemption_word_list {
                        simple_word_table_conf_index_list.push(simple_word_table_conf_id);
                        simple_word_map.insert(simple_word_id, exemption_word);
                        simple_word_id += 1;
                    }

                    simple_word_table_conf_id += 1
                }
            }
        }

        let process_type_tree = build_process_type_tree(&process_type_set).into_boxed_slice();

        Matcher {
            process_type_tree,
            simple_word_table_conf_list: simple_word_table_conf_list.into_boxed_slice(),
            simple_word_table_conf_index_list: simple_word_table_conf_index_list.into_boxed_slice(),
            simple_matcher: (!simple_table.is_empty()).then(|| SimpleMatcher::new(&simple_table)),
            regex_matcher: (!regex_table_list.is_empty())
                .then(|| RegexMatcher::new(&regex_table_list)),
            sim_matcher: (!sim_table_list.is_empty()).then(|| SimMatcher::new(&sim_table_list)),
        }
    }

    /// Matches words in the given text based on the configured match tables.
    ///
    /// This function processes the input text through various match tables
    /// configured in the [`Matcher`] instance. It handles simple word matches,
    /// regex matches, and similarity matches by checking against the processed
    /// text and returning the results in a `HashMap`.
    ///
    /// # Arguments
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    /// A [`HashMap`] where keys are match IDs and values are vectors of [`MatchResult`] items.
    /// If the input text is empty, an empty [`HashMap`] is returned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{MatchTableBuilder, MatchTableType, ProcessType, MatcherBuilder};
    ///
    /// let match_table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
    ///     .add_word("detect")
    ///     .build();
    ///
    /// let matcher = MatcherBuilder::new().add_table(1, match_table).build();
    ///
    /// let result = matcher.word_match("we should detect this");
    /// assert!(result.contains_key(&1));
    /// assert_eq!(result.get(&1).unwrap().len(), 1);
    /// ```
    pub fn word_match<'a>(&'a self, text: &'a str) -> HashMap<u32, Vec<MatchResult<'a>>> {
        if text.is_empty() {
            return HashMap::new();
        }

        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self._word_match_with_processed_text_process_type_masks(&processed_text_process_type_masks)
    }

    /// Matches processed text against the configured match tables.
    ///
    /// # Algorithm
    /// 1. Initializes an empty `match_result_dict` (`HashMap<u32, Vec<MatchResult>>`) and an exemption tracker `failed_match_table_id_set`.
    /// 2. Passes the pre-processed texts sequentially into the underlying `regex_matcher` and `sim_matcher`.
    /// 3. Collects their `RegexResult` and `SimResult` structs, upcasting them to `MatchResult` and pushing them to the result map payload.
    /// 4. Scans the `simple_matcher`. Simple matching uses an aggregated offset dictionary map (`simple_word_table_conf_list`).
    /// 5. For each simple match reported:
    ///    - If the simple match maps to an `is_exemption` configuration line: it inserts the parent `match_table_id` into the `failed_match_table_id_set`, aggressively scrubbing all previous sibling hits mapped from the same `table_id` and blocking future hits.
    ///    - If it's a standard simple match: checks if `failed_match_table_id_set` blocks this `match_table_id`. If not, it computes its actual distinct `word_id` by mapping `- word_table_conf.offset` and adds the match to the payload.
    ///
    /// # Arguments
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple contains a processed text variant (as [`Cow<'a, str>`]) and a `u64` bitmask of applicable process type IDs.
    ///
    /// # Returns
    /// * [`HashMap<u32, Vec<MatchResult>>`] - A map where keys are match IDs and
    ///   values are vectors of [`MatchResult`] items. Each [`MatchResult`] holds
    ///   information about a match found in the corresponding match table.
    ///   If no matches are found, the function returns an empty [`HashMap`].
    fn _word_match_with_processed_text_process_type_masks<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> HashMap<u32, Vec<MatchResult<'a>>> {
        let mut match_result_dict = HashMap::new();
        let mut failed_match_table_id_set = HashSet::new();

        if let Some(regex_matcher) = &self.regex_matcher {
            for regex_result in
                regex_matcher.process_preprocessed(processed_text_process_type_masks)
            {
                let result_list: &mut Vec<MatchResult> =
                    match_result_dict.entry(regex_result.match_id).or_default();

                result_list.push(regex_result.into());
            }
        }

        if let Some(sim_matcher) = &self.sim_matcher {
            for sim_result in sim_matcher.process_preprocessed(processed_text_process_type_masks) {
                let result_list = match_result_dict.entry(sim_result.match_id).or_default();

                result_list.push(sim_result.into());
            }
        }

        if let Some(simple_matcher) = &self.simple_matcher {
            for simple_result in
                simple_matcher.process_preprocessed(processed_text_process_type_masks)
            {
                let word_table_conf = self.simple_word_table_conf_list.get(
                    self.simple_word_table_conf_index_list[simple_result.word_id as usize],
                ).expect("simple_word_table_conf_index_list` is pre-populated guaranteeing index mapping corresponds directly to valid indices mapped within `simple_word_table_conf_list`.");
                let match_table_id = ((word_table_conf.match_id as usize) << 32)
                    | (word_table_conf.table_id as usize);

                if failed_match_table_id_set.contains(&match_table_id) {
                    continue;
                }

                let result_list = match_result_dict
                    .entry(word_table_conf.match_id)
                    .or_default();
                if word_table_conf.is_exemption {
                    failed_match_table_id_set.insert(match_table_id);
                    result_list
                        .retain(|match_result| match_result.table_id != word_table_conf.table_id);
                } else {
                    result_list.push(MatchResult {
                        match_id: word_table_conf.match_id,
                        table_id: word_table_conf.table_id,
                        word_id: simple_result.word_id - word_table_conf.offset,
                        word: simple_result.word,
                        similarity: None,
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
    /// defined for the [`Matcher`] instance and then checks if any matches
    /// are found using the underlying match tables (simple, regex, and
    /// similarity match tables).
    ///
    /// # Arguments
    /// * `text` - A string slice representing the input text to be processed and matched.
    ///
    /// # Returns
    /// * `bool` - Returns `true` if any matches are found, otherwise returns `false`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{MatchTableBuilder, MatchTableType, ProcessType, MatcherBuilder, TextMatcherTrait};
    ///
    /// let match_table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
    ///     .add_word("detect")
    ///     .build();
    ///
    /// let matcher = MatcherBuilder::new().add_table(1, match_table).build();
    ///
    /// assert!(matcher.is_match("we should detect this"));
    /// assert!(!matcher.is_match("clean text"));
    /// ```
    fn is_match(&self, text: &str) -> bool {
        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.is_match_preprocessed(&processed_text_process_type_masks)
    }
    /// Processes the input text to generate a list of match results.
    ///
    /// This function takes an input text string, processes it according to the
    /// [`Matcher`] instance's configured process type tree, and then generates a
    /// list of match results by applying the processed text against the configured
    /// match tables.
    ///
    /// The process involves reducing the input text based on the type tree, transforming
    /// it into a structured format (`processed_text_process_type_masks`) suitable for
    /// matching operations. The results are then aggregated into a single list of
    /// [`MatchResult`] instances.
    ///
    /// # Arguments
    ///
    /// * `text` - A reference to the input text string to be processed.
    ///
    /// # Returns
    ///
    /// * [`Vec<MatchResult<'a>>`] - A vector containing match results corresponding to
    ///   the patterns defined in the match tables.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{MatchTableBuilder, MatchTableType, ProcessType, MatcherBuilder, TextMatcherTrait};
    ///
    /// let match_table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
    ///     .add_words(["apple", "banana"])
    ///     .build();
    ///
    /// let matcher = MatcherBuilder::new().add_table(1, match_table).build();
    ///
    /// let results = matcher.process("I have an apple and a banana");
    /// assert_eq!(results.len(), 2);
    /// ```
    fn process(&'a self, text: &'a str) -> Vec<MatchResult<'a>> {
        let processed_text_process_type_masks =
            reduce_text_process_with_tree(&self.process_type_tree, text);

        self.process_preprocessed(&processed_text_process_type_masks)
    }

    /// Processes the given text and returns a **lazy** iterator over [`MatchResult`] matches.
    ///
    /// # Design note — why the word-match map is still eager
    ///
    /// The [`Matcher`] applies **exemption logic**: a simple-matcher hit on an exemption word for a
    /// given `(match_id, table_id)` pair must *retroactively remove* previously accumulated
    /// results for that pair. This is implemented via `_word_match_with_processed_text_process_type_masks`,
    /// which returns a [`HashMap<u32, Vec<MatchResult>>`] only after processing the entire input.
    ///
    /// Because all results must be seen before any can be safely emitted (an exemption hit in the
    /// middle of the input would invalidate earlier simple-match results), the aggregation into the
    /// [`HashMap`] must remain eager.
    ///
    /// The benefit over calling [`TextMatcherTrait::process`] is that the final `collect()` step is avoided: results
    /// are yielded lazily to the caller as it advances the iterator. Callers that short-circuit
    /// (e.g., looking for the *first* result satisfying some predicate) pay no allocation cost for
    /// the results they never consume.
    ///
    /// # Arguments
    ///
    /// * `text` - A reference to the input text string to be processed.
    ///
    /// # Returns
    ///
    /// * An `impl Iterator<Item = MatchResult<'a>>` — a lazy iterator of match results.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use matcher_rs::{MatchTableBuilder, MatchTableType, ProcessType, MatcherBuilder, TextMatcherTrait};
    ///
    /// let match_table = MatchTableBuilder::new(1, MatchTableType::Simple { process_type: ProcessType::None })
    ///     .add_word("find")
    ///     .build();
    ///
    /// let matcher = MatcherBuilder::new().add_table(1, match_table).build();
    ///
    /// let mut iter = matcher.process_iter("find me");
    /// assert!(iter.next().is_some());
    /// assert!(iter.next().is_none());
    /// ```
    fn process_iter(&'a self, text: &'a str) -> impl Iterator<Item = MatchResult<'a>> + 'a {
        gen move {
            if text.is_empty() {
                return;
            }

            let processed_text_process_type_masks =
                reduce_text_process_with_tree(&self.process_type_tree, text);

            let matches = self._word_match_with_processed_text_process_type_masks(
                &processed_text_process_type_masks,
            );
            for value_list in matches.into_values() {
                for match_result in value_list {
                    yield match_result;
                }
            }
        }
    }
}

impl<'a> TextMatcherInternal<'a, MatchResult<'a>> for Matcher {
    /// Checks if there are any matches for the processed text within the configured match tables.
    ///
    /// This function takes a reference to a processed text set and determines if any matches
    /// exist within the match tables of the [`Matcher`] instance. The function prioritizes
    /// checking the simple matcher first. If the simple matcher is not configured or
    /// doesn't find any matches, it proceeds to check the regex matcher and then the
    /// similarity matcher, in that order.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple
    ///   contains a processed text piece (as [`Cow<str>`]) and a
    ///   u64 bitmask of process type IDs (`u64`).
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if any matches are found within any of the matchers, otherwise `false`.
    ///
    /// # Safety
    ///
    /// This function is safe to use under normal circumstances but depends on the reliability
    /// of the underlying matchers and the integrity of the `processed_text_process_type_masks`
    /// input. Ensure the input data is correctly processed and the matchers are properly
    /// initialized before calling this function.
    fn is_match_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> bool {
        if self.simple_matcher.is_some() {
            return !self
                ._word_match_with_processed_text_process_type_masks(
                    processed_text_process_type_masks,
                )
                .is_empty();
        }
        if let Some(regex_matcher) = &self.regex_matcher
            && regex_matcher.is_match_preprocessed(processed_text_process_type_masks)
        {
            return true;
        }
        if let Some(sim_matcher) = &self.sim_matcher
            && sim_matcher.is_match_preprocessed(processed_text_process_type_masks)
        {
            return true;
        }
        false
    }

    /// Aggregates match results by processing the pre-processed text with the configured matchers.
    ///
    /// This function takes a reference to a pre-processed text set (a list of tuples containing
    /// processed text and associated [`HashSet`]) and generates match results using the instance's
    /// configured matchers. The function focuses on word-level matching and aggregates the
    /// results into a single list of [`MatchResult`] instances.
    ///
    /// The process involves invoking the appropriate matcher to obtain match results for the
    /// provided pre-processed text and then flattening the results into a single vector.
    ///
    /// # Arguments
    ///
    /// * `processed_text_process_type_masks` - A reference to a slice of tuples, where each tuple
    ///   contains a processed text piece (as [`Cow<str>`]) and a
    ///   u64 bitmask of process type IDs (`u64`).
    ///
    /// # Returns
    ///
    /// * [`Vec<MatchResult<'a>>`] - A vector containing aggregated match results generated
    ///   from the match IDs.
    fn process_preprocessed(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
    ) -> Vec<MatchResult<'a>> {
        self._word_match_with_processed_text_process_type_masks(processed_text_process_type_masks)
            .into_values()
            .flatten()
            .collect()
    }
}
