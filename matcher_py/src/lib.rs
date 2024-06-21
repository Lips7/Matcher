use std::borrow::Cow;
use std::collections::HashMap;

use numpy::{PyArray1, PyArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::{
    pyclass, pymethods, pymodule, wrap_pyfunction, Py, PyModule, PyObject, PyResult, Python,
};
use pyo3::types::{
    PyAnyMethods, PyDict, PyDictMethods, PyList, PyListMethods, PyString, PyStringMethods,
};
use pyo3::{intern, pyfunction, Bound, IntoPy};

use matcher_rs::{
    reduce_text_process as reduce_text_process_rs, text_process as text_process_rs,
    MatchResult as MatchResultRs, MatchResultTrait, MatchTableMap as MatchTableMapRs,
    Matcher as MatcherRs, SimpleMatchType, SimpleMatchTypeWordMap as SimpleMatchTypeWordMapRs,
    SimpleMatcher as SimpleMatcherRs, SimpleResult as SimpleResultRs, TextMatcherTrait,
};

/// A struct that wraps around the [SimpleResultRs] struct from the [matcher_rs] crate.
///
/// This struct serves as a bridge between the Rust and Python representations of match results.
/// It encapsulates a [SimpleResultRs] instance and provides necessary implementations
/// for conversion and trait compliance, facilitating its use in Python.
///
/// # Lifetime Parameters
/// - `'a`: The lifetime parameter that corresponds to the lifetime of the encapsulated
///   [SimpleResultRs] instance.
///
/// # Example
/// ```no_run
/// use std::borrow::Cow;
///
/// use matcher_py::*;
/// use matcher_rs::SimpleResult as SimpleResultRs;
///
/// let simple_result_rs = SimpleResultRs {
///     word_id: 1,
///     word: Cow::borrowed("example"),
/// };
/// let simple_result = SimpleResult(simple_result_rs);
///
/// assert_eq!(simple_result.0.word_id, 1);
/// assert_eq!(simple_result.0.word, "example");
/// ```
struct SimpleResult<'a>(SimpleResultRs<'a>);

impl<'a> IntoPy<PyObject> for SimpleResult<'a> {
    /// Converts a [SimpleResult] instance into a Python dictionary [PyObject].
    ///
    /// This implementation of the [IntoPy] trait allows for converting a [SimpleResult]
    /// into a Python dictionary containing the match result data, which can be used
    /// in Python code. The dictionary includes the following key-value pairs:
    ///
    /// - `"word_id"`: The unique identifier (u64) for the matched word.
    /// - `"word"`: The matched word as a string slice.
    ///
    /// # Parameters
    /// - `self`: The [SimpleResult] instance to be converted.
    /// - `py`: The Python interpreter state.
    ///
    /// # Returns
    /// - [PyObject]: A Python dictionary containing the match result data.
    ///
    /// # Panics
    /// Panics if setting a dictionary item fails. Although highly unlikely,
    /// failures might occur due to memory issues or internal Python state inconsistencies.
    /// ```text
    fn into_py(self, py: Python<'_>) -> PyObject {
        let dict = PyDict::new_bound(py);

        dict.set_item(intern!(py, "word_id"), self.0.word_id)
            .unwrap();
        dict.set_item(intern!(py, "word"), self.0.word.as_ref())
            .unwrap();

        dict.into()
    }
}

impl MatchResultTrait<'_> for SimpleResult<'_> {
    fn word_id(&self) -> u64 {
        self.0.word_id()
    }
    fn word(&self) -> &str {
        self.0.word.as_ref()
    }
}

struct MatchResult<'a>(MatchResultRs<'a>);

impl<'a> IntoPy<PyObject> for MatchResult<'a> {
    /// Converts a [MatchResult] instance into a Python dictionary [PyObject].
    ///
    /// This implementation of the [IntoPy] trait allows for converting a [MatchResult]
    /// into a Python dictionary containing the match result data, which can be used
    /// in Python code. The dictionary includes the following key-value pairs:
    ///
    /// - `"table_id"`: The unique identifier (u64) for the table.
    /// - `"word"`: The matched word as a string slice.
    ///
    /// # Parameters
    /// - `self`: The [MatchResult] instance to be converted.
    /// - `py`: The Python interpreter state.
    ///
    /// # Returns
    /// - [PyObject]: A Python dictionary containing the match result data.
    ///
    /// # Panics
    /// Panics if setting a dictionary item fails. Although highly unlikely,
    /// failures might occur due to memory issues or internal Python state inconsistencies.
    /// ```text
    fn into_py(self, py: Python<'_>) -> PyObject {
        let dict = PyDict::new_bound(py);

        dict.set_item(intern!(py, "table_id"), self.0.table_id)
            .unwrap();
        dict.set_item(intern!(py, "word"), self.0.word.as_ref())
            .unwrap();

        dict.into()
    }
}

#[pyfunction]
#[pyo3(signature=(simple_match_type, text))]
/// Processes text using a specified simple match type.
///
/// This function applies a text processing operation based on the provided simple match type,
/// which is an enumeration representing different types of simple matches.
/// The function is directly linked to the `text_process_rs` function from the
/// `matcher_rs` crate, which performs the actual processing logic.
///
/// # Parameters
/// - `simple_match_type` (u8): A byte value that corresponds to a specific type of simple match.
/// - `text` (&str): A string slice containing the text to be processed.
///
/// # Returns
/// - `PyResult<Cow<'_, str>>`: On success, returns a `Cow` string representing the processed text.
///   On failure, returns a Python exception detailing the error.
///
/// # Errors
/// This function will return a `PyValueError` if the text processing operation
/// in the `matcher_rs` crate fails, encapsulating the underlying error message.
fn text_process(simple_match_type: u8, text: &str) -> PyResult<Cow<'_, str>> {
    let simple_match_type =
        SimpleMatchType::from_bits(simple_match_type).unwrap_or(SimpleMatchType::None);
    match text_process_rs(simple_match_type, text) {
        Ok(result) => Ok(result),
        Err(e) => Err(PyValueError::new_err(e)),
    }
}

#[pyfunction]
#[pyo3(signature=(simple_match_type, text))]
/// Reduces text using a specified simple match type.
///
/// This function applies a text reduction process based on the provided simple match type,
/// which is an enumeration representing different types of simple matches.
/// The function is directly linked to the `reduce_text_process_rs` function from the
/// `matcher_rs` crate, which performs the actual reduction logic.
///
/// # Parameters
/// - `simple_match_type` (u8): A byte value that corresponds to a specific type of simple match.
/// - `text` (&str): A string slice containing the text to be processed.
///
/// # Returns
/// - `Vec<Cow<'_, str>>`: A vector of `Cow` strings representing the reduced text fragments.
///
/// # Errors
/// This function will default to `SimpleMatchType::None` if the provided byte value does not
/// correspond to a valid `SimpleMatchType`. It will not raise an error in such cases but will
/// produce results based on the `SimpleMatchType::None`.
fn reduce_text_process(simple_match_type: u8, text: &str) -> Vec<Cow<'_, str>> {
    let simple_match_type =
        SimpleMatchType::from_bits(simple_match_type).unwrap_or(SimpleMatchType::None);
    reduce_text_process_rs(simple_match_type, text)
        .into_iter()
        .collect()
}

#[pyclass(module = "matcher_py")]
/// A Python class that wraps the [MatcherRs] struct from the [matcher_rs] crate.
///
/// This class provides functionality for text matching using a deserialized match table map.
/// It allows for single text matching, batch text processing using Python lists, and batch
/// processing using NumPy arrays.
///
/// # Fields
/// - `matcher`: An instance of the [MatcherRs] struct that performs the core matching logic.
/// - `match_table_map_bytes`: A serialized byte array representing the match table map,
///   used for reconstructing the [MatcherRs] instance during deserialization.
///
/// # Example
///
/// ```python
/// import msgspec
/// import numpy as np
///
/// from matcher_py import Matcher
/// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
///
/// msgpack_encoder = msgspec.msgpack.Encoder()
///
/// matcher = Matcher(
///     msgpack_encoder.encode(
///         {
///             1: [
///                 MatchTable(
///                     table_id=1,
///                     match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
///                     word_list=["hello", "world"],
///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
///                     exemption_word_list=["word"],
///                 )
///             ]
///         }
///     )
/// )
///
/// # Check if a text matches
/// assert matcher.is_match("hello")
/// assert not matcher.is_match("hello, word")
///
/// # Perform word matching as a dict
/// assert matcher.word_match(r"hello, world")[1]
///
/// # Perform word matching as a string
/// result = matcher.word_match_as_string("hello")
/// assert result == """{1:"[{\\"table_id\\":1,\\"word\\":\\"hello\\"}]"}"""
///
/// # Perform batch processing as a dict using a list
/// text_list = ["hello", "world", "hello,word"]
/// batch_results = matcher.batch_word_match(text_list)
/// print(batch_results)
///
/// # Perform batch processing as a string using a list
/// text_list = ["hello", "world", "hello,word"]
/// batch_results = matcher.batch_word_match_as_string(text_list)
/// print(batch_results)
///
/// # Perform batch processing as a dict using a numpy array
/// text_array = np.array(["hello", "world", "hello,word"], dtype=np.dtype("object"))
/// numpy_results = matcher.numpy_word_match(text_array)
/// print(numpy_results)
///
/// # Perform batch processing as a string using a numpy array
/// text_array = np.array(["hello", "world", "hello,word"], dtype=np.dtype("object"))
/// numpy_results = matcher.numpy_word_match_as_string(text_array)
/// print(numpy_results)
/// ```
struct Matcher {
    matcher: MatcherRs,
    match_table_map_bytes: Vec<u8>,
}

#[pymethods]
impl Matcher {
    #[new]
    #[pyo3(signature=(match_table_map_bytes))]
    /// Creates a new instance of the [Matcher] class from a serialized byte array.
    ///
    /// This constructor takes a serialized byte array representing a match table map,
    /// deserializes it, and uses it to initialize the [Matcher] instance. If the
    /// deserialization fails, an error is returned.
    ///
    /// # Parameters
    /// - `match_table_map_bytes`: A reference to a byte slice containing the serialized
    ///   match table map data.
    ///
    /// # Returns
    /// - [`PyResult<Matcher>`]: A result containing the newly created [Matcher] instance
    ///   if successful, or a [PyValueError] if deserialization fails.
    ///
    /// # Errors
    /// Returns a [PyValueError] with an error message if the deserialization of the byte array
    /// into a [MatchTableMapRs] fails. The error message includes details about the failure.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import Matcher
    /// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// match_table_map_bytes = msgpack_encoder.encode(
    ///     {
    ///         1: [
    ///             MatchTable(
    ///                 table_id=1,
    ///                 match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                 word_list=["hello", "world"],
    ///                 exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                 exemption_word_list=["word"],
    ///             )
    ///         ]
    ///     }
    /// )
    ///
    /// matcher = Matcher(match_table_map_bytes)
    /// ```
    fn new(match_table_map_bytes: &[u8]) -> PyResult<Matcher> {
        let match_table_map: MatchTableMapRs = match rmp_serde::from_slice(match_table_map_bytes) {
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

    /// Returns the arguments needed to create a new [Matcher] instance during unpickling.
    ///
    /// This method provides the byte array representing the match table map, which is
    /// necessary to reconstruct the [Matcher] object when it is unpickled in Python.
    ///
    /// # Parameters
    /// - `self`: The [Matcher] instance.
    ///
    /// # Returns
    /// - `&[u8]`: A reference to the byte array containing the serialized match table map data.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import Matcher
    /// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// matcher = Matcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             1: [
    ///                 MatchTable(
    ///                     table_id=1,
    ///                     match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// pickle_data = pickle.dumps(matcher)
    /// unpickled_matcher = pickle.loads(pickle_data)
    /// ```
    fn __getnewargs__(&self) -> (&[u8],) {
        (&self.match_table_map_bytes,)
    }

    /// Serializes the [Matcher] object's state for pickling.
    ///
    /// This method is called during the pickling process to extract the state of the
    /// [Matcher] instance in the form of a byte array. This byte array represents the
    /// match table map, which can be used to reconstruct the [Matcher] object during
    /// unpickling.
    ///
    /// # Returns
    /// - `&[u8]`: A reference to the byte array containing the serialized match table map data.
    ///
    /// # Example
    ///
    /// ```python
    /// import pickle
    /// import msgspec
    ///
    /// from matcher_py import Matcher
    /// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// matcher = Matcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             1: [
    ///                 MatchTable(
    ///                     table_id=1,
    ///                     match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Serialize the object to a byte array
    /// pickle_data = pickle.dumps(matcher)
    ///
    /// # Deserialize the object from a byte array
    /// unpickled_matcher = pickle.loads(pickle_data)
    /// ```
    fn __getstate__(&self) -> &[u8] {
        &self.match_table_map_bytes
    }

    #[pyo3(signature=(match_table_map_bytes))]
    /// Restores the state of the [Matcher] object from a serialized byte array.
    ///
    /// This method is called during the unpickling process to restore the state of the
    /// [Matcher] instance using the provided byte array. The byte array should represent
    /// a serialized [MatchTableMapRs]. The method deserializes this byte array to reconstruct
    /// the match table map and updates the `matcher` attribute accordingly.
    ///
    /// # Parameters
    /// - `self`: The [Matcher] instance.
    /// - `match_table_map_bytes`: A reference to the byte array containing the serialized
    ///   match table map data.
    ///
    /// # Example
    ///
    /// ```python
    /// import pickle
    /// import msgspec
    ///
    /// from matcher_py import Matcher
    /// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// matcher = Matcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             1: [
    ///                 MatchTable(
    ///                     table_id=1,
    ///                     match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Serialize the object to a byte array
    /// pickle_data = pickle.dumps(matcher)
    ///
    /// # Deserialize the object from a byte array and restore its state
    /// unpickled_matcher = pickle.loads(pickle_data)
    /// unpickled_matcher.__setstate__(msgpack_encoder.encode(
    ///     {
    ///         1: [
    ///             MatchTable(
    ///                 table_id=1,
    ///                 match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                 word_list=["hello", "world"],
    ///                 exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                 exemption_word_list=["word"],
    ///             )
    ///         ]
    ///     }
    /// ))
    /// ```
    fn __setstate__(&mut self, match_table_map_bytes: &[u8]) {
        self.matcher = MatcherRs::new(
            &rmp_serde::from_slice::<MatchTableMapRs>(match_table_map_bytes).unwrap(),
        );
    }

    #[pyo3(signature=(text))]
    /// Checks if the given text contains any matches according to the configured match tables.
    ///
    /// This method uses the `is_match` function of the [MatcherRs] instance to determine whether
    /// the input text contains any words that match the criteria defined in the match tables.
    ///
    /// # Parameters
    /// - `self`: The [Matcher] instance.
    /// - `text`: A string slice representing the text to be checked for matches.
    ///
    /// # Returns
    /// - `bool`: `true` if the text contains a match, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import Matcher
    /// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// matcher = Matcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             1: [
    ///                 MatchTable(
    ///                     table_id=1,
    ///                     match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Check if the text contains any matches
    /// result = matcher.is_match("hello")
    /// print(result)  # Output: True
    /// ```
    fn is_match(&self, text: &str) -> bool {
        self.matcher.is_match(text)
    }

    #[pyo3(signature=(text))]
    /// Performs word matching on the given text and returns the results as a dictionary.
    ///
    /// This method leverages the `word_match` function of the [MatcherRs] instance to identify
    /// matches within the provided text. The results are mapped into a `HashMap` where the
    /// keys are match IDs and the values are lists of [MatchResult] objects.
    ///
    /// # Parameters
    /// - `self`: The [Matcher] instance.
    /// - `text`: A string slice representing the text to be checked for matches.
    ///
    /// # Returns
    /// - `HashMap<u64, Vec<MatchResult<'_>>>`: A dictionary where each key is a match ID (u64),
    ///   and each value is a list of [MatchResult] objects corresponding to the matches found.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import Matcher
    /// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// matcher = Matcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             1: [
    ///                 MatchTable(
    ///                     table_id=1,
    ///                     match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Perform word matching and get the results as a dictionary
    /// result = matcher.word_match("hello")
    /// print(result)  # Output: Dictionary with match IDs as keys and lists of MatchResult objects as values
    /// ```
    fn word_match(&self, text: &str) -> HashMap<u64, Vec<MatchResult<'_>>> {
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

    #[pyo3(signature=(text))]
    /// Returns the word match results for the given text as a JSON string.
    ///
    /// This method checks if the input `text` is empty. If it is, the method returns an empty JSON object (`{}`)
    /// as a string. Otherwise, it leverages the `word_match_as_string` function of the [MatcherRs] instance to
    /// obtain the word match results as a JSON string.
    ///
    /// # Parameters
    /// - `self`: The [Matcher] instance.
    /// - `text`: A string slice representing the text to be checked for matches.
    ///
    /// # Returns
    /// - `String`: A JSON string representing the match results. Returns `{}` if the input text is empty.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import Matcher
    /// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// matcher = Matcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             1: [
    ///                 MatchTable(
    ///                     table_id=1,
    ///                     match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Get word match results as a JSON string
    /// result = matcher.word_match_as_string("hello")
    /// print(result)  # Output: JSON string with match results
    /// ```
    fn word_match_as_string(&self, text: &str) -> String {
        text.is_empty()
            .then_some(String::from("{}"))
            .unwrap_or_else(|| self.matcher.word_match_as_string(text))
    }

    #[pyo3(signature=(text_array))]
    /// Batch processes a list of texts and performs word matching on each text,
    /// returning the results as a list of dictionaries.
    ///
    /// This method iterates over a [PyList] containing texts, performs word matching
    /// on each text using the [word_match](Matcher::word_match) method, and collects
    /// the results into a [Vec<HashMap<u64, Vec<MatchResult<'_>>>>].
    ///
    /// # Parameters
    /// - `self`: The [Matcher] instance.
    /// - `text_array`: A reference to a [PyList] containing texts to be processed.
    ///
    /// # Returns
    /// - `PyResult<Vec<HashMap<u64, Vec<MatchResult<'_>>>>>`: A result containing a
    ///   vector of dictionaries. Each dictionary has match IDs (u64) as keys and lists
    ///   of [MatchResult] objects as values.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import Matcher
    /// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// matcher = Matcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             1: [
    ///                 MatchTable(
    ///                     table_id=1,
    ///                     match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Perform word matching for a batch of texts
    /// text_array = ["hello", "world", "hello world"]
    /// result = matcher.batch_word_match(text_array)
    /// print(result)  # Output: List of dictionaries with match results for each text
    /// ```
    fn batch_word_match(
        &self,
        text_array: &Bound<'_, PyList>,
    ) -> PyResult<Vec<HashMap<u64, Vec<MatchResult<'_>>>>> {
        let mut result_list = Vec::with_capacity(text_array.len());

        for text in text_array.iter() {
            let text_py_string = text.downcast::<PyString>()?;
            result_list.push(self.word_match(text_py_string.to_cow().as_ref().unwrap()));
        }

        Ok(result_list)
    }

    #[pyo3(signature=(text_array))]
    /// Batch processes a list of texts and performs word matching,
    /// returning the results as a list of JSON strings.
    ///
    /// This method iterates over a [PyList] containing texts, performs word matching
    /// on each text using the [word_match_as_string](Matcher::word_match_as_string) method,
    /// and collects the results into a vector of JSON strings.
    ///
    /// # Parameters
    /// - `self`: The [Matcher] instance.
    /// - `text_array`: A reference to a [PyList] containing texts to be processed.
    ///
    /// # Returns
    /// - `PyResult<Vec<String>>`: A result containing a vector of JSON strings. Each string
    ///   represents the match results for the corresponding input text.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import Matcher
    /// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    /// matcher = Matcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             1: [
    ///                 MatchTable(
    ///                     table_id=1,
    ///                     match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Perform word matching for a batch of texts and get results as JSON strings
    /// text_array = ["hello", "world", "hello world"]
    /// result = matcher.batch_word_match_as_string(text_array)
    /// print(result)  # Output: List of JSON strings with match results for each text
    /// ```
    fn batch_word_match_as_string(&self, text_array: &Bound<'_, PyList>) -> PyResult<Vec<String>> {
        let mut result_list = Vec::with_capacity(text_array.len());

        for text in text_array.iter() {
            let text_py_string = text.downcast::<PyString>()?;
            result_list.push(self.word_match_as_string(text_py_string.to_cow().as_ref().unwrap()));
        }

        Ok(result_list)
    }

    #[pyo3(signature=(text_array, inplace = false))]
    /// Batch processes a NumPy 1-D array of texts and performs word matching
    /// on each text, returning the results as Python objects.
    ///
    /// This function iterates over a NumPy 1-D array of texts, performs word matching
    /// on each text, and collects the results into a new NumPy array or modifies the
    /// original array in-place based on the `inplace` parameter. If `inplace` is set to `true`,
    /// the original array is modified directly. The result for each text is obtained by
    /// calling the [word_match](Matcher::word_match) method.
    ///
    /// # Parameters
    /// - `self`: The [Matcher] instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a [PyArray1] containing texts to be processed.
    /// - `inplace`: A boolean flag indicating whether to modify the original array in-place.
    ///
    /// # Returns
    /// - `Option<Py<PyArray1<PyObject>>>`: If `inplace` is `false`, a new `PyArray1` containing
    ///   the word match results for each text as Python objects. If `inplace` is `true`, returns
    ///   [None] as the original array is modified in-place.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// import numpy as np
    ///
    /// from matcher_py import Matcher
    /// from matcher_py.extension_types import MatchTable, MatchTableType, SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    /// matcher = Matcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             1: [
    ///                 MatchTable(
    ///                     table_id=1,
    ///                     match_table_type=MatchTableType.Simple(simple_match_type=SimpleMatchType.MatchNone),
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// text_array = np.array(["hello", "world", "hello word"], dtype=np.dtype("object"))
    /// result = matcher.numpy_word_match(text_array)
    /// print(result)  # Output: A new NumPy array with word match results as Python objects
    ///
    /// inplace_result = matcher.numpy_word_match(text_array, inplace=True)
    /// print(text_array)  # Output: The original NumPy array modified with word match results
    /// ```
    fn numpy_word_match(
        &self,
        py: Python,
        text_array: &Bound<'_, PyArray1<PyObject>>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                *text = text
                    .downcast_bound::<PyString>(py)
                    .map_or(py.None(), |text_py_string| {
                        self.word_match(text_py_string.to_cow().as_ref().unwrap())
                            .into_py(py)
                    });
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    unsafe { text_array.as_array() }.map(|text| {
                        text.downcast_bound::<PyString>(py)
                            .map_or(py.None(), |text_py_string| {
                                self.word_match(text_py_string.to_cow().as_ref().unwrap())
                                    .into_py(py)
                            })
                    }),
                )
                .into(),
            )
        }
    }

    #[pyo3(signature=(text_array, inplace = false))]
    /// Batch processes a NumPy 1-D array of texts and performs word matching as strings
    /// on each text, returning the results as Python objects.
    ///
    /// This function iterates over a NumPy 1-D array of texts, performs word matching
    /// as strings on each text, and collects the results into a new NumPy array or modifies the
    /// original array in-place based on the `inplace` parameter. If `inplace` is set to `true`,
    /// the original array is modified directly. The result for each text is obtained by
    /// calling the [word_match_as_string](Matcher::word_match_as_string) method.
    ///
    /// # Parameters
    /// - `self`: The [Matcher] instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a [PyArray1] containing texts to be processed.
    /// - `inplace`: A boolean flag indicating whether to modify the original array in-place.
    ///
    /// # Returns
    /// - `Option<Py<PyArray1<PyObject>>>`: If `inplace` is `false`, a new `PyArray1` containing
    ///   the word match results as strings for each text as Python objects. If `inplace` is `true`,
    ///   returns [None] as the original array is modified in-place.
    ///
    /// # Example
    ///
    /// ```python
    /// import numpy as np
    ///
    /// from matcher_py import Matcher
    ///
    /// matcher = Matcher(...)
    ///
    /// text_array = np.array(["hello", "world", "hello word"], dtype=np.dtype("object"))
    /// result = matcher.numpy_word_match_as_string(text_array)
    /// print(result)  # Output: A new NumPy array with word match results as Python objects
    ///
    /// inplace_result = matcher.numpy_word_match_as_string(text_array, inplace=True)
    /// print(text_array)  # Output: The original NumPy array modified with word match results
    /// ```
    fn numpy_word_match_as_string(
        &self,
        py: Python,
        text_array: &Bound<'_, PyArray1<PyObject>>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                *text = text
                    .downcast_bound::<PyString>(py)
                    .map_or(py.None(), |text_py_string| {
                        self.word_match_as_string(text_py_string.to_cow().as_ref().unwrap())
                            .into_py(py)
                    });
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    unsafe { text_array.as_array() }.map(|text| {
                        text.downcast_bound::<PyString>(py)
                            .map_or(py.None(), |text_py_string| {
                                self.word_match_as_string(text_py_string.to_cow().as_ref().unwrap())
                                    .into_py(py)
                            })
                    }),
                )
                .into(),
            )
        }
    }
}

#[pyclass(module = "matcher_py")]
/// A Python class that wraps the [SimpleMatcherRs] struct from the [matcher_rs] crate.
///
/// This class provides functionality for simple text matching using a serialized
/// type word map. It enables single text matching and batch text processing
/// using both Python lists and NumPy arrays.
///
/// # Fields
/// - `simple_matcher`: An instance of the [SimpleMatcherRs] struct which performs
///   the core matching logic.
/// - `simple_match_type_word_map_bytes`: A serialized byte array representing the
///   simple match type word map used for initializing the `simple_matcher` field during
///   deserialization.
///
/// # Example
/// ```python
/// import msgspec
///
/// import numpy as np
///
/// from matcher_py import SimpleMatcher
/// from matcher_py.extension_types import SimpleMatchType
///
/// msgpack_encoder = msgspec.msgpack.Encoder()
///
/// simple_matcher = SimpleMatcher(
///     msgpack_encoder.encode(
///         {
///             SimpleMatchType.MatchNone: {
///                 1: "example"
///             }
///         }
///     )
/// )
///
/// # Check if a text matches
/// assert simple_matcher.is_match("example")
///
/// # Perform simple processing
/// results = simple_matcher.simple_process("example")
/// print(results)
///
/// # Perform batch processing using a list
/// text_list = ["example", "test", "example test"]
/// batch_results = simple_matcher.batch_simple_process(text_list)
/// print(batch_results)
///
/// # Perform batch processing using a NumPy array
/// text_array = np.array(["example", "test", "example test"], dtype=np.dtype("object"))
/// numpy_results = simple_matcher.numpy_simple_process(text_array)
/// print(numpy_results)
/// ```
struct SimpleMatcher {
    simple_matcher: SimpleMatcherRs,
    simple_match_type_word_map_bytes: Vec<u8>,
}

#[pymethods]
impl SimpleMatcher {
    #[new]
    #[pyo3(signature=(simple_match_type_word_map_bytes))]
    /// Creates a new instance of [SimpleMatcher].
    ///
    /// This constructor initializes a new [SimpleMatcher] by deserializing the provided byte array
    /// representing the simple match type word map. The byte array is deserialized using the `rmp_serde`
    /// crate to reconstruct the map, which is then used to initialize the underlying `simple_matcher` field.
    ///
    /// # Parameters
    /// - `_py`: The Python interpreter state.
    /// - `simple_match_type_word_map_bytes`: A byte slice that contains the serialized simple match type word map.
    ///
    /// # Errors
    /// - Returns a [PyValueError] if deserialization of the `simple_match_type_word_map_bytes` fails.
    ///
    /// # Returns
    /// - [`PyResult<SimpleMatcher>`]: An instance of [SimpleMatcher] if deserialization and initialization are successful.
    ///
    /// # Example
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import SimpleMatcher
    /// from matcher_py.extension_types import SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// simple_match_type_word_map = msgpack_encoder.encode(
    ///     {
    ///         SimpleMatchType.MatchNone: {
    ///             1: "example"
    ///         }
    ///     }
    /// )
    ///
    /// simple_matcher = SimpleMatcher(simple_match_type_word_map)
    /// print(simple_matcher.simple_matcher)
    /// ```
    fn new(_py: Python, simple_match_type_word_map_bytes: &[u8]) -> PyResult<SimpleMatcher> {
        let simple_match_type_word_map: SimpleMatchTypeWordMapRs =
            match rmp_serde::from_slice(simple_match_type_word_map_bytes) {
                Ok(simple_match_type_word_map) => simple_match_type_word_map,
                Err(e) => return Err(PyValueError::new_err(
                    format!("Deserialize simple_match_type_word_map_bytes failed, Please check the input data.\n Err: {}", e),
                )),
            };

        Ok(SimpleMatcher {
            simple_matcher: SimpleMatcherRs::new(&simple_match_type_word_map),
            simple_match_type_word_map_bytes: Vec::from(simple_match_type_word_map_bytes),
        })
    }

    /// Provides the arguments necessary to recreate the [SimpleMatcher] object during unpickling.
    ///
    /// This method is called by the Python pickling process and provides the serialized
    /// simple match type word map byte array, which is necessary to reconstruct the [SimpleMatcher]
    /// object.
    ///
    /// # Returns
    /// - `&[u8]`: A reference to the byte array containing the serialized simple match type word map data.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import SimpleMatcher
    /// from matcher_py.extension_types import SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// simple_match_type_word_map = msgpack_encoder.encode(
    ///     {
    ///         SimpleMatchType.MatchNone: {
    ///             1: "example"
    ///         }
    ///     }
    /// )
    ///
    /// simple_matcher = SimpleMatcher(simple_match_type_word_map)
    ///
    /// # Check the args returned for recreating the object
    /// serialized_args = simple_matcher.__getnewargs__()
    /// print(serialized_args)
    /// ```
    fn __getnewargs__(&self) -> (&[u8],) {
        (&self.simple_match_type_word_map_bytes,)
    }

    /// Serializes the state of the [SimpleMatcher] object for pickling.
    ///
    /// This method is called during the pickling process to capture the state of the [SimpleMatcher]
    /// instance. It returns a reference to the byte array containing the serialized simple match
    /// type word map data, which is used to reconstruct the object during unpickling.
    ///
    /// # Returns
    /// - `&[u8]`: A reference to the byte array representing the serialized simple match type word map.
    ///
    /// # Example
    ///
    /// ```python
    /// import pickle
    /// import msgspec
    ///
    /// from matcher_py import SimpleMatcher
    /// from matcher_py.extension_types import SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// simple_match_type_word_map = msgpack_encoder.encode(
    ///     {
    ///         SimpleMatchType.MatchNone: {
    ///             1: "example"
    ///         }
    ///     }
    /// )
    ///
    /// simple_matcher = SimpleMatcher(simple_match_type_word_map)
    ///
    /// # Serialize SimpleMatcher instance to a byte stream using pickle
    /// pickled_data = pickle.dumps(simple_matcher)
    /// print(pickled_data)
    /// ```
    fn __getstate__(&self) -> &[u8] {
        &self.simple_match_type_word_map_bytes
    }

    #[pyo3(signature=(simple_match_type_word_map_bytes))]
    /// Restores the state of the [SimpleMatcher] object from the provided serialized data.
    ///
    /// This method is called during the unpickling process to reinitialize the `simple_matcher`
    /// instance with the given serialized simple match type word map byte array. The byte array
    /// is deserialized into a [SimpleMatchTypeWordMapRs] and a new [SimpleMatcherRs] instance is
    /// created using the deserialized word map.
    ///
    /// # Parameters
    /// - `self`: The [SimpleMatcher] instance.
    /// - `simple_match_type_word_map_bytes`: A reference to a byte slice containing the serialized
    ///    simple match type word map data.
    ///
    /// # Example
    ///
    /// ```python
    /// import pickle
    /// import msgspec
    ///
    /// from matcher_py import SimpleMatcher
    /// from matcher_py.extension_types import SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// simple_match_type_word_map = msgpack_encoder.encode(
    ///     {
    ///         SimpleMatchType.MatchNone: {
    ///             1: "example"
    ///         }
    ///     }
    /// )
    ///
    /// simple_matcher = SimpleMatcher(simple_match_type_word_map)
    ///
    /// # Serialize and deserialize using pickle
    /// pickled_data = pickle.dumps(simple_matcher)
    /// deserialized_matcher = pickle.loads(pickled_data)
    ///
    /// # The deserialized object should have the same state
    /// assert deserialized_matcher.is_match("example")
    /// ```
    fn __setstate__(&mut self, simple_match_type_word_map_bytes: &[u8]) {
        self.simple_matcher = SimpleMatcherRs::new(
            &rmp_serde::from_slice::<SimpleMatchTypeWordMapRs>(simple_match_type_word_map_bytes)
                .unwrap(),
        );
    }

    #[pyo3(signature=(text))]
    /// Checks if the given text matches any of the patterns in the simple matcher.
    ///
    /// This method takes a string slice as input and invokes the `is_match` method on the internal
    /// `simple_matcher` instance. It returns a boolean indicating whether the text matches any of
    /// the patterns defined in the `simple_matcher`.
    ///
    /// # Parameters
    /// - `self`: The [SimpleMatcher] instance.
    /// - `text`: A reference to a string slice that will be checked against the patterns.
    ///
    /// # Returns
    /// - `bool`: `true` if the text matches any pattern; otherwise, `false`.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import SimpleMatcher
    /// from matcher_py.extension_types import SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// simple_matcher = SimpleMatcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             SimpleMatchType.MatchNone: {
    ///                 1: "example"
    ///             }
    ///         }
    ///     )
    /// )
    ///
    /// # Check if a given text matches any of the patterns
    /// is_match = simple_matcher.is_match("example")
    /// print(is_match)  # Output: True if "example" matches any pattern; otherwise, False
    /// ```
    fn is_match(&self, text: &str) -> bool {
        self.simple_matcher.is_match(text)
    }

    #[pyo3(signature=(text))]
    /// Performs simple processing on the given text and returns the results as a list of [SimpleResult] instances.
    ///
    /// This method takes a string slice as input, invokes the `process` method on the internal `simple_matcher`
    /// instance, and collects the resulting items into a vector of [SimpleResult] instances.
    ///
    /// # Parameters
    /// - `self`: The [SimpleMatcher] instance.
    /// - `text`: A reference to a string slice that will be processed.
    ///
    /// # Returns
    /// - `Vec<SimpleResult>`: A vector of [SimpleResult] instances representing the results of the simple processing.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import SimpleMatcher
    /// from matcher_py.extension_types import SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    ///
    /// simple_matcher = SimpleMatcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             SimpleMatchType.MatchNone: {
    ///                 1: "example"
    ///             }
    ///         }
    ///     )
    /// )
    ///
    /// results = simple_matcher.simple_process("example")
    /// print(results)  # Output: A list of SimpleResult instances
    /// ```
    fn simple_process(&self, text: &str) -> Vec<SimpleResult> {
        self.simple_matcher
            .process(text)
            .into_iter()
            .map(SimpleResult)
            .collect()
    }

    #[pyo3(signature=(text_array))]
    /// Batch processes a list of texts and performs simple processing
    /// on each text, returning the results as vectors of [SimpleResult] instances.
    ///
    /// This function iterates over a Python list of texts, performs simple processing
    /// on each text, and collects the results into a vector of vectors of [SimpleResult] instances.
    /// The result for each text is obtained by calling the [simple_process](SimpleMatcher::simple_process)
    /// method, producing a vector of [SimpleResult] instances for each text.
    ///
    /// # Parameters
    /// - `self`: The [SimpleMatcher] instance.
    /// - `text_array`: A reference to a [PyList] containing texts to be processed.
    ///
    /// # Returns
    /// - `PyResult<Vec<Vec<SimpleResult>>>`: A vector of vectors containing the simple processing results
    ///   for each text as vectors of [SimpleResult] instances.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
    /// from matcher_py import SimpleMatcher
    /// from matcher_py.extension_types import SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    /// simple_matcher = SimpleMatcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             SimpleMatchType.MatchNone: {
    ///                 1: "example"
    ///             }
    ///         }
    ///     )
    /// )
    ///
    /// text_list = ["example", "test", "example test"]
    /// result = simple_matcher.batch_simple_process(text_list)
    /// print(result)  # Output: A list of lists of SimpleResult instances
    /// ```
    fn batch_simple_process(
        &self,
        text_array: &Bound<'_, PyList>,
    ) -> PyResult<Vec<Vec<SimpleResult>>> {
        let mut result_list = Vec::with_capacity(text_array.len());

        for text in text_array.iter() {
            let text_py_string = text.downcast::<PyString>()?;
            result_list.push(self.simple_process(text_py_string.to_cow().as_ref().unwrap()));
        }

        Ok(result_list)
    }

    #[pyo3(signature=(text_array, inplace = false))]
    /// Processes a NumPy array of texts using the simple processing method,
    /// with an optional in-place operation. Each element of the input array
    /// is expected to be a Python string object.
    ///
    /// This function can either modify the input NumPy array in-place or return a
    /// new NumPy array with the processed results. The processing for each text
    /// is performed by the [simple_process](SimpleMatcher::simple_process) method,
    /// which returns [SimpleResult] instances.
    ///
    /// # Parameters
    /// - `self`: The [SimpleMatcher] instance.
    /// - `py`: The Python interpreter state, managed by the PyO3 library.
    /// - `text_array`: A reference to a NumPy array containing Python string objects
    ///   to be processed.
    /// - `inplace`: A boolean flag indicating whether the processing should be done
    ///   in-place. Defaults to `false`.
    ///
    /// # Returns
    /// - `Option<Py<PyArray1<PyObject>>>`: Returns `None` if `inplace` is `true`.
    ///   Otherwise, returns a new NumPy array with the processed results.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    /// import numpy as np
    ///
    /// from matcher_py import SimpleMatcher
    /// from matcher_py.extension_types import SimpleMatchType
    ///
    /// msgpack_encoder = msgspec.msgpack.Encoder()
    /// simple_matcher = SimpleMatcher(
    ///     msgpack_encoder.encode(
    ///         {
    ///             SimpleMatchType.MatchNone: {
    ///                 1: "example"
    ///             }
    ///         }
    ///     )
    /// )
    ///
    /// text_array = np.array(["example", "test", "example test"])
    /// result = simple_matcher.numpy_simple_process(text_array, inplace=False)
    /// print(result)  # Output: A NumPy array with lists of SimpleResult instances
    ///
    /// simple_matcher.numpy_simple_process(text_array, inplace=True)
    /// print(text_array)  # Output: The original NumPy array modified in-place with lists of SimpleResult instances
    /// ```
    fn numpy_simple_process(
        &self,
        py: Python,
        text_array: &Bound<'_, PyArray1<PyObject>>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                *text = text
                    .downcast_bound::<PyString>(py)
                    .map_or(py.None(), |text_py_string| {
                        self.simple_process(text_py_string.to_cow().as_ref().unwrap())
                            .into_py(py)
                    });
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    unsafe { text_array.as_array() }.map(|text| {
                        text.downcast_bound::<PyString>(py)
                            .map_or(py.None(), |text_py_string| {
                                self.simple_process(text_py_string.to_cow().as_ref().unwrap())
                                    .into_py(py)
                            })
                    }),
                )
                .into(),
            )
        }
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
