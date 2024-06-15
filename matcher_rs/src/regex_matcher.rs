use std::borrow::Cow;

use fancy_regex::{escape, Regex};

use super::{MatchResultTrait, MatchTableType, TextMatcherTrait};

/// A structure representing a table of regex patterns used for matching text.
///
/// # Fields
///
/// * `table_id` - A unique identifier for the table.
/// * `match_id` - A string identifier for the match.
/// * `match_table_type` - The type of match table, which determines how the patterns are interpreted.
/// * `word_list` - A list of words or patterns associated with this table.
///
/// # Examples
///
/// ```
/// use matcher_rs::{RegexTable, MatchTableType};
///
/// let regex_table = RegexTable {
///     table_id: 1,
///     match_id: "example_match",
///     match_table_type: MatchTableType::SimilarChar,
///     word_list: vec!["word1", "word2"],
/// };
/// ```
///
/// This structure is used in the `RegexMatcher` to organize and categorize different sets
/// of regex patterns for efficient text matching.
pub struct RegexTable<'a> {
    pub table_id: u64,
    pub match_id: &'a str,
    pub match_table_type: MatchTableType,
    pub word_list: Vec<&'a str>,
}

/// An enumeration representing different types of regular expressions used for matching.
///
/// # Variants
///
/// * `StandardRegex` - This variant holds a single `regex` field of type `Regex`,
///   which is used to match text based on a single regular expression pattern.
///   - `regex`: A compiled regular expression pattern used for text matching.
///
/// * `ListRegex` - This variant holds two fields: `regex_list` and `word_list`.
///   `regex_list` is a vector of compiled regular expressions, and `word_list`
///   is a vector of corresponding string patterns.
///   - `regex_list`: A vector of compiled regular expression patterns for text matching.
///   - `word_list`: A vector of string patterns associated with the regular expressions.
///
/// This enumeration helps categorize the regex matching strategies supported by the `RegexMatcher`:
/// - `StandardRegex` for single pattern matching.
/// - `ListRegex` for matching against a list of patterns.
enum RegexType {
    StandardRegex {
        regex: Regex,
    },
    ListRegex {
        regex_list: Vec<Regex>,
        word_list: Vec<String>,
    },
}

/// A structure representing a table that holds regex patterns for matching.
///
/// # Fields
///
/// * `table_id` - A unique identifier for the table.
/// * `match_id` - A string identifier for the match.
/// * `table_match_type` - The type of regex pattern matching used in the table, which can be either `StandardRegex` with a single regex or `ListRegex` with a list of regex patterns.
///
/// This structure is used in the `RegexMatcher` to store different sets of regex patterns along with their identifiers.
struct RegexPatternTable {
    table_id: u64,
    match_id: String,
    table_match_type: RegexType,
}

/// A structure representing the result of a regex match.
///
/// # Fields
///
/// * `word` - The matched word or pattern, which can be either an owned `String`
///            or a borrowed string slice (`&str`). It uses `Cow` (Clone on Write)
///            to efficiently handle both owned and borrowed data.
/// * `table_id` - A unique identifier for the regex table that produced this match result.
/// * `match_id` - A string identifier for the match, which corresponds to the `match_id`
///                of the `RegexTable` that contains the regex pattern.
///
/// # Debug
/// The structure derives `Debug` for easier debugging and logging of match results.
///
/// This structure is returned by the `RegexMatcher` when processing text, and it contains
/// all the necessary information to identify which regex table and pattern matched a
/// particular piece of text.
///
/// # Examples
///
/// ```
/// use std::borrow::Cow;
/// use matcher_rs::RegexResult;
///
/// let result = RegexResult {
///     word: Cow::Borrowed("example"),
///     table_id: 1,
///     match_id: "example_match",
/// };
///
/// println!("{:?}", result);
/// ```
///
#[derive(Debug)]
pub struct RegexResult<'a> {
    pub word: Cow<'a, str>,
    pub table_id: u64,
    pub match_id: &'a str,
}

impl MatchResultTrait<'_> for RegexResult<'_> {
    /// Provides the implementation for the `MatchResultTrait` trait for the `RegexResult` struct.
    ///
    /// This implementation allows `RegexResult` to expose methods defined by the `MatchResultTrait` trait.
    ///
    /// # Methods
    ///
    /// * `table_id(&self) -> u64` - Returns the unique identifier for the regex table that produced this match result.
    /// * `word(&self) -> &str` - Returns the matched word or pattern as a string slice.
    ///
    /// # Examples
    ///
    /// Suppose we have a `RegexResult` instance and we want to get its `table_id` and matched `word`:
    ///
    /// ```
    /// use matcher_rs::{RegexResult, MatchResultTrait};
    /// use std::borrow::Cow;
    ///
    /// let result = RegexResult {
    ///     word: Cow::Borrowed("example"),
    ///     table_id: 1,
    ///     match_id: "example_match",
    /// };
    ///
    /// assert_eq!(result.table_id(), 1);
    /// assert_eq!(result.word(), "example");
    /// ```
    fn table_id(&self) -> u64 {
        self.table_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

/// A structure responsible for managing and handling regex pattern tables for text matching.
///
/// The `RegexMatcher` stores a list of `RegexPatternTable` structures, each of which contains
/// regex patterns and associated metadata used for efficient text matching operations. The struct
/// provides methods to create a new instance from a list of `RegexTable` structures, as well as
/// to check for matches and process the text to produce a list of match results.
///
/// # Fields
///
/// * `regex_pattern_table_list` - A vector of `RegexPatternTable` structures that hold regex patterns
///   and associated metadata for text matching.
///
/// # Usage
///
/// This structure is used within the `RegexMatcher` to efficiently manage multiple regex patterns
/// and their corresponding match tables. It enables the `RegexMatcher` to perform text matching
/// operations and return results based on the provided regex tables.
///
/// # Example
///
/// ```
/// use matcher_rs::{RegexMatcher, RegexTable, MatchTableType, TextMatcherTrait};
///
/// let regex_table = RegexTable {
///     table_id: 1,
///     match_id: "example_match",
///     match_table_type: MatchTableType::SimilarChar,
///     word_list: vec!["1,一", "2,二"],
/// };
///
/// let regex_matcher = RegexMatcher::new(&vec![regex_table]);
/// assert!(regex_matcher.is_match("12"));
/// assert!(regex_matcher.is_match("一2"));
/// assert!(regex_matcher.is_match("1二"));
/// ```
///
pub struct RegexMatcher {
    regex_pattern_table_list: Vec<RegexPatternTable>,
}

impl RegexMatcher {
    /// Creates a new `RegexMatcher` instance from a list of `RegexTable` structures.
    ///
    /// This function takes a reference to a vector of `RegexTable` and converts it into a `RegexMatcher`
    /// instance. It processes each `RegexTable` in the provided list and constructs corresponding
    /// `RegexPatternTable` structures based on the `match_table_type`.
    ///
    /// # Arguments
    ///
    /// * `regex_table_list` - A reference to a vector of `RegexTable` instances. Each `RegexTable`
    ///   contains a unique `table_id`, `match_id`, a match table type, and a list of words or patterns.
    ///
    /// # Returns
    ///
    /// A `RegexMatcher` instance containing the converted list of `RegexPatternTable` structures,
    /// ready to be used for text matching operations.
    ///
    /// # Match Table Types
    ///
    /// The function handles different types of match tables (`MatchTableType`):
    ///
    /// * `SimilarChar` - Converts the words list into a single regex pattern, where each word is
    ///   escaped and separated by `|`. The combined pattern is joined using `.?.`.
    ///
    /// * `Acrostic` - Creates individual regex patterns for each word, where words are separated by
    ///   `".*?[\s\pP]+?"`. The patterns are constructed to match acrostic patterns in the text.
    ///
    /// * `Regex` - Treats each word in the list as a full regex pattern. It creates a list of regex
    ///   patterns corresponding to the words.
    ///
    /// # Panics
    ///
    /// The function uses `unwrap()` to handle the result of regex creation, which may panic if
    /// any of the provided patterns are invalid regular expressions.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::{RegexMatcher, RegexTable, MatchTableType};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: "example_match",
    ///     match_table_type: MatchTableType::SimilarChar,
    ///     word_list: vec!["1,一", "2,二"],
    /// };
    ///
    /// let regex_matcher = RegexMatcher::new(&vec![regex_table]);
    /// ```
    pub fn new(regex_table_list: &Vec<RegexTable>) -> RegexMatcher {
        // Create an empty vector with pre-allocated capacity to hold regex pattern tables
        let mut regex_pattern_table_list = Vec::with_capacity(regex_table_list.len());

        // Iterate through each regex table provided in the input list
        for regex_table in regex_table_list {
            // Get the number of words/patterns in the current regex table
            let size = regex_table.word_list.len();

            // Match on the type of match table to determine how to process its patterns
            match regex_table.match_table_type {
                // Handle the SimilarChar match table type
                MatchTableType::SimilarChar => {
                    // Create a combined regex pattern by escaping each word and joining them with .? separator
                    let pattern = regex_table
                        .word_list
                        .iter()
                        .map(|charstr| format!("({})", escape(charstr).replace(',', "|")))
                        .collect::<Vec<String>>()
                        .join(".?");

                    // Add a new RegexPatternTable entry for StandardRegex type with the compiled regex pattern
                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id.to_owned(),
                        table_match_type: RegexType::StandardRegex {
                            regex: Regex::new(&pattern).unwrap(),
                        },
                    });
                }
                // Handle the Acrostic match table type
                MatchTableType::Acrostic => {
                    // Create vectors to hold word list and regex list with pre-allocated capacity
                    let mut word_list = Vec::with_capacity(size);
                    let mut regex_list = Vec::with_capacity(size);

                    // Iterate through each word in the word list and create corresponding regex patterns
                    for &word in regex_table.word_list.iter() {
                        let pattern = format!(
                            r"(?i)(?:^|[\s\pP]+?){}",
                            escape(word).replace(',', r".*?[\s\pP]+?")
                        );

                        // Add the current word and its corresponding regex pattern to the lists
                        word_list.push(word.to_owned());
                        regex_list.push(Regex::new(&pattern).unwrap());
                    }

                    // Add a new RegexPatternTable entry for ListRegex type with the compiled regex list and word list
                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id.to_owned(),
                        table_match_type: RegexType::ListRegex {
                            regex_list,
                            word_list,
                        },
                    });
                }
                // Handle the Regex match table type
                MatchTableType::Regex => {
                    // Create a word list by cloning each word in the regex table's word list
                    let word_list = regex_table
                        .word_list
                        .iter()
                        .map(|&word| word.to_owned())
                        .collect::<Vec<String>>();

                    // Add a new RegexPatternTable entry for ListRegex type with compiled regex list and original word list
                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id.to_owned(),
                        table_match_type: RegexType::ListRegex {
                            regex_list: word_list
                                .iter()
                                .filter_map(|word| Regex::new(word).ok())
                                .collect(),
                            word_list,
                        },
                    });
                }
                // Handle unexpected match table types (unreachable code)
                _ => unreachable!(),
            };
        }

        // Return the constructed RegexMatcher instance containing the list of RegexPatternTables
        RegexMatcher {
            regex_pattern_table_list,
        }
    }
}

impl<'a> TextMatcherTrait<'a, RegexResult<'a>> for RegexMatcher {
    /// Checks if the given text matches any of the regex patterns in the `RegexPatternTable`.
    ///
    /// This function iterates through all the regex tables stored in `regex_pattern_table_list` and
    /// checks if the provided text matches any of the patterns based on the `RegexType`. If any match
    /// is found, the function returns `true`; otherwise, it returns `false`.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice (`&str`) containing the text to be checked against the stored regex patterns.
    ///
    /// # Returns
    ///
    /// * `bool` - `true` if the text matches any of the regex patterns, `false` otherwise.
    ///
    /// # Match Types
    ///
    /// The function handles different `RegexType` variants within the `RegexPatternTable`:
    ///
    /// * `StandardRegex` - Checks if the text matches a single compiled regex pattern.
    /// * `ListRegex` - Checks if the text matches any of the compiled regex patterns in the list.
    ///
    /// # Panics
    ///
    /// This function may panic if the regex matching fails unexpectedly, although this should be rare
    /// as it relies on precompiled and presumably valid regex patterns.
    ///
    /// # Examples
    ///
    /// Suppose you have a `RegexMatcher` instance configured with regex patterns and you want to check
    /// if a given text matches any of those patterns:
    ///
    /// ```
    /// use matcher_rs::{RegexMatcher, RegexTable, MatchTableType, TextMatcherTrait};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: "example_match",
    ///     match_table_type: MatchTableType::SimilarChar,
    ///     word_list: vec!["1,一", "2,二"],
    /// };
    ///
    /// let regex_matcher = RegexMatcher::new(&vec![regex_table]);
    ///
    /// assert!(regex_matcher.is_match("12"));
    /// assert!(!regex_matcher.is_match("abc"));
    /// ```
    fn is_match(&self, text: &str) -> bool {
        // Iterate through each regex table in the list of regex pattern tables
        for regex_table in &self.regex_pattern_table_list {
            // Match based on the type of regex pattern table (StandardRegex or ListRegex)
            match &regex_table.table_match_type {
                // Handle the StandardRegex type
                RegexType::StandardRegex { regex } => {
                    // Check if the text matches the single regex pattern
                    // If a match is found, return true immediately
                    if regex.is_match(text).unwrap() {
                        return true;
                    }
                }
                // Handle the ListRegex type
                RegexType::ListRegex { regex_list, .. } => {
                    // Check if the text matches any regex pattern in the list
                    // If a match is found, return true immediately
                    if regex_list.iter().any(|regex| regex.is_match(text).unwrap()) {
                        return true;
                    }
                }
            }
        }

        // Return false if no match is found in any of the regex patterns
        false
    }

    /// Processes the given text to find and return a list of regex match results.
    ///
    /// This function iterates through all regex tables in `regex_pattern_table_list` and attempts to find matches
    /// in the provided text. It constructs a `Vec` of `RegexResult` instances, each representing a matched pattern
    /// along with metadata identifying the table and match.
    ///
    /// # Arguments
    ///
    /// * `text` - A string slice that contains the text to search for regex matches.
    ///
    /// # Returns
    ///
    /// A `Vec` of `RegexResult` instances where each result corresponds to a pattern match found in the provided text.
    ///
    /// # Regex Types
    ///
    /// The function handles different `RegexType` variants within the `RegexPatternTable`:
    ///
    /// * `StandardRegex` - For each capture group match found, collects all non-empty matches and joins them into a
    ///   single string. Each match is added to the result list as an owned `String`.
    ///
    /// * `ListRegex` - For each regex in the list, if a match is found, adds the corresponding word from `word_list`
    ///   as a borrowed `&str` to the result list.
    ///
    /// # Examples
    ///
    /// Suppose you have a `RegexMatcher` instance configured with regex patterns and you want to process a given text
    /// to extract all matching patterns:
    ///
    /// ```
    /// use matcher_rs::{RegexMatcher, RegexTable, MatchTableType, TextMatcherTrait};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: "example_match",
    ///     match_table_type: MatchTableType::SimilarChar,
    ///     word_list: vec!["1,一", "2,二"],
    /// };
    ///
    /// let regex_matcher = RegexMatcher::new(&vec![regex_table]);
    ///
    /// let results = regex_matcher.process("1二");
    ///
    /// for result in results {
    ///     println!("{:?}", result);
    /// }
    /// ```
    fn process(&'a self, text: &str) -> Vec<RegexResult<'a>> {
        // Initialize an empty vector to hold the results of the regex matches
        let mut result_list = Vec::new();

        // Iterate through each regex pattern table stored in the RegexMatcher
        for regex_table in &self.regex_pattern_table_list {
            // Match based on the type of regex pattern table
            match &regex_table.table_match_type {
                // Handle the StandardRegex type
                RegexType::StandardRegex { regex } => {
                    // Iterate through all captures found by the regex in the provided text
                    for caps in regex.captures_iter(text).map(|caps| caps.unwrap()) {
                        // Create a new RegexResult and add it to the result list
                        result_list.push(RegexResult {
                            // Combine all non-empty capture groups into one string
                            word: Cow::Owned(
                                caps.iter()
                                    .skip(1)
                                    .filter_map(|m| m.map(|match_char| match_char.as_str()))
                                    .collect::<Vec<&str>>()
                                    .join(""),
                            ),
                            // Set the table_id and match_id fields in the result
                            table_id: regex_table.table_id,
                            match_id: &regex_table.match_id,
                        });
                    }
                }
                // Handle the ListRegex type
                RegexType::ListRegex {
                    regex_list,
                    word_list,
                } => {
                    // Iterate through each regex in the list
                    for (index, regex) in regex_list.iter().enumerate() {
                        // If the text matches the current regex
                        if regex.is_match(text).unwrap() {
                            // Create a new RegexResult and add it to the result list
                            result_list.push(RegexResult {
                                // Use the corresponding word from the word list as the result
                                word: Cow::Borrowed(&word_list[index]),
                                // Set the table_id and match_id fields in the result
                                table_id: regex_table.table_id,
                                match_id: &regex_table.match_id,
                            });
                        }
                    }
                }
            }
        }

        // Return the list of regex match results
        result_list
    }
}
