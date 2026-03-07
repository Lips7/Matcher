use std::borrow::Cow;
use std::convert::Infallible;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::{PyModule, PyResult, Python, pyclass, pymethods, pymodule, wrap_pyfunction};
use pyo3::types::{PyDict, PyDictMethods, PyModuleMethods};
use pyo3::{Bound, IntoPyObject, intern, pyfunction};

use matcher_rs::{
    ProcessType, SimpleMatcher as SimpleMatcherRs, SimpleResult as SimpleResultRs,
    SimpleTableSerde as SimpleTableRs, reduce_text_process as reduce_text_process_rs,
    text_process as text_process_rs,
};

/// A structure representing a simple result from the SimpleMatcher.
///
/// This wraps around the [`SimpleResultRs`] type from the matcher_rs library,
/// allowing it to be used within this module's context.
///
/// The lifetime parameter `'a` ensures that the [`SimpleResult`] does not outlive
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

/// Processes the given text based on the specified process type.
///
/// This function leverages the[`text_process_rs`]function from the `matcher_rs`
/// library to process the input text according to the provided `process_type`.
/// The `process_type` is expected to be a bitmask representing different processing
/// options as defined by the [`ProcessType`] enum from `matcher_rs`.
///
/// # Arguments
/// - `process_type` (u8): An 8-bit unsigned integer specifying the type of processing
///   to be applied to the text. This should be a valid bitmask for [`ProcessType`].
/// - `text` (&str): A string slice reference to the text that needs processing.
///
/// # Returns
/// - [`PyResult<Cow<'_, str>>`]: A Python result object wrapping either a [`Cow`] string slice
///   that contains the processed text or a [`PyValueError`] if the processing fails.
///
/// # Errors
/// This function returns a [`PyValueError`] if the [`text_process_rs`]function encounters
/// an error during processing.
#[pyfunction]
#[pyo3(signature=(process_type, text))]
fn text_process(process_type: u8, text: &str) -> PyResult<Cow<'_, str>> {
    let process_type = ProcessType::from_bits(process_type).unwrap_or(ProcessType::None);
    match text_process_rs(process_type, text) {
        Ok(result) => Ok(result),
        Err(e) => Err(PyValueError::new_err(e.to_string())),
    }
}

/// Reduces the given text based on the specified process type and returns a vector of [`Cow`] strings.
///
/// This function leverages the [`reduce_text_process_rs`] function from the [`matcher_rs`] library
/// to process the input text according to the provided `process_type`. The reduced text segments
/// are collected into a vector and returned as [`Cow`] string slices.
///
/// # Arguments
/// - `process_type` (u8): An 8-bit unsigned integer specifying the type of processing to be
///   applied to the text. This should be a valid bitmask for [`ProcessType`].
/// - `text` (&str): A string slice reference to the text that needs to be reduced.
///
/// # Returns
/// - [`Vec<Cow<'_, str>>`]: A vector of [`Cow`] string slices that contains the reduced text segments.
#[pyfunction]
#[pyo3(signature=(process_type, text))]
fn reduce_text_process(process_type: u8, text: &str) -> Vec<Cow<'_, str>> {
    let process_type = ProcessType::from_bits(process_type).unwrap_or(ProcessType::None);
    reduce_text_process_rs(process_type, text)
        .into_iter()
        .collect()
}

/// A Python class that wraps the [`SimpleMatcherRs`] Rust structure, providing
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
/// - `simple_matcher` [`SimpleMatcherRs`]: An instance of the [`SimpleMatcherRs`]
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
    /// initializes the [`SimpleMatcher`] instance. The method performs deserialization
    /// of the bytes into a [`SimpleTableRs`] structure and uses it to create an internal
    /// `SimpleMatcherRs`.
    ///
    /// # Arguments
    /// - `simple_table_bytes` (&[u8]): A byte slice containing the serialized match table data.
    ///
    /// # Returns
    /// - [`PyResult<SimpleMatcher>`]: A result containing the newly created [`SimpleMatcher`]
    ///   instance, or a [`PyValueError`] if deserialization fails.
    ///
    /// # Errors
    /// - Returns a [`PyValueError`] if deserialization of `simple_table_bytes` fails, with a
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
                )));
            }
        };

        Ok(SimpleMatcher {
            simple_matcher: SimpleMatcherRs::new(&simple_table),
            simple_table_bytes: Vec::from(simple_table_bytes),
        })
    }

    /// Retrieves the arguments needed to create a new instance of [`SimpleMatcher`] during unpickling.
    ///
    /// This method returns a tuple containing the `simple_table_bytes` which is required
    /// to reconstruct the [`SimpleMatcher`] instance. It is used by Python's pickle module
    /// when deserializing an object.
    ///
    /// # Returns
    /// - `(&[u8],)`: A tuple containing a byte slice that represents the serialized match table.
    fn __getnewargs__(&self) -> (&[u8],) {
        (&self.simple_table_bytes,)
    }

    /// Retrieves the current state of the [`SimpleMatcher`] for serialization.
    ///
    /// This method returns a reference to the `simple_table_bytes` which
    /// represents the serialized state of the match table. It is typically used
    /// by serialization mechanisms to obtain the internal data necessary for
    /// reconstructing the [`SimpleMatcher`] instance.
    ///
    /// # Returns
    /// - `&[u8]`: A byte slice that contains the serialized match table data.
    fn __getstate__(&self) -> &[u8] {
        &self.simple_table_bytes
    }

    /// Restores the state of the [`SimpleMatcher`] from the provided bytes.
    ///
    /// This method is used to restore the [`SimpleMatcher`] instance from a serialized state.
    /// It deserializes the given bytes into a [`SimpleTableRs`] and then reinitializes the
    /// `simple_matcher` with the deserialized table.
    ///
    /// # Arguments
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
    /// # Arguments
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
    /// text, producing a list of [`SimpleResult`] instances. Each result
    /// represents a match found according to the match table.
    ///
    /// # Arguments
    /// - `text` (&'a str): A string slice representing the text to be processed for matches.
    ///
    /// # Returns
    /// - `Vec<SimpleResult>`: A vector of [`SimpleResult`] instances, each encapsulating
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
    m.add_class::<SimpleMatcher>()?;
    m.add_function(wrap_pyfunction!(reduce_text_process, m)?)?;
    m.add_function(wrap_pyfunction!(text_process, m)?)?;
    Ok(())
}
