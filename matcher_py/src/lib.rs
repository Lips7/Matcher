#![allow(unsafe_op_in_unsafe_fn)]

use std::collections::HashMap;

use numpy::{PyArray1, PyArrayMethods};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::{pyclass, pymethods, pymodule, Py, PyModule, PyObject, PyResult, Python};
use pyo3::types::{
    PyAnyMethods, PyBytes, PyBytesMethods, PyDict, PyDictMethods, PyList, PyListMethods, PyString,
    PyStringMethods,
};
use pyo3::{intern, Bound, IntoPy, PyAny};

use matcher_rs::{
    MatchResult as MatchResultRs, MatchResultTrait, MatchTableMap as MatchTableMapRs,
    Matcher as MatcherRs, SimpleMatchTypeWordMap as SimpleMatchTypeWordMapRs,
    SimpleMatcher as SimpleMatcherRs, SimpleResult as SimpleResultRs, TextMatcherTrait,
};

/// A struct that wraps around the `SimpleResultRs` struct from the `matcher_rs` crate.
///
/// This struct serves as a bridge between the Rust and Python representations of match results.
/// It encapsulates a `SimpleResultRs` instance and provides necessary implementations
/// for conversion and trait compliance, facilitating its use in Python.
///
/// # Lifetime Parameters
/// - `'a`: The lifetime parameter that corresponds to the lifetime of the encapsulated
///   `SimpleResultRs` instance.
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
    /// Converts a `SimpleResult` instance into a Python dictionary (`PyObject`).
    ///
    /// This implementation of the `IntoPy` trait allows for converting a `SimpleResult`
    /// into a Python dictionary containing the match result data, which can be used
    /// in Python code. The dictionary includes the following key-value pairs:
    ///
    /// - `"word_id"`: The unique identifier (u64) for the matched word.
    /// - `"word"`: The matched word as a string slice.
    ///
    /// # Parameters
    /// - `self`: The `SimpleResult` instance to be converted.
    /// - `py`: The Python interpreter state.
    ///
    /// # Returns
    /// - `PyObject`: A Python dictionary containing the match result data.
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
    /// Converts a `MatchResult` instance into a Python dictionary (`PyObject`).
    ///
    /// This implementation of the `IntoPy` trait allows for converting a `MatchResult`
    /// into a Python dictionary containing the match result data, which can be used
    /// in Python code. The dictionary includes the following key-value pairs:
    ///
    /// - `"table_id"`: The unique identifier (u64) for the table.
    /// - `"word"`: The matched word as a string slice.
    ///
    /// # Parameters
    /// - `self`: The `MatchResult` instance to be converted.
    /// - `py`: The Python interpreter state.
    ///
    /// # Returns
    /// - `PyObject`: A Python dictionary containing the match result data.
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

#[pyclass(module = "matcher_py")]
/// A Python class that wraps the `MatcherRs` struct from the `matcher_rs` crate.
///
/// This class provides functionality for text matching using a deserialized match table map.
/// It allows for single text matching, batch text processing using Python lists, and batch
/// processing using NumPy arrays.
///
/// # Fields
/// - `matcher`: An instance of the `MatcherRs` struct that performs the core matching logic.
/// - `match_table_map_bytes`: A serialized byte array representing the match table map,
///   used for reconstructing the `MatcherRs` instance during deserialization.
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
///                     match_table_type=MatchTableType.Simple,
///                     simple_match_type=SimpleMatchType.MatchFanjianDeleteNormalize,
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
/// batch_results = matcher.batch_word_match_as_dict(text_list)
/// print(batch_results)
///
/// # Perform batch processing as a string using a list
/// text_list = ["hello", "world", "hello,word"]
/// batch_results = matcher.batch_word_match_as_string(text_list)
/// print(batch_results)
///
/// # Perform batch processing as a dict using a numpy array
/// text_array = np.array(["hello", "world", "hello,word"], dtype=np.dtype("object"))
/// numpy_results = matcher.numpy_word_match_as_dict(text_array)
/// print(numpy_results)
///
/// # Perform batch processing as a string using a numpy array
/// text_array = np.array(["hello", "world", "hello,word"], dtype=np.dtype("object"))
/// numpy_results = matcher.numpy_word_match_as_string(text_array)
/// print(numpy_results)
/// ```
struct Matcher {
    matcher: MatcherRs,
    match_table_map_bytes: Py<PyBytes>,
}

#[pymethods]
impl Matcher {
    #[new]
    /// Creates a new `Matcher` instance by deserializing the provided byte array
    /// into a `MatchTableMapRs` object and using it to initialize the `matcher`.
    ///
    /// This method attempts to deserialize the input byte array into a `MatchTableMapRs`
    /// object. If deserialization is successful, it initializes the `matcher` field with a
    /// new `MatcherRs` instance created from the deserialized `MatchTableMapRs` object.
    ///
    /// # Parameters
    /// - `_py`: The Python interpreter state.
    /// - `match_table_map_bytes`: A reference to a `PyBytes` object containing the
    ///   serialized byte array of the match table map.
    ///
    /// # Returns
    /// - `PyResult<Matcher>`: A result containing a new `Matcher` instance if deserialization
    ///   is successful, or a `PyValueError` if deserialization fails.
    ///
    /// # Errors
    /// - Returns a `PyValueError` if deserialization of the byte array fails. The error message
    ///   will include details about the failure.
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
    ///                     match_table_type=MatchTableType.Simple,
    ///                     simple_match_type=SimpleMatchType.MatchFanjianDeleteNormalize,
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    /// ```
    fn new(_py: Python, match_table_map_bytes: &Bound<'_, PyBytes>) -> PyResult<Matcher> {
        let match_table_map: MatchTableMapRs =
            match rmp_serde::from_slice(match_table_map_bytes.as_bytes()) {
                Ok(match_table_map) => match_table_map,
                Err(e) => {
                    return Err(PyValueError::new_err(format!(
                "Deserialize match_table_map_bytes failed, Please check the input data.\nErr: {}",
                e
            )))
                }
            };

        Ok(Matcher {
            matcher: MatcherRs::new(match_table_map),
            match_table_map_bytes: match_table_map_bytes.as_unbound().to_owned(),
        })
    }

    /// Returns the arguments needed to recreate the `Matcher` object during unpickling.
    ///
    /// This method is used for serialization support when pickling the `Matcher`
    /// instance in Python. It provides the byte array representing the match table map,
    /// which is necessary to reconstruct the `Matcher` object.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    ///
    /// # Returns
    /// - `Py<PyBytes>`: A reference to the `PyBytes` object containing the
    ///   serialized match table map data.
    fn __getnewargs__(&self, py: Python) -> Py<PyBytes> {
        self.match_table_map_bytes.clone_ref(py)
    }

    /// Returns the byte array needed to recreate the `Matcher` object during unpickling.
    ///
    /// This method is used for serialization support when pickling the `Matcher`
    /// instance in Python. It provides the byte array representing the match table map,
    /// which is necessary to reconstruct the `Matcher` object.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    ///
    /// # Returns
    /// - `Py<PyBytes>`: A reference to the `PyBytes` object containing the
    ///   serialized match table map data.
    fn __getstate__(&self, py: Python) -> Py<PyBytes> {
        self.match_table_map_bytes.clone_ref(py)
    }

    /// Reconstructs the `Matcher` object during unpickling.
    ///
    /// This method is called during the unpickling process to restore the state
    /// of the `Matcher` instance based on the provided byte array representing the
    /// match table map. It deserializes the byte array and re-initializes the `matcher`
    /// field with a new `MatcherRs` instance.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance to be re-initialized.
    /// - `_py`: The Python interpreter state.
    /// - `match_table_map_bytes`: A reference to a `PyBytes` object containing the serialized
    ///   byte array of the match table map.
    ///
    /// # Panics
    /// This method will panic if deserialization of the byte array fails.
    /// In practice, this means that the pickled object was corrupted or incompatible.
    fn __setstate__(&mut self, _py: Python, match_table_map_bytes: &Bound<'_, PyBytes>) {
        self.matcher = MatcherRs::new(
            rmp_serde::from_slice::<MatchTableMapRs>(match_table_map_bytes.as_bytes()).unwrap(),
        );
    }

    #[pyo3(signature=(text))]
    /// Checks if the given text matches using the `matcher` instance.
    ///
    /// This method attempts to downcast the provided `PyAny` object to a `PyString`.
    /// If the downcast is successful, it checks if the `matcher` instance considers the
    /// string to be a match. If the downcast fails, it returns `false`.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `_py`: The Python interpreter state.
    /// - `text`: A reference to a `PyAny` object which is expected to be a `PyString`.
    ///
    /// # Returns
    /// - `bool`: `true` if the text matches; `false` otherwise.
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
    ///                     match_table_type=MatchTableType.Simple,
    ///                     simple_match_type=SimpleMatchType.MatchFanjianDeleteNormalize,
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
    /// ```
    fn is_match(&self, _py: Python, text: &Bound<'_, PyAny>) -> bool {
        text.downcast::<PyString>().map_or(false, |text| {
            self.matcher
                .is_match(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
        })
    }

    #[pyo3(signature=(text))]
    /// Performs word matching on the given text and returns the results as a detailed
    /// dictionary containing `MatchResult` instances.
    ///
    /// This method attempts to downcast the provided `PyAny` object to a `PyString`.
    /// If the downcast is successful, it calls the `word_match_raw` method on the
    /// `matcher` instance, passing the text as a string slice, and returns the resulting
    /// dictionary with detailed match results. If the downcast fails, it returns an empty
    /// dictionary.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `_py`: The Python interpreter state.
    /// - `text`: A reference to a `PyAny` object which is expected to be a `PyString`.
    ///
    /// # Returns
    /// - `HashMap<u64, Vec<MatchResult<'_>>>`: A dictionary where keys are match IDs and
    ///   values are vectors of `MatchResult` instances. If the input `text` is not a `PyString`,
    ///   an empty dictionary is returned.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
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
    ///                     match_table_type=MatchTableType.Simple,
    ///                     simple_match_type=SimpleMatchType.MatchFanjianDeleteNormalize,
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Perform word matching
    /// result = matcher.word_match_raw("hello")
    /// print(result)  # Output: {1: [<matcher_py.MatchResult object at ...>]}
    /// ```
    fn word_match_raw(
        &self,
        _py: Python,
        text: &Bound<'_, PyAny>,
    ) -> HashMap<u64, Vec<MatchResult<'_>>> {
        text.downcast::<PyString>().map_or(HashMap::new(), |text| {
            self.matcher
                .word_match_raw(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
                .into_iter()
                .map(|(match_id, match_result_list)| {
                    (
                        match_id,
                        match_result_list.into_iter().map(MatchResult).collect(),
                    )
                })
                .collect()
        })
    }

    #[pyo3(signature=(text))]
    /// Performs word matching on the given text and returns the results as a dictionary.
    ///
    /// This method attempts to downcast the provided `PyAny` object to a `PyString`.
    /// If the downcast is successful, it calls the `word_match` method on the `matcher`
    /// instance, passing the text as a string slice, and returns the resulting dictionary.
    /// If the downcast fails, it returns an empty dictionary.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `_py`: The Python interpreter state.
    /// - `text`: A reference to a `PyAny` object which is expected to be a `PyString`.
    ///
    /// # Returns
    /// - `HashMap<&str, String>`: A dictionary containing the word match results.
    ///   If the input `text` is not a `PyString`, an empty dictionary is returned.
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
    ///                     match_table_type=MatchTableType.Simple,
    ///                     simple_match_type=SimpleMatchType.MatchFanjianDeleteNormalize,
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Perform word matching as a dict
    /// result = matcher.word_match("hello")
    /// print(result) Output: {1:"[{\"table_id\":1,\"word\":\"hello\"}]"}
    /// ```
    fn word_match(&self, _py: Python, text: &Bound<'_, PyAny>) -> HashMap<u64, String> {
        text.downcast::<PyString>().map_or(HashMap::new(), |text| {
            self.matcher
                .word_match(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
        })
    }

    #[pyo3(signature=(text))]
    /// Performs word matching on the given text and returns the results as a JSON string.
    ///
    /// This method attempts to downcast the provided `PyAny` object to a `PyString`.
    /// If the downcast is successful, it calls the `word_match_as_string` method on the `matcher`
    /// instance, passing the text as a string slice, and returns the resulting JSON string.
    /// If the downcast fails, it returns a JSON string representing an empty dictionary.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text`: A reference to a `PyAny` object which is expected to be a `PyString`.
    ///
    /// # Returns
    /// - `Py<PyString>`: A `PyString` containing the word match results as a JSON string.
    ///   If the input `text` is not a `PyString`, a JSON string representing an empty
    ///   dictionary is returned.
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
    ///                     match_table_type=MatchTableType.Simple,
    ///                     simple_match_type=SimpleMatchType.MatchFanjianDeleteNormalize,
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Perform word matching as a string
    /// result = matcher.word_match_as_string("hello")
    /// print(result) Output: '{1:"[{\\"table_id\\":1,\\"word\\":\\"hello\\"}]"}'
    /// ```
    fn word_match_as_string(&self, py: Python, text: &Bound<'_, PyAny>) -> Py<PyString> {
        text.downcast::<PyString>()
            .map_or(PyString::intern_bound(py, "{}"), |text| {
                PyString::intern_bound(
                    py,
                    &self
                        .matcher
                        .word_match_as_string(unsafe { text.to_cow().as_ref().unwrap_unchecked() }),
                )
            })
            .into()
    }

    #[pyo3(signature=(text_array))]
    /// Batch processes a list of texts and performs word matching on each text,
    /// returning the results as a dictionary for each text.
    ///
    /// This method iterates over a list of texts, performs word matching for each text,
    /// and collects the results into a new list. The result for each text is obtained
    /// by calling the `word_match` method, which returns a dictionary for each text.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyList` containing texts to be processed.
    ///
    /// # Returns
    /// - `Py<PyList>`: A `PyList` containing the word match results for each text,
    ///   represented as dictionaries.
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
    ///                     match_table_type=MatchTableType.Simple,
    ///                     simple_match_type=SimpleMatchType.MatchFanjianDeleteNormalize,
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Perform word matching as a dict
    /// text_array = ["hello", "world", "hello word"]
    /// result = matcher.batch_word_match_as_dict(text_array)
    /// print(result) # Output: [{1:"[{\"table_id\":1,\"word\":\"hello\"}]"}, ...]
    /// ```
    fn batch_word_match_as_dict(&self, py: Python, text_array: &Bound<'_, PyList>) -> Py<PyList> {
        let result_list = PyList::empty_bound(py);

        text_array.iter().for_each(|text| {
            result_list.append(self.word_match(py, &text)).unwrap();
        });

        result_list.into()
    }

    #[pyo3(signature=(text_array))]
    /// Batch processes a list of texts and performs word matching on each text,
    /// returning the results as a JSON string for each text.
    ///
    /// This method iterates over a list of texts, performs word matching for each text,
    /// and collects the results into a new list. The result for each text is obtained
    /// by calling the `word_match_as_string` method, which returns a JSON string for each text.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyList` containing texts to be processed.
    ///
    /// # Returns
    /// - `Py<PyList>`: A `PyList` containing the word match results for each text
    ///   as JSON strings.
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
    ///                     match_table_type=MatchTableType.Simple,
    ///                     simple_match_type=SimpleMatchType.MatchFanjianDeleteNormalize,
    ///                     word_list=["hello", "world"],
    ///                     exemption_simple_match_type=SimpleMatchType.MatchNone,
    ///                     exemption_word_list=["word"],
    ///                 )
    ///             ]
    ///         }
    ///     )
    /// )
    ///
    /// # Perform word matching as a string for a batch of texts
    /// text_array = ["hello", "world", "hello word"]
    /// result = matcher.batch_word_match_as_string(text_array)
    /// print(result)  # Output: ['{1:"[{\\"table_id\\":1,\\"word\\":\\"hello\\"}]"}', ...]
    /// ```
    fn batch_word_match_as_string(&self, py: Python, text_array: &Bound<'_, PyList>) -> Py<PyList> {
        let result_list = PyList::empty_bound(py);

        text_array.iter().for_each(|text| {
            result_list
                .append(self.word_match_as_string(py, &text))
                .unwrap();
        });

        result_list.into()
    }

    #[pyo3(signature=(text_array, inplace = false))]
    /// Batch processes a NumPy 1-D array of texts and performs word matching
    /// on each text, returning the results as dictionaries.
    ///
    /// This function iterates over a NumPy 1-D array of texts, performs word matching
    /// on each text, and collects the results into a new NumPy array or modifies the
    /// original array in-place based on the `inplace` parameter. If `inplace` is set to `true`,
    /// the original array is modified directly. The result for each text is obtained by
    /// calling the `word_match` method, producing a dictionary for each text.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyArray1` containing texts to be processed.
    /// - `inplace`: A boolean flag indicating whether to modify the original array in-place.
    ///
    /// # Returns
    /// - `Option<Py<PyArray1<PyObject>>>`: If `inplace` is `false`, a new `PyArray1` containing
    ///   the word match results for each text as dictionaries. If `inplace` is `true`, returns
    ///   `None` as the original array is modified in-place.
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
    ///                     match_table_type=MatchTableType.Simple,
    ///                     simple_match_type=SimpleMatchType.MatchFanjianDeleteNormalize,
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
    /// result = matcher.numpy_word_match_as_dict(text_array)
    /// print(result)  # Output: A new NumPy array with word match results as dictionaries
    ///
    /// inplace_result = matcher.numpy_word_match_as_dict(text_array, inplace=True)
    /// print(text_array)  # Output: The original NumPy array modified with word match results
    /// ```
    fn numpy_word_match_as_dict(
        &self,
        py: Python,
        text_array: &Bound<'_, PyArray1<PyObject>>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                *text = self.word_match(py, text.bind(py)).into_py(py);
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    unsafe { text_array.as_array() }
                        .map(|text| self.word_match(py, text.bind(py)).into_py(py)),
                )
                .into(),
            )
        }
    }

    #[pyo3(signature=(text_array, inplace = false))]
    /// Batch processes a NumPy 1-D array of texts and performs word matching
    /// on each text, returning the results as JSON strings.
    ///
    /// This function iterates over a NumPy 1-D array of texts, performs word matching
    /// on each text, and collects the results into a new NumPy array or modifies the
    /// original array in-place based on the `inplace` parameter. If `inplace` is set to `true`,
    /// the original array is modified directly. The result for each text is obtained by
    /// calling the `word_match_as_string` method, producing a JSON string for each text.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyArray1` containing texts to be processed.
    /// - `inplace`: A boolean flag indicating whether to modify the original array in-place.
    ///
    /// # Returns
    /// - `Option<Py<PyArray1<PyObject>>>`: If `inplace` is `false`, a new `PyArray1` containing
    ///   the word match results for each text as JSON strings. If `inplace` is `true`, returns
    ///   `None` as the original array is modified in-place.
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
    ///                     match_table_type=MatchTableType.Simple,
    ///                     simple_match_type=SimpleMatchType.MatchFanjianDeleteNormalize,
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
    /// result = matcher.numpy_word_match_as_string(text_array)
    /// print(result)  # Output: A new NumPy array with word match results as JSON strings
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
                *text = self.word_match_as_string(py, text.bind(py)).into_py(py);
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    unsafe { text_array.as_array() }
                        .map(|text| self.word_match_as_string(py, text.bind(py)).into_py(py)),
                )
                .into(),
            )
        }
    }
}

#[pyclass(module = "matcher_py")]
/// A Python class that wraps the `SimpleMatcherRs` struct from the `matcher_rs` crate.
///
/// This class provides functionality for simple text matching using a serialized
/// type word map. It enables single text matching and batch text processing
/// using both Python lists and NumPy arrays.
///
/// # Fields
/// - `simple_matcher`: An instance of the `SimpleMatcherRs` struct which performs
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
    simple_match_type_word_map_bytes: Py<PyBytes>,
}

#[pymethods]
impl SimpleMatcher {
    #[new]
    /// Creates a new `SimpleMatcher` instance by deserializing the provided byte array
    /// into a `SimpleMatchTypeWordMapRs` object and using it to initialize the `simple_matcher`.
    ///
    /// This method attempts to deserialize the input byte array into a `SimpleMatchTypeWordMapRs`
    /// object. If deserialization is successful, it initializes the `simple_matcher` field with a
    /// new `SimpleMatcherRs` instance created from the deserialized `SimpleMatchTypeWordMapRs` object.
    ///
    /// # Parameters
    /// - `_py`: The Python interpreter state.
    /// - `simple_match_type_word_map_bytes`: A reference to a `PyBytes` object containing the
    ///   serialized byte array of the simple match type word map.
    ///
    /// # Returns
    /// - `PyResult<SimpleMatcher>`: A result containing a new `SimpleMatcher` instance if deserialization
    ///   is successful, or a `PyValueError` if deserialization fails.
    ///
    /// # Errors
    /// - Returns a `PyValueError` if deserialization of the byte array fails. The error message
    ///   will include details about the failure.
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
    /// ```
    fn new(
        _py: Python,
        simple_match_type_word_map_bytes: &Bound<'_, PyBytes>,
    ) -> PyResult<SimpleMatcher> {
        let simple_match_type_word_map: SimpleMatchTypeWordMapRs =
            match rmp_serde::from_slice(simple_match_type_word_map_bytes.as_bytes()) {
                Ok(simple_match_type_word_map) => simple_match_type_word_map,
                Err(e) => return Err(PyValueError::new_err(
                    format!("Deserialize simple_match_type_word_map_bytes failed, Please check the input data.\n Err: {}", e),
                )),
            };

        Ok(SimpleMatcher {
            simple_matcher: SimpleMatcherRs::new(simple_match_type_word_map),
            simple_match_type_word_map_bytes: simple_match_type_word_map_bytes
                .as_unbound()
                .to_owned(),
        })
    }

    /// Returns the arguments needed to recreate the `SimpleMatcher` object during unpickling.
    ///
    /// This method is used for serialization support when pickling the `SimpleMatcher`
    /// instance in Python. It provides the byte array representing the simple match type word map,
    /// which is necessary to reconstruct the `SimpleMatcher` object.
    ///
    /// # Parameters
    /// - `self`: The `SimpleMatcher` instance.
    /// - `py`: The Python interpreter state.
    ///
    /// # Returns
    /// - `Py<PyBytes>`: A reference to the `PyBytes` object containing the
    ///   serialized simple match type word map data.
    fn __getnewargs__(&self, py: Python) -> Py<PyBytes> {
        self.simple_match_type_word_map_bytes.clone_ref(py)
    }

    /// Returns the byte array needed to recreate the `SimpleMatcher` object during unpickling.
    ///
    /// This method is used for serialization support when pickling the `SimpleMatcher`
    /// instance in Python. It provides the byte array representing the simple match type word map,
    /// which is necessary to reconstruct the `SimpleMatcher` object.
    ///
    /// # Parameters
    /// - `self`: The `SimpleMatcher` instance.
    /// - `py`: The Python interpreter state.
    ///
    /// # Returns
    /// - `Py<PyBytes>`: A reference to the `PyBytes` object containing the
    ///   serialized simple match type word map data.
    fn __getstate__(&self, py: Python) -> Py<PyBytes> {
        self.simple_match_type_word_map_bytes.clone_ref(py)
    }

    /// Reconstructs the `SimpleMatcher` object during unpickling.
    ///
    /// This method is called during the unpickling process to restore the state
    /// of the `SimpleMatcher` instance based on the provided byte array representing the
    /// simple match type word map. It deserializes the byte array and re-initializes the
    /// `simple_matcher` field with a new `SimpleMatcherRs` instance.
    ///
    /// # Parameters
    /// - `self`: The `SimpleMatcher` instance to be re-initialized.
    /// - `_py`: The Python interpreter state.
    /// - `simple_match_type_word_map_bytes`: A reference to a `PyBytes` object containing the
    ///   serialized byte array of the simple match type word map.
    ///
    /// # Panics
    /// This method will panic if deserialization of the byte array fails. Typically, this means
    /// that the pickled object was corrupted or is incompatible.
    fn __setstate__(&mut self, _py: Python, simple_match_type_word_map_bytes: &Bound<'_, PyBytes>) {
        self.simple_matcher = SimpleMatcherRs::new(
            rmp_serde::from_slice::<SimpleMatchTypeWordMapRs>(
                simple_match_type_word_map_bytes.as_bytes(),
            )
            .unwrap(),
        );
    }

    #[pyo3(signature=(text))]
    /// Checks if the given text matches any of the predefined words in the `simple_matcher` instance.
    ///
    /// This method attempts to downcast the provided `PyAny` object to a `PyString`.
    /// If the downcast is successful, it checks if the `simple_matcher` instance considers the
    /// string to be a match. If the downcast fails, it returns `false`.
    ///
    /// # Parameters
    /// - `self`: The `SimpleMatcher` instance.
    /// - `_py`: The Python interpreter state.
    /// - `text`: A reference to a `PyAny` object which is expected to be a `PyString`.
    ///
    /// # Returns
    /// - `bool`: `true` if the text matches any of the predefined words; `false` otherwise.
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
    /// # Check if a text matches
    /// assert simple_matcher.is_match("example")
    /// assert not simple_matcher.is_match("test")
    /// ```
    fn is_match(&self, _py: Python, text: &Bound<'_, PyAny>) -> bool {
        text.downcast::<PyString>().map_or(false, |text| {
            self.simple_matcher
                .is_match(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
        })
    }

    #[pyo3(signature=(text))]
    /// Performs simple text processing on the given text and returns the results as a list.
    ///
    /// This method attempts to downcast the provided `PyAny` object to a `PyString`.
    /// If the downcast is successful, it calls the `process` method on the `simple_matcher`
    /// field, passing the text as a string slice, and collects the results into a new `PyList`.
    /// Each result is converted to a `SimpleResult` instance and appended to the list.
    /// If the downcast fails, it returns an empty `PyList`.
    ///
    /// # Parameters
    /// - `self`: The `SimpleMatcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text`: A reference to a `PyAny` object which is expected to be a `PyString`.
    ///
    /// # Returns
    /// - `Py<PyList>`: A `PyList` containing the simple processing results.
    ///   If the input `text` is not a `PyString`, an empty `PyList` is returned.
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
    /// # Perform simple processing on a single text
    /// result = simple_matcher.simple_process("example")
    /// print(result)  # Output: A list with the simple processing results
    /// ```
    fn simple_process(&self, py: Python, text: &Bound<'_, PyAny>) -> Py<PyList> {
        text.downcast::<PyString>()
            .map_or(PyList::empty_bound(py).into(), |text| {
                let result_list = PyList::empty_bound(py);
                self.simple_matcher
                    .process(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
                    .into_iter()
                    .for_each(|simple_result| {
                        result_list
                            .append(SimpleResult(simple_result).into_py(py))
                            .unwrap()
                    });
                result_list.into()
            })
    }

    #[pyo3(signature=(text_array))]
    /// Batch processes a list of texts and performs simple processing on each text,
    /// returning the results as a list of lists.
    ///
    /// This method iterates over a list of texts, performs simple processing for each text,
    /// and collects the results into a new list. The result for each text is obtained
    /// by calling the `simple_process` method, which returns a list of `SimpleResult` instances
    /// for each text. These lists are then appended to a new `PyList`.
    ///
    /// # Parameters
    /// - `self`: The `SimpleMatcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyList` containing texts to be processed.
    ///
    /// # Returns
    /// - `Py<PyList>`: A `PyList` containing the simple processing results for each text,
    ///   represented as lists.
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
    /// # Perform simple processing as a batch
    /// text_array = ["example", "test", "example test"]
    /// results = simple_matcher.batch_simple_process(text_array)
    /// print(results)
    /// ```
    fn batch_simple_process(&self, py: Python, text_array: &Bound<'_, PyList>) -> Py<PyList> {
        let result_list = PyList::empty_bound(py);

        text_array.iter().for_each(|text| {
            result_list.append(self.simple_process(py, &text)).unwrap();
        });

        result_list.into()
    }

    #[pyo3(signature=(text_array, inplace = false))]
    /// Batch processes a NumPy 1-D array of texts and performs simple processing
    /// on each text, returning the results as lists of `SimpleResult` instances.
    ///
    /// This function iterates over a NumPy 1-D array of texts, performs simple processing
    /// on each text, and collects the results into a new NumPy array or modifies the
    /// original array in-place based on the `inplace` parameter. If `inplace` is set to `true`,
    /// the original array is modified directly. The result for each text is obtained by
    /// calling the `simple_process` method, producing a list of `SimpleResult` instances for each text.
    ///
    /// # Parameters
    /// - `self`: The `SimpleMatcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyArray1` containing texts to be processed.
    /// - `inplace`: A boolean flag indicating whether to modify the original array in-place.
    ///
    /// # Returns
    /// - `Option<Py<PyArray1<PyObject>>>`: If `inplace` is `false`, a new `PyArray1` containing
    ///   the simple processing results for each text as lists of `SimpleResult` instances. If `inplace` is `true`, returns
    ///   `None` as the original array is modified in-place.
    ///
    /// # Example
    ///
    /// ```python
    /// import msgspec
    ///
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
    /// text_array = np.array(["example", "test", "example test"], dtype=np.dtype("object"))
    /// result = simple_matcher.numpy_simple_process(text_array)
    /// print(result)  # Output: A new NumPy array with simple processing results as lists of `SimpleResult` instances
    ///
    /// inplace_result = simple_matcher.numpy_simple_process(text_array, inplace=True)
    /// print(text_array)  # Output: The original NumPy array modified with simple processing results
    /// ```
    fn numpy_simple_process(
        &self,
        py: Python,
        text_array: &Bound<'_, PyArray1<PyObject>>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                *text = self.simple_process(py, text.bind(py)).into_any();
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    unsafe { text_array.as_array() }
                        .map(|text| self.simple_process(py, text.bind(py)).into_any()),
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
    Ok(())
}
