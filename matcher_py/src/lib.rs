use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::Infallible;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::{pyclass, pymethods, pymodule, wrap_pyfunction, PyModule, PyResult, Python};
use pyo3::types::{PyDict, PyDictMethods, PyModuleMethods};
use pyo3::{intern, pyfunction, Bound, IntoPyObject};

use matcher_rs::{
    reduce_text_process as reduce_text_process_rs, text_process as text_process_rs,
    MatchResult as MatchResultRs, MatchTableMapSerde as MatchTableMapRs, Matcher as MatcherRs,
    ProcessType, SimpleMatcher as SimpleMatcherRs, SimpleResult as SimpleResultRs,
    SimpleTableSerde as SimpleTableRs, TextMatcherTrait,
};

/// A structure representing a simple result from the SimpleMatcher.
///
/// This wraps around the [SimpleResultRs] type from the matcher_rs library,
/// allowing it to be used within this module's context.
///
/// The lifetime parameter `'a` ensures that the [SimpleResult] does not outlive
/// the data it references.
pub struct SimpleResult<'a>(SimpleResultRs<'a>);

impl<'py> IntoPyObject<'py> for SimpleResult<'py> {
    type Target = PyDict;
    type Output = Bound<'py, Self::Target>;
    type Error = Infallible;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let dict = PyDict::new(py);

        dict.set_item(intern!(py, "word_id"), self.0.word_id)
            .unwrap();
        dict.set_item(intern!(py, "word"), self.0.word.as_ref())
            .unwrap();

        Ok(dict)
    }
}

/// A structure representing a match result from the Matcher.
///
/// This wraps around the [MatchResultRs] type from the matcher_rs library,
/// allowing it to be used within this module's context.
///
/// The lifetime parameter `'a` ensures that the [MatchResult] does not outlive
/// the data it references.
pub struct MatchResult<'a>(MatchResultRs<'a>);

impl<'py> IntoPyObject<'py> for MatchResult<'py> {
    type Target = PyDict;
    type Output = Bound<'py, Self::Target>;
    type Error = Infallible;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let dict = PyDict::new(py);

        dict.set_item(intern!(py, "match_id"), self.0.match_id)
            .unwrap();
        dict.set_item(intern!(py, "table_id"), self.0.table_id)
            .unwrap();
        dict.set_item(intern!(py, "word_id"), self.0.word_id)
            .unwrap();
        dict.set_item(intern!(py, "word"), self.0.word.as_ref())
            .unwrap();
        dict.set_item(intern!(py, "similarity"), self.0.similarity)
            .unwrap();

        Ok(dict)
    }
}

/// Processes the given text based on the specified process type.
///
/// This function leverages the `text_process_rs` function from the `matcher_rs`
/// library to process the input text according to the provided `process_type`.
/// The `process_type` is expected to be a bitmask representing different processing
/// options as defined by the [ProcessType] enum from `matcher_rs`.
///
/// # Parameters
/// - `process_type` (u8): An 8-bit unsigned integer specifying the type of processing
///   to be applied to the text. This should be a valid bitmask for [ProcessType].
/// - `text` (&str): A string slice reference to the text that needs processing.
///
/// # Returns
/// - [`PyResult<Cow<'_, str>>`]: A Python result object wrapping either a [Cow] string slice
///   that contains the processed text or a [PyValueError] if the processing fails.
///
/// # Errors
/// This function returns a [PyValueError] if the `text_process_rs` function encounters
/// an error during processing.
#[pyfunction]
#[pyo3(signature=(process_type, text))]
fn text_process(process_type: u8, text: &str) -> PyResult<Cow<'_, str>> {
    let process_type = ProcessType::from_bits(process_type).unwrap_or(ProcessType::None);
    match text_process_rs(process_type, text) {
        Ok(result) => Ok(result),
        Err(e) => Err(PyValueError::new_err(e)),
    }
}

/// Reduces the given text based on the specified process type and returns a vector of [Cow] strings.
///
/// This function leverages the `reduce_text_process_rs` function from the `matcher_rs` library
/// to process the input text according to the provided `process_type`. The reduced text segments
/// are collected into a vector and returned as [Cow] string slices.
///
/// # Parameters
/// - `process_type` (u8): An 8-bit unsigned integer specifying the type of processing to be
///   applied to the text. This should be a valid bitmask for [ProcessType].
/// - `text` (&str): A string slice reference to the text that needs to be reduced.
///
/// # Returns
/// - [`Vec<Cow<'_, str>>`]: A vector of [Cow] string slices that contains the reduced text segments.
#[pyfunction]
#[pyo3(signature=(process_type, text))]
fn reduce_text_process(process_type: u8, text: &str) -> Vec<Cow<'_, str>> {
    let process_type = ProcessType::from_bits(process_type).unwrap_or(ProcessType::None);
    reduce_text_process_rs(process_type, text)
        .into_iter()
        .collect()
}

/// This class represents a Matcher, which provides functionality to match and process
/// text based on a matchup table map. It leverages the `matcher_rs` library to perform
/// the operations.
///
/// # Fields
/// - `matcher` [MatcherRs]: An instance of [MatcherRs] that performs the actual matching logic.
/// - `match_table_map_bytes` [`Vec<u8>`]: A byte vector representing the serialized form of the
///   match table map.
///
/// The [Matcher] class supports several methods for:
/// - Initializing a new instance with a serialized match table map.
/// - Implementing state serialization and deserialization methods for Python compatibility.
/// - Checking for matches in a given text.
/// - Processing the text to produce match results.
/// - Matching words in the text and returning results either as objects or strings.
#[pyclass(module = "matcher_py")]
pub struct Matcher {
    matcher: MatcherRs,
    match_table_map_bytes: Vec<u8>,
}

#[pymethods]
impl Matcher {
    /// Creates a new instance of the [Matcher] class using the provided match table map bytes.
    ///
    /// This function initializes a new [Matcher] by deserializing the provided byte slice into
    /// a [MatchTableMapRs] object using the `sonic_rs` library. The resulting map is then used
    /// to instantiate the actual [MatcherRs] object.
    ///
    /// # Parameters
    /// - `match_table_map_bytes` (&[u8]): A byte slice representing the serialized match table map.
    ///
    /// # Returns
    /// - [`PyResult<Matcher>`]: A Python result object wrapping the new [Matcher] instance, or
    ///   a [PyValueError] if deserialization of the byte slice fails.
    ///
    /// # Errors
    /// This function returns a [PyValueError] if the provided byte slice cannot be deserialized
    /// into a [MatchTableMapRs] object, usually indicating that the input data is invalid or corrupted.
    #[new]
    #[pyo3(signature=(match_table_map_bytes))]
    fn new(match_table_map_bytes: &[u8]) -> PyResult<Matcher> {
        let match_table_map: MatchTableMapRs = match sonic_rs::from_slice(match_table_map_bytes) {
            Ok(match_table_map) => match_table_map,
            Err(e) => {
                return Err(PyValueError::new_err(format!(
                "Deserialize match_table_map_bytes failed, Please check the input data.\nErr: {}",
                e
            )))
            }
        };

        Ok(Matcher {
            matcher: MatcherRs::new(&match_table_map),
            match_table_map_bytes: Vec::from(match_table_map_bytes),
        })
    }

    /// Returns the argument tuple to be passed to the `__new__` method during unpickling.
    ///
    /// This function provides compatibility with Python's pickling protocol by returning
    /// the necessary arguments to reconstruct the current instance of the Matcher class.
    ///
    /// # Returns
    /// - `(&[u8],)`: A single-element tuple containing a reference to the `match_table_map_bytes`
    ///   byte slice, which is used to reinitialize the Matcher instance.
    fn __getnewargs__(&self) -> (&[u8],) {
        (&self.match_table_map_bytes,)
    }

    /// Returns the byte slice representing the serialized match table map.
    ///
    /// This function provides compatibility with Python's pickling protocol by returning
    /// the internal `match_table_map_bytes` byte slice. This serialized form is used for
    /// saving the state of the Matcher instance, which can later be restored using the
    /// `__setstate__` method.
    ///
    /// # Returns
    /// - `&[u8]`: A reference to the byte slice containing the serialized match table map.
    fn __getstate__(&self) -> &[u8] {
        &self.match_table_map_bytes
    }

    /// Restores the state of the Matcher instance from the provided byte slice.
    ///
    /// This function is used for compatibility with Python's pickling protocol. It
    /// deserializes the given `match_table_map_bytes` into a [MatchTableMapRs] object
    /// and reinitializes the internal `matcher` field with this new map.
    ///
    /// # Parameters
    /// - `match_table_map_bytes` (&[u8]): A byte slice representing the serialized match table map.
    ///
    /// # Panics
    /// This function will panic if the provided byte slice cannot be deserialized into a
    /// [MatchTableMapRs] object. Ensure that the input data is correct and valid.
    #[pyo3(signature=(match_table_map_bytes))]
    fn __setstate__(&mut self, match_table_map_bytes: &[u8]) {
        self.matcher = MatcherRs::new(
            &sonic_rs::from_slice::<MatchTableMapRs>(match_table_map_bytes).unwrap(),
        );
        self.match_table_map_bytes = match_table_map_bytes.to_vec();
    }

    /// Checks if the given text matches any pattern.
    ///
    /// This function utilizes the internal `matcher` to determine if any part of the
    /// provided `text` conforms to the patterns defined within the matcher.
    ///
    /// # Parameters
    /// - `text` (&str): The input text to be checked against the match patterns.
    ///
    /// # Returns
    /// - `bool`: Returns `true` if the text matches any pattern, `false` otherwise.
    #[pyo3(signature=(text))]
    fn is_match(&self, text: &str) -> bool {
        self.matcher.is_match(text)
    }

    /// Processes the given text and returns a list of match results.
    ///
    /// This function uses the internal `matcher` to analyze the provided `text`
    /// and generate a list of [MatchResult] instances that represent the matches found.
    ///
    /// # Parameters
    /// - `text` (&str): The input text to be processed and checked for matches.
    ///
    /// # Returns
    /// - [`Vec<MatchResult<'_>>`]: A vector of [MatchResult] instances, where each entry
    ///   indicates a match found within the text according to the patterns defined within the matcher.
    #[pyo3(signature=(text))]
    fn process<'a>(&'a self, text: &'a str) -> Vec<MatchResult<'a>> {
        self.matcher
            .process(text)
            .into_iter()
            .map(MatchResult)
            .collect()
    }

    /// Matches words in the provided text and returns a mapping of match IDs to match results.
    ///
    /// This function uses the internal `matcher` to identify patterns in the given `text`. The results
    /// are organized in a [HashMap] where each key is a match ID (u32) and its value is a vector of
    /// [MatchResult] instances corresponding to that match ID.
    ///
    /// # Parameters
    /// - `text` (&str): The input text to be checked against the match patterns.
    ///
    /// # Returns
    /// - [`HashMap<u32, Vec<MatchResult<'_>>>`]: A mapping of match IDs to lists of match results,
    ///   indicating all patterns found in the text.
    #[pyo3(signature=(text))]
    fn word_match<'a>(&'a self, text: &'a str) -> HashMap<u32, Vec<MatchResult<'a>>> {
        self.matcher
            .word_match(text)
            .into_iter()
            .map(|(match_id, match_result_list)| {
                (
                    match_id,
                    match_result_list.into_iter().map(MatchResult).collect(),
                )
            })
            .collect()
    }

    /// Matches words in the provided text and returns a string representation of the results.
    ///
    /// This function uses the internal `matcher` to identify patterns in the given `text` and
    /// returns a string that represents the match results. The format of the string will depend
    /// on the internal implementation of the `word_match` method in the matcher.
    ///
    /// # Parameters
    /// - `text` (&str): The input text to be checked against the match patterns.
    ///
    /// # Returns
    /// - `String`: A string representation of the match results found in the text.
    #[pyo3(signature=(text))]
    fn word_match_as_string(&self, text: &str) -> String {
        unsafe { sonic_rs::to_string(&self.matcher.word_match(text)).unwrap_unchecked() }
    }
}

/// A Python class that wraps the `SimpleMatcherRs` Rust structure, providing
/// methods for matching text against a set of predefined patterns.
///
/// This class is intended to be used for simple pattern matching tasks. It
/// leverages a serialized table of patterns (provided as bytes) to initialize
/// the internal `simple_matcher` field. Various methods are provided to
/// interact with this matcher, allowing for checking if text matches any
/// patterns, processing text to get match results, and serializing/deserializing
/// the matcher's state.
///
/// # Fields
/// - `simple_matcher` [SimpleMatcherRs]: An instance of the `SimpleMatcherRs`
///   structure which performs the actual pattern matching.
/// - `simple_table_bytes` [`Vec<u8>`]: A byte vector that holds the serialized
///   representation of the match table.
#[pyclass(module = "matcher_py")]
pub struct SimpleMatcher {
    simple_matcher: SimpleMatcherRs,
    simple_table_bytes: Vec<u8>,
}

#[pymethods]
impl SimpleMatcher {
    /// Creates a new instance of `SimpleMatcher`.
    ///
    /// This constructor takes a byte slice representing a serialized match table and
    /// initializes the `SimpleMatcher` instance. The method performs deserialization
    /// of the bytes into a `SimpleTableRs` structure and uses it to create an internal
    /// `SimpleMatcherRs`.
    ///
    /// # Parameters
    /// - `simple_table_bytes` (&[u8]): A byte slice containing the serialized match table data.
    ///
    /// # Returns
    /// - `PyResult<SimpleMatcher>`: A result containing the newly created `SimpleMatcher`
    ///   instance, or a `PyValueError` if deserialization fails.
    ///
    /// # Errors
    /// - Returns a `PyValueError` if deserialization of `simple_table_bytes` fails, with a
    ///   message indicating the failure reason.
    #[new]
    #[pyo3(signature=(simple_table_bytes))]
    fn new(_py: Python, simple_table_bytes: &[u8]) -> PyResult<SimpleMatcher> {
        let simple_table: SimpleTableRs = match sonic_rs::from_slice(simple_table_bytes) {
            Ok(simple_table) => simple_table,
            Err(e) => {
                return Err(PyValueError::new_err(format!(
                    "Deserialize simple_table_bytes failed, Please check the input data.\n Err: {}",
                    e
                )))
            }
        };

        Ok(SimpleMatcher {
            simple_matcher: SimpleMatcherRs::new(&simple_table),
            simple_table_bytes: Vec::from(simple_table_bytes),
        })
    }

    /// Retrieves the arguments needed to create a new instance of `SimpleMatcher` during unpickling.
    ///
    /// This method returns a tuple containing the `simple_table_bytes` which is required
    /// to reconstruct the `SimpleMatcher` instance. It is used by Python's pickle module
    /// when deserializing an object.
    ///
    /// # Returns
    /// - `(&[u8],)`: A tuple containing a byte slice that represents the serialized match table.
    fn __getnewargs__(&self) -> (&[u8],) {
        (&self.simple_table_bytes,)
    }

    /// Retrieves the current state of the `SimpleMatcher` for serialization.
    ///
    /// This method returns a reference to the `simple_table_bytes` which
    /// represents the serialized state of the match table. It is typically used
    /// by serialization mechanisms to obtain the internal data necessary for
    /// reconstructing the `SimpleMatcher` instance.
    ///
    /// # Returns
    /// - `&[u8]`: A byte slice that contains the serialized match table data.
    fn __getstate__(&self) -> &[u8] {
        &self.simple_table_bytes
    }

    /// Restores the state of the `SimpleMatcher` from the provided bytes.
    ///
    /// This method is used to restore the `SimpleMatcher` instance from a serialized state.
    /// It deserializes the given bytes into a `SimpleTableRs` and then reinitializes the
    /// `simple_matcher` with the deserialized table.
    ///
    /// # Parameters
    /// - `simple_table_bytes` (&[u8]): A byte slice containing the serialized match table data.
    ///
    /// # Errors
    /// - Panics if deserialization of `simple_table_bytes` fails.
    #[pyo3(signature=(simple_table_bytes))]
    fn __setstate__(&mut self, simple_table_bytes: &[u8]) {
        self.simple_matcher = SimpleMatcherRs::new(
            &sonic_rs::from_slice::<SimpleTableRs>(simple_table_bytes).unwrap(),
        );
        self.simple_table_bytes = simple_table_bytes.to_vec();
    }

    /// Checks if the provided text matches any patterns.
    ///
    /// This method uses the internal `simple_matcher` to determine if the given
    /// text contains any matches according to the match table.
    ///
    /// # Parameters
    /// - `text` (&str): A string slice representing the text to be checked for matches.
    ///
    /// # Returns
    /// - `bool`: A boolean value indicating whether any patterns match the provided text.
    #[pyo3(signature=(text))]
    fn is_match(&self, text: &str) -> bool {
        self.simple_matcher.is_match(text)
    }

    /// Processes the provided text and returns a list of results.
    ///
    /// This method uses the internal `simple_matcher` to process the given
    /// text, producing a list of `SimpleResult` instances. Each result
    /// represents a match found according to the match table.
    ///
    /// # Parameters
    /// - `text` (&'a str): A string slice representing the text to be processed for matches.
    ///
    /// # Returns
    /// - `Vec<SimpleResult>`: A vector of `SimpleResult` instances, each encapsulating
    ///   a match found in the text.
    #[pyo3(signature=(text))]
    fn process<'a>(&'a self, text: &'a str) -> Vec<SimpleResult<'a>> {
        self.simple_matcher
            .process(text)
            .into_iter()
            .map(SimpleResult)
            .collect()
    }
}

#[pymodule]
fn matcher_py(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Matcher>()?;
    m.add_class::<SimpleMatcher>()?;
    m.add_function(wrap_pyfunction!(reduce_text_process, m)?)?;
    m.add_function(wrap_pyfunction!(text_process, m)?)?;
    Ok(())
}
