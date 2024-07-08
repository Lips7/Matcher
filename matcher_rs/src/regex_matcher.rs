use std::borrow::Cow;

use fancy_regex::{escape, Regex};
use regex::RegexSet;
use sonic_rs::{Deserialize, Serialize};

use crate::matcher::{MatchResultTrait, TextMatcherTrait};
#[cfg(feature = "serde")]
use crate::util::serde::{serde_regex, serde_regex_list, serde_regex_set};

/// Enumeration representing different types of regex match algorithms used in text matching.
///
/// The [RegexMatchType] enum provides a way to distinguish between various match algorithms
/// that can be applied during regex pattern matching. Each variant defines a specific matching
/// strategy, allowing for flexible and tailored text matching operations.
///
/// # Variants
///
/// * [SimilarChar](RegexMatchType::Regex) - Represents a matching strategy that identifies matches based on character similarity. This type of matching is useful for finding text that is similar in character composition but not necessarily identical.
/// * [Acrostic](RegexMatchType::Acrostic) - Represents a matching strategy that identifies acrostic matches, where the matching portion of the text forms an acrostic pattern. This type of matching is particularly useful for specific types of literary analysis or word games.
/// * [Regex](RegexMatchType::Regex) - Represents a standard regex matching strategy, utilizing regular expressions to identify precise patterns within the text. This type of matching is widely used for its flexibility and power in text processing.
///
/// This enum is used within various text matching applications to specify the match type to be applied,
/// enabling the application to execute the appropriate algorithm for the desired matching criteria.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RegexMatchType {
    SimilarChar,
    Acrostic,
    Regex,
}

/// Represents a table containing regex patterns and their associated metadata for text matching operations.
///
/// The [RegexTable] struct is designed to encapsulate a collection of regex patterns along with relevant
/// identifiers and match type information. This structure is utilized in regex-based text matching processes
/// to organize and manage various sets of regex patterns efficiently.
///
/// # Fields
///
/// * `table_id` - A unique identifier for the regex table. This field is used to distinguish between different regex tables.
/// * `match_id` - An identifier that corresponds to the specific match operation associated with this regex table. This helps in tracking and categorizing match results.
/// * `regex_match_type` - The type of regex match algorithm being used, represented by the [RegexMatchType] enum. This field defines the matching strategy applied by the regex patterns in the table.
/// * `word_list` - A reference to a vector of string slices (`&'a Vec<&'a str>`) that represents the list of words or patterns that the regex in this table aims to match against. This collection allows the regex operations to process and match text efficiently.
///
/// # Example
///
/// ```rust
/// use matcher_rs::{RegexTable, RegexMatchType};
///
/// let word_list = vec!["example", "test", "sample"];
/// let regex_table = RegexTable {
///     table_id: 1,
///     match_id: 42,
///     regex_match_type: RegexMatchType::Regex,
///     word_list: &word_list,
/// };
///
/// println!("{:?}", regex_table);
/// ```
///
/// The example above demonstrates how to create a [RegexTable] instance, populate it with a list of words,
/// and print the structure for debugging or logging purposes.
///
/// This struct is primarily used in advanced text matching applications, where the organization and efficient
/// management of regex patterns are crucial for the performance and accuracy of the matching process.
#[derive(Debug, Clone)]
pub struct RegexTable<'a> {
    pub table_id: u32,
    pub match_id: u32,
    pub regex_match_type: RegexMatchType,
    pub word_list: &'a Vec<&'a str>,
}

/// Enum representing different types of regex pattern tables used in the [RegexMatcher].
///
/// The `RegexType` enum is utilized within `RegexPatternTable` to define the structure and behavior of the regex
/// patterns stored in each table. It supports two types of regex patterns: `Standard` and `List`.
///
/// # Variants
///
/// * `Standard` - Represents a table that holds a single compiled regex pattern.
///   - `regex` ([Regex]): The compiled regex pattern used for matching text.
///
/// * `List` - Represents a table that holds a list of compiled regex patterns and their corresponding words.
///   - `regex_list` ([`Vec<Regex>`]): A list of compiled regex patterns used for matching text.
///   - `word_list` ([`Vec<String>`]): A list of words corresponding to each regex pattern in `regex_list`.
///
/// * `Set` - Represents a table that holds a set of compiled regex patterns.
///   - `regex_set` ([RegexSet]): A regex set of compiled regex patterns used for matching text.
///   - `word_list` ([`Vec<String>`]): A list of words corresponding to each regex pattern in `regex_list`.
///
/// # Usage
///
/// This enum enables the [RegexMatcher] to distinguish between tables that use a singular regex pattern and those
/// that use multiple patterns stored in a list. The associated data for each variant ensures that the [RegexMatcher]
/// can accurately process match operations and return results based on the specific table type.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
enum RegexType {
    Standard {
        #[cfg_attr(feature = "serde", serde(with = "serde_regex"))]
        regex: Regex,
    },
    List {
        #[cfg_attr(feature = "serde", serde(with = "serde_regex_list"))]
        regex_list: Vec<Regex>,
        word_list: Vec<String>,
    },
    Set {
        #[cfg_attr(feature = "serde", serde(with = "serde_regex_set"))]
        regex_set: RegexSet,
        word_list: Vec<String>,
    },
}

/// A structure representing a table of regex patterns used for text matching.
///
/// The `RegexPatternTable` struct is designed to hold compiled regex patterns and associated metadata,
/// allowing the [RegexMatcher] to efficiently organize and manage different sets of patterns for matching
/// text. Each `RegexPatternTable` instance corresponds to a specific regex table and contains details
/// such as a unique identifier, match identifier, and the type of regex patterns stored.
///
/// # Fields
///
/// * `table_id` - A unique identifier for the regex pattern table. This identifier distinguishes the table from other regex tables.
/// * `match_id` - A unique identifier for the match, which corresponds to the `match_id` of the [RegexTable] that contains the regex pattern.
/// * `regex_type` - The type of regex pattern table, represented by the `RegexType` enum. This field determines the structure and behavior of the regex patterns stored in the table.
///
/// The `RegexPatternTable` struct is utilized internally by the [RegexMatcher] to categorize and execute regex-based text matching operations.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
struct RegexPatternTable {
    table_id: u32,
    match_id: u32,
    regex_type: RegexType,
}

/// Represents a result from a regex matching operation, containing metadata about the match.
///
/// The `RegexResult` structure is designed to encapsulate information about a particular regex match,
/// including the matched word or pattern, the table identifier from which the match originated, and
/// the match identifier associated with the match.
///
/// # Fields
///
/// * `match_id` - A [u32] that serves as an identifier for the match. This identifier
///   is used to differentiate between match results originating from different regex tables, allowing
///   for more detailed and organized match results.
///
/// * `table_id` - A [u32] representing the unique identifier of the regex table that produced the match result.
///   This helps in distinguishing which regex table contributed to the result, facilitating organized processing
///   and categorization of matches.
///
/// * `word` - A [Cow<'a, str>] that holds the matched word or pattern. This field can either be a
///   borrowed string slice or an owned [String], offering flexibility in how the match result is stored.
///
/// This structure is primarily utilized in text matching applications where regex patterns are used
/// to identify specific words or patterns within the target text, and the results need to be tracked
/// and processed accordingly.
#[derive(Debug, Clone)]
pub struct RegexResult<'a> {
    pub match_id: u32,
    pub table_id: u32,
    pub word: Cow<'a, str>,
}

impl MatchResultTrait<'_> for RegexResult<'_> {
    fn table_id(&self) -> u32 {
        self.table_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

/// A structure responsible for managing and handling regex pattern tables for text matching.
///
/// The [RegexMatcher] stores a list of `RegexPatternTable` structures, each of which contains
/// regex patterns and associated metadata used for efficient text matching operations. The struct
/// provides methods to create a new instance from a list of [RegexTable] structures, as well as
/// to check for matches and process the text to produce a list of match results.
///
/// # Fields
///
/// * `regex_pattern_table_list` - A vector of `RegexPatternTable` structures that hold regex patterns
///   and associated metadata for text matching.
///
/// # Usage
///
/// This structure is used within the [RegexMatcher] to efficiently manage multiple regex patterns
/// and their corresponding match tables. It enables the [RegexMatcher] to perform text matching
/// operations and return results based on the provided regex tables.
///
/// # Example
///
/// ```
/// use matcher_rs::{RegexMatcher, RegexTable, RegexMatchType, TextMatcherTrait};
///
/// let regex_table = RegexTable {
///     table_id: 1,
///     match_id: 1,
///     regex_match_type: RegexMatchType::SimilarChar,
///     word_list: &vec!["1,一", "2,二"],
/// };
///
/// let regex_matcher = RegexMatcher::new(&vec![regex_table]);
/// assert!(regex_matcher.is_match("12"));
/// assert!(regex_matcher.is_match("一2"));
/// assert!(regex_matcher.is_match("1二"));
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RegexMatcher {
    regex_pattern_table_list: Vec<RegexPatternTable>,
}

impl RegexMatcher {
    /// Creates a new [RegexMatcher] instance from a list of [RegexTable].
    ///
    /// This constructor function initializes a [RegexMatcher] with the provided list of [RegexTable] instances.
    /// Each [RegexTable] contains regex patterns and other metadata. The function processes these tables and
    /// compiles the regex patterns into `RegexPatternTable` structures, which are then stored in the `regex_pattern_table_list`.
    ///
    /// # Arguments
    ///
    /// * `regex_table_list` - A slice of [RegexTable] instances to be used for initializing the [RegexMatcher].
    ///
    /// # Returns
    ///
    /// * [RegexMatcher] - A new instance of [RegexMatcher] containing compiled regex patterns and associated metadata.
    ///
    /// # Processing
    ///
    /// The function handles different `RegexMatchType` variants within the [RegexTable]:
    ///
    /// * [SimilarChar](RegexMatchType::SimilarChar) - Constructs a regex pattern where each character in the word list is separated by an optional dot (`.?`).
    ///   This pattern is then compiled into a single regex and stored in a `RegexPatternTable` with `RegexType::Standard`.
    ///
    /// * [Acrostic](RegexMatchType::Acrostic) - Creates regex patterns that match words starting from the beginning or after any punctuation or whitespace.
    ///   These patterns are compiled into individual regexes and stored in a `RegexPatternTable` with either `RegexType::List`
    ///   or `RegexType::Set`, depending on whether a `RegexSet` can be successfully created.
    ///
    /// * [Regex](RegexMatchType::Regex) - Compiles each word in the word list into individual regexes and stores them in a `RegexPatternTable` with either
    ///   `RegexType::List` or `RegexType::Set`, similar to the `Acrostic` type.
    ///
    /// Any invalid regex patterns encountered during the creation process are ignored, and a warning message is printed to the console.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::{RegexMatcher, RegexTable, RegexMatchType, TextMatcherTrait};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     regex_match_type: RegexMatchType::SimilarChar,
    ///     word_list: &vec!["1,一", "2,二"],
    /// };
    ///
    /// let regex_matcher = RegexMatcher::new(&vec![regex_table]);
    ///
    /// assert!(regex_matcher.is_match("12"));
    /// assert!(regex_matcher.is_match("一2"));
    /// assert!(regex_matcher.is_match("1二"));
    /// ```
    pub fn new(regex_table_list: &[RegexTable]) -> RegexMatcher {
        let mut regex_pattern_table_list = Vec::with_capacity(regex_table_list.len());

        for regex_table in regex_table_list {
            let size = regex_table.word_list.len();

            match regex_table.regex_match_type {
                RegexMatchType::SimilarChar => {
                    let pattern = regex_table
                        .word_list
                        .iter()
                        .map(|charstr| format!("({})", escape(charstr).replace(',', "|")))
                        .collect::<Vec<String>>()
                        .join(".?");

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        regex_type: RegexType::Standard {
                            regex: Regex::new(&pattern).unwrap(),
                        },
                    });
                }
                RegexMatchType::Acrostic => {
                    let mut word_list = Vec::with_capacity(size);
                    let mut regex_list = Vec::with_capacity(size);
                    let mut pattern_list = Vec::with_capacity(size);

                    for &word in regex_table.word_list.iter() {
                        let pattern = format!(
                            r"(?i)(?:^|[\s\pP]+?){}",
                            escape(word).replace(',', r".*?[\s\pP]+?")
                        );
                        match Regex::new(&pattern) {
                            Ok(regex) => {
                                regex_list.push(regex);
                                word_list.push(word.to_owned());
                                pattern_list.push(pattern);
                            }
                            Err(e) => {
                                println!("Acrostic word {word} is illegal, ignored. Error: {e}");
                            }
                        }
                    }

                    let regex_type = RegexSet::new(pattern_list).map_or(
                        RegexType::List {
                            regex_list,
                            word_list: word_list.clone(),
                        },
                        |regex_set| RegexType::Set {
                            regex_set,
                            word_list,
                        },
                    );

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        regex_type,
                    });
                }
                RegexMatchType::Regex => {
                    let mut word_list = Vec::with_capacity(size);
                    let mut regex_list = Vec::with_capacity(size);

                    for &word in regex_table.word_list.iter() {
                        match Regex::new(word) {
                            Ok(regex) => {
                                regex_list.push(regex);
                                word_list.push(word.to_owned());
                            }
                            Err(e) => {
                                println!("Regex word {word} is illegal, ignored. Error: {e}");
                            }
                        }
                    }

                    let regex_type = RegexSet::new(&word_list).map_or(
                        RegexType::List {
                            regex_list,
                            word_list: word_list.clone(),
                        },
                        |regex_set| RegexType::Set {
                            regex_set,
                            word_list,
                        },
                    );

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        regex_type,
                    });
                }
            };
        }

        RegexMatcher {
            regex_pattern_table_list,
        }
    }
}

impl<'a> TextMatcherTrait<'a, RegexResult<'a>> for RegexMatcher {
    /// Determines if the provided text matches any of the regex patterns stored in the match tables.
    ///
    /// This function iterates through all the `RegexPatternTable` instances in `regex_pattern_table_list`
    /// and checks if the provided text matches any of the regex patterns based on the `RegexType` of each table.
    ///
    /// # Arguments
    ///
    /// * `self` - A reference to the [RegexMatcher] instance.
    /// * `text` - A string slice (`&str`) containing the text to be checked for matches against the regex patterns.
    ///
    /// # Returns
    ///
    /// * `bool` - Returns `true` if the text matches any of the regex patterns, otherwise returns `false`.
    ///
    /// # Match Checking
    ///
    /// The function handles different `RegexType` variants within the `RegexPatternTable`:
    ///
    /// * `Standard` - Checks if the text matches the single compiled regex pattern stored in the table.
    ///   If a match is found, the function returns `true`.
    ///
    /// * `List` - Iterates through the list of compiled regex patterns and checks if the text matches
    ///   any of them. If a match is found, the function returns `true`.
    ///
    /// * `Set` - Checks if the text matches the single compiled regex pattern stored in the table.
    ///   If a match is found, the function returns `true`.
    ///
    /// If no matches are found after checking all regex patterns in all tables, the function returns `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::{RegexMatcher, RegexTable, RegexMatchType, TextMatcherTrait};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     regex_match_type: RegexMatchType::SimilarChar,
    ///     word_list: &vec!["1,一", "2,二"],
    /// };
    ///
    /// let regex_matcher = RegexMatcher::new(&vec![regex_table]);
    ///
    /// assert!(regex_matcher.is_match("12"));
    /// assert!(regex_matcher.is_match("一2"));
    /// assert!(regex_matcher.is_match("1二"));
    /// ```
    fn is_match(&self, text: &str) -> bool {
        for regex_table in &self.regex_pattern_table_list {
            match &regex_table.regex_type {
                RegexType::Standard { regex } => {
                    if regex.is_match(text).unwrap() {
                        return true;
                    }
                }
                RegexType::List { regex_list, .. } => {
                    if regex_list.iter().any(|regex| regex.is_match(text).unwrap()) {
                        return true;
                    }
                }
                RegexType::Set { regex_set, .. } => {
                    if regex_set.is_match(text) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Processes the provided text and returns a list of regex match results.
    ///
    /// This function iterates through all the `RegexPatternTable` instances in `regex_pattern_table_list`
    /// and searches for matches within the provided text based on the `RegexType` of each table.
    ///
    /// # Arguments
    ///
    /// * `&'a self` - A reference to the [RegexMatcher] instance with a defined lifetime `'a`.
    /// * `text` - A string slice (`&str`) containing the text to be checked for regex matches.
    ///
    /// # Returns
    ///
    /// * `Vec<RegexResult<'a>>` - A vector containing the results of regex matches. Each result includes
    ///   the matched word, table ID, and match ID.
    ///
    /// # Match Processing
    ///
    /// The function handles different `RegexType` variants within the `RegexPatternTable`:
    ///
    /// * `Standard` - Iterates through the captures of the regex for the given text. For each capture
    ///   group (excluding the entire match), it collects the matched substrings, concatenates them, and
    ///   stores the result.
    ///
    /// * `List` - Iterates through the list of compiled regex patterns. If the text matches any regex,
    ///   it pushes the associated word from `word_list` and the table/match IDs to the result list.
    ///
    /// * `Set` - Retrieves the patterns from the regex set. For each matched pattern index, it pushes
    ///   the corresponding pattern and the table/match IDs to the result list.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::{RegexMatcher, RegexTable, RegexMatchType, TextMatcherTrait};
    /// use std::borrow::Cow;
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     regex_match_type: RegexMatchType::SimilarChar,
    ///     word_list: &vec!["1,一", "2,二"],
    /// };
    ///
    /// let regex_matcher = RegexMatcher::new(&vec![regex_table]);
    ///
    /// let results = regex_matcher.process("12");
    /// for result in results {
    ///     println!("Matched word: {}", result.word);
    ///     println!("Table ID: {}", result.table_id);
    ///     println!("Match ID: {}", result.match_id);
    /// }
    /// ```
    fn process(&'a self, text: &str) -> Vec<RegexResult<'a>> {
        let mut result_list = Vec::new();

        for regex_table in &self.regex_pattern_table_list {
            match &regex_table.regex_type {
                RegexType::Standard { regex } => {
                    result_list.extend(regex.captures_iter(text).map(|caps| {
                        RegexResult {
                            match_id: regex_table.match_id,
                            table_id: regex_table.table_id,
                            word: Cow::Owned(
                                caps.unwrap()
                                    .iter()
                                    .skip(1)
                                    .filter_map(|m| m.map(|match_char| match_char.as_str()))
                                    .collect::<String>(),
                            ),
                        }
                    }))
                }
                RegexType::List {
                    regex_list,
                    word_list,
                } => result_list.extend(regex_list.iter().enumerate().filter_map(
                    |(index, regex)| {
                        regex.is_match(text).unwrap().then_some(RegexResult {
                            match_id: regex_table.match_id,
                            table_id: regex_table.table_id,
                            word: Cow::Borrowed(&word_list[index]),
                        })
                    },
                )),
                RegexType::Set {
                    regex_set,
                    word_list,
                } => result_list.extend(regex_set.matches(text).into_iter().map(|index| {
                    RegexResult {
                        match_id: regex_table.match_id,
                        table_id: regex_table.table_id,
                        word: Cow::Borrowed(&word_list[index]),
                    }
                })),
            }
        }

        result_list
    }
}
