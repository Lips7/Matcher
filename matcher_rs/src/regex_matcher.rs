use std::borrow::Cow;

use fancy_regex::{escape, Regex};

use super::{MatchResultTrait, MatchTableType, TextMatcherTrait};

#[derive(Debug, Clone)]
/// A structure representing a regex table used for text matching.
///
/// The `RegexTable` structure is designed to hold a collection of words or patterns along
/// with metadata for identifying and categorizing the table.
///
/// # Fields
///
/// * `table_id` - A unique identifier for the regex table. This identifier distinguishes
///   the table from other regex tables.
///
/// * `match_id` - An ID that serves as an identifier for the match. This identifier
///   is used to differentiate match results originating from different regex tables.
///
/// * `match_table_type` - The type of match table, represented by the `MatchTableType` enumeration.
///   This field determines how the words or patterns in the table will be processed and matched
///   against the target text. Possible values include:
///   - `MatchTableType::SimilarChar`: Treats the word list as containing characters or patterns
///     that should be matched similarly within the text.
///   - `MatchTableType::Acrostic`: Treats the word list as containing acrostic patterns for matching.
///   - `MatchTableType::Regex`: Treats the word list as containing full regex patterns.
///
/// * `word_list` - A vector of string slices (`&'a str`) representing the words or patterns to be
///   matched. Each entry in the vector corresponds to a word or pattern that will be used in the
///   regex matching process.
///
/// This structure is used by the `RegexMatcher` to organize and manage different sets of patterns
/// along with their associated metadata, enabling efficient and flexible text matching operations.
///
/// # Examples
///
/// ```
/// use matcher_rs::{RegexTable, MatchTableType};
///
/// let regex_table = RegexTable {
///     table_id: 1,
///     match_id: 1,
///     match_table_type: MatchTableType::SimilarChar,
///     word_list: vec!["1,一", "2,二"],
/// };
///
/// println!("Regex Table ID: {}", regex_table.table_id);
/// println!("Match ID: {}", regex_table.match_id);
/// println!("Match Table Type: {:?}", regex_table.match_table_type);
/// println!("Word List: {:?}", regex_table.word_list);
/// ```
pub struct RegexTable<'a> {
    pub table_id: u64,
    pub match_id: u64,
    pub match_table_type: MatchTableType,
    pub word_list: Vec<&'a str>,
}

#[derive(Debug, Clone)]
/// Enum representing different types of regex pattern tables used in the `RegexMatcher`.
///
/// The `RegexType` enum is utilized within `RegexPatternTable` to define the structure and behavior of the regex
/// patterns stored in each table. It supports two types of regex patterns: `StandardRegex` and `ListRegex`.
///
/// # Variants
///
/// * `StandardRegex` - Represents a table that holds a single compiled regex pattern.
///   - `regex` (`Regex`): The compiled regex pattern used for matching text.
///
/// * `ListRegex` - Represents a table that holds a list of compiled regex patterns and their corresponding words.
///   - `regex_list` (`Vec<Regex>`): A list of compiled regex patterns used for matching text.
///   - `word_list` (`Vec<String>`): A list of words corresponding to each regex pattern in `regex_list`.
///
/// # Usage
///
/// This enum enables the `RegexMatcher` to distinguish between tables that use a singular regex pattern and those
/// that use multiple patterns stored in a list. The associated data for each variant ensures that the `RegexMatcher`
/// can accurately process match operations and return results based on the specific table type.
enum RegexType {
    StandardRegex {
        regex: Regex,
    },
    ListRegex {
        regex_list: Vec<Regex>,
        word_list: Vec<String>,
    },
}

#[derive(Debug, Clone)]
/// A structure representing a table of regex patterns used for text matching.
///
/// The `RegexPatternTable` struct is designed to hold compiled regex patterns and associated metadata,
/// allowing the `RegexMatcher` to efficiently organize and manage different sets of patterns for matching
/// text. Each `RegexPatternTable` instance corresponds to a specific regex table and contains details
/// such as a unique identifier, match identifier, and the type of regex patterns stored.
///
/// # Fields
///
/// * `table_id` - A unique identifier for the regex pattern table. This identifier distinguishes the table from other regex tables.
/// * `match_id` - A unique identifier for the match, which corresponds to the `match_id` of the `RegexTable` that contains the regex pattern.
/// * `table_match_type` - The type of regex pattern table, represented by the `RegexType` enum. This field determines the structure and behavior of the regex patterns stored in the table.
///
/// The `RegexPatternTable` struct is utilized internally by the `RegexMatcher` to categorize and execute regex-based text matching operations.
struct RegexPatternTable {
    table_id: u64,
    match_id: u64,
    table_match_type: RegexType,
}

/// Represents a result from a regex matching operation, containing metadata about the match.
///
/// The `RegexResult` structure is designed to encapsulate information about a particular regex match,
/// including the matched word or pattern, the table identifier from which the match originated, and
/// the match identifier associated with the match.
///
/// # Fields
///
/// * `word` - A `Cow<'a, str>` that holds the matched word or pattern. This field can either be a
///   borrowed string slice or an owned `String`, offering flexibility in how the match result is stored.
///
/// * `table_id` - A `u64` representing the unique identifier of the regex table that produced the match result.
///   This helps in distinguishing which regex table contributed to the result, facilitating organized processing
///   and categorization of matches.
///
/// * `match_id` - A `u64` that serves as an identifier for the match. This identifier
///   is used to differentiate between match results originating from different regex tables, allowing
///   for more detailed and organized match results.
///
/// # Example
///
/// ```
/// use matcher_rs::RegexResult;
/// use std::borrow::Cow;
///
/// let result = RegexResult {
///     word: Cow::Borrowed("example"),
///     table_id: 1,
///     match_id: 1,
/// };
///
/// println!("{:?}", result);
/// ```
///
/// The example above demonstrates how to create a `RegexResult` instance and print its fields for
/// debugging or logging purposes.
///
/// This structure is primarily utilized in text matching applications where regex patterns are used
/// to identify specific words or patterns within the target text, and the results need to be tracked
/// and processed accordingly.
#[derive(Debug, Clone)]
pub struct RegexResult<'a> {
    pub word: Cow<'a, str>,
    pub table_id: u64,
    pub match_id: u64,
}

impl MatchResultTrait<'_> for RegexResult<'_> {
    fn table_id(&self) -> u64 {
        self.table_id
    }
    fn word(&self) -> &str {
        self.word.as_ref()
    }
}

#[derive(Debug, Clone)]
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
///     match_id: 1,
///     match_table_type: MatchTableType::SimilarChar,
///     word_list: vec!["1,一", "2,二"],
/// };
///
/// let regex_matcher = RegexMatcher::new(&vec![regex_table]);
/// assert!(regex_matcher.is_match("12"));
/// assert!(regex_matcher.is_match("一2"));
/// assert!(regex_matcher.is_match("1二"));
/// ```
pub struct RegexMatcher {
    regex_pattern_table_list: Vec<RegexPatternTable>,
}

impl RegexMatcher {
    /// Constructs a new `RegexMatcher` instance from a list of `RegexTable` structures.
    ///
    /// This function initializes a `RegexMatcher` by processing the provided `regex_table_list` and
    /// compiling the necessary regex patterns based on the `MatchTableType` for each table. The resulting
    /// `RegexMatcher` contains a list of `RegexPatternTable` structures that store compiled regex patterns
    /// and related metadata for efficient text matching operations.
    ///
    /// # Arguments
    ///
    /// * `regex_table_list` - A reference to a vector of `RegexTable` structures, each representing a table
    ///   of words or patterns along with associated metadata and match table type.
    ///
    /// # Returns
    ///
    /// A `RegexMatcher` instance containing compiled regex patterns and metadata for performing text matching
    /// based on the provided `RegexTable` structures.
    ///
    /// # Match Table Types
    ///
    /// The function handles different `MatchTableType` variants within the `RegexTable`:
    ///
    /// * `SimilarChar` - Creates a combined regex pattern by escaping each word in the word list and joining
    ///   them with a `.?` separator. The resulting pattern is stored as a `StandardRegex` type in a new
    ///   `RegexPatternTable` entry.
    ///
    /// * `Acrostic` - Iterates through each word in the word list, creating corresponding regex patterns to
    ///   match acrostic patterns in the text. Each pattern is prefixed with `(?:^|[\s\pP]+?)` to support
    ///   case-insensitive matching at the start of words or after punctuation. The resulting patterns and
    ///   words are stored as a `ListRegex` type in a new `RegexPatternTable` entry.
    ///
    /// * `Regex` - Treats each word in the word list as a full regex pattern and compiles it accordingly.
    ///   The compiled regex patterns and corresponding words are stored as a `ListRegex` type in a new
    ///   `RegexPatternTable` entry.
    ///
    /// # Panics
    ///
    /// This function may panic if the regex compilation fails for any of the provided patterns. Such cases
    /// should be rare, as the input is typically prevalidated to ensure proper regex syntax.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::{RegexMatcher, RegexTable, MatchTableType, TextMatcherTrait};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     match_table_type: MatchTableType::SimilarChar,
    ///     word_list: vec!["1,一", "2,二"],
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

            match regex_table.match_table_type {
                MatchTableType::SimilarChar => {
                    // Create a combined regex pattern by escaping each word and joining them with .? separator
                    let pattern = regex_table
                        .word_list
                        .iter()
                        .map(|charstr| format!("({})", escape(charstr).replace(',', "|")))
                        .collect::<Vec<String>>()
                        .join(".?");

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        table_match_type: RegexType::StandardRegex {
                            regex: Regex::new(&pattern).unwrap(),
                        },
                    });
                }
                MatchTableType::Acrostic => {
                    let mut word_list = Vec::with_capacity(size);
                    let mut regex_list = Vec::with_capacity(size);

                    // Iterate through each word in the word list and create corresponding regex patterns
                    for &word in regex_table.word_list.iter() {
                        let pattern = format!(
                            r"(?i)(?:^|[\s\pP]+?){}",
                            escape(word).replace(',', r".*?[\s\pP]+?")
                        );

                        word_list.push(word.to_owned());
                        regex_list.push(Regex::new(&pattern).unwrap());
                    }

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        table_match_type: RegexType::ListRegex {
                            regex_list,
                            word_list,
                        },
                    });
                }
                MatchTableType::Regex => {
                    let word_list = regex_table
                        .word_list
                        .iter()
                        .map(|&word| word.to_owned())
                        .collect::<Vec<String>>();

                    regex_pattern_table_list.push(RegexPatternTable {
                        table_id: regex_table.table_id,
                        match_id: regex_table.match_id,
                        table_match_type: RegexType::ListRegex {
                            regex_list: word_list
                                .iter()
                                .filter_map(|word| Regex::new(word).ok())
                                .collect(),
                            word_list,
                        },
                    });
                }
                _ => unreachable!(),
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
    /// * `self` - A reference to the `RegexMatcher` instance.
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
    /// * `StandardRegex` - Checks if the text matches the single compiled regex pattern stored in the table.
    ///   If a match is found, the function returns `true`.
    ///
    /// * `ListRegex` - Iterates through the list of compiled regex patterns and checks if the text matches
    ///   any of them. If a match is found, the function returns `true`.
    ///
    /// If no matches are found after checking all regex patterns in all tables, the function returns `false`.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::{RegexMatcher, RegexTable, MatchTableType, TextMatcherTrait};
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     match_table_type: MatchTableType::SimilarChar,
    ///     word_list: vec!["1,一", "2,二"],
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
            match &regex_table.table_match_type {
                RegexType::StandardRegex { regex } => {
                    if regex.is_match(text).unwrap() {
                        return true;
                    }
                }
                RegexType::ListRegex { regex_list, .. } => {
                    if regex_list.iter().any(|regex| regex.is_match(text).unwrap()) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Processes the given text and returns a list of `RegexResult` instances for matches found.
    ///
    /// This function iterates through all the regex tables stored in `regex_pattern_table_list` and checks
    /// the provided text against the regex patterns based on the `RegexType` of each table. If a match is found,
    /// a corresponding `RegexResult` instance is created and added to the result list.
    ///
    /// # Arguments
    ///
    /// * `self` - A reference to the `RegexMatcher` instance.
    /// * `text` - A string slice (`&str`) containing the text to be processed and searched for matches.
    ///
    /// # Returns
    ///
    /// * `Vec<RegexResult<'a>>` - A vector of `RegexResult` instances, each representing a match found in the text.
    ///
    /// # Match Processing
    ///
    /// The function handles different `RegexType` variants within the `RegexPatternTable`:
    ///
    /// * `StandardRegex` - For each match found, the captured groups (except the full match) are concatenated
    ///   to form the matched word, which is stored in a `RegexResult` instance.
    ///
    /// * `ListRegex` - If the text matches any regex pattern in the list, the corresponding word from `word_list`
    ///   is stored in a `RegexResult` instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use matcher_rs::{RegexMatcher, RegexTable, MatchTableType, TextMatcherTrait, RegexResult};
    /// use std::borrow::Cow;
    ///
    /// let regex_table = RegexTable {
    ///     table_id: 1,
    ///     match_id: 1,
    ///     match_table_type: MatchTableType::SimilarChar,
    ///     word_list: vec!["1,一", "2,二"],
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
            match &regex_table.table_match_type {
                RegexType::StandardRegex { regex } => {
                    for caps in regex.captures_iter(text).map(|caps| caps.unwrap()) {
                        result_list.push(RegexResult {
                            word: Cow::Owned(
                                caps.iter()
                                    .skip(1)
                                    .filter_map(|m| m.map(|match_char| match_char.as_str()))
                                    .collect::<Vec<&str>>()
                                    .join(""),
                            ),
                            table_id: regex_table.table_id,
                            match_id: regex_table.match_id,
                        });
                    }
                }
                RegexType::ListRegex {
                    regex_list,
                    word_list,
                } => {
                    for (index, regex) in regex_list.iter().enumerate() {
                        if regex.is_match(text).unwrap() {
                            result_list.push(RegexResult {
                                word: Cow::Borrowed(&word_list[index]),
                                table_id: regex_table.table_id,
                                match_id: regex_table.match_id,
                            });
                        }
                    }
                }
            }
        }

        result_list
    }
}
