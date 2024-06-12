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
    MatchResultTrait, MatchTableMap as MatchTableMapRs, Matcher as MatcherRs,
    SimpleMatchTypeWordMap as SimpleMatchTypeWordMapRs, SimpleMatcher as SimpleMatcherRs,
    SimpleResult as SimpleResultRs, TextMatcherTrait,
};

/// A wrapper struct for the `SimpleResultRs` type from the `matcher_rs` crate.
///
/// # Parameters
/// - `'a`: A lifetime parameter, ensuring that the `SimpleResultRs` instance
///   tied to this struct does not outlive its bound.
///
/// This struct is used internally within the `SimpleMatcher` class to encapsulate
/// the results produced by the simple matching operations.
///
/// The `SimpleResult` struct provides a convenient way to work with
/// `SimpleResultRs` within the Python bindings.
struct SimpleResult<'a>(SimpleResultRs<'a>);

impl<'a> IntoPy<PyObject> for SimpleResult<'a> {
    /// Converts a `SimpleResult` instance into a Python dictionary.
    ///
    /// This method is used for converting a `SimpleResult` instance from
    /// Rust into a Python dictionary (`PyObject`).
    ///
    /// The resulting Python dictionary will have the following keys:
    /// - "word_id": Corresponds to the `word_id` field of the `SimpleResultRs` instance.
    /// - "word": Corresponds to the `word` field of the `SimpleResultRs` instance.
    ///
    /// # Parameters
    /// - `self`: The `SimpleResult` instance to be converted.
    /// - `py`: The Python interpreter state.
    ///
    /// # Returns
    /// A `PyObject` representing the `SimpleResult` as a Python dictionary.
    ///
    /// # Panics
    /// This method will panic if setting an item in the dictionary fails.
    ///
    /// # Examples
    /// ``` no_run
    /// use pyo3::prelude::*;
    /// use pyo3::types::{PyDict, PyObject};
    ///
    /// impl<'a> IntoPy<PyObject> for SimpleResult<'a> {
    ///     fn into_py(self, py: Python<'_>) -> PyObject {
    ///         let dict = PyDict::new_bound(py);
    ///
    ///         dict.set_item(intern!(py, "word_id"), self.0.word_id)
    ///             .unwrap();
    ///         dict.set_item(intern!(py, "word"), self.0.word.as_ref())
    ///             .unwrap();
    ///
    ///         dict.into()
    ///     }
    /// }
    /// ```
    fn into_py(self, py: Python<'_>) -> PyObject {
        // Create a new Python dictionary in the specified Python interpreter state.
        let dict = PyDict::new_bound(py);

        // Set the "word_id" key in the dictionary to the word_id field of the SimpleResultRs instance.
        dict.set_item(intern!(py, "word_id"), self.0.word_id)
            .unwrap();

        // Set the "word" key in the dictionary to the word field of the SimpleResultRs instance.
        dict.set_item(intern!(py, "word"), self.0.word.as_ref())
            .unwrap();

        // Convert the Python dictionary into a PyObject and return it.
        dict.into()
    }
}

impl MatchResultTrait<'_> for SimpleResult<'_> {
    /// Implementation of the `MatchResultTrait` for the `SimpleResult` struct.
    ///
    /// This implementation provides access to the `word_id` and `word` fields
    /// of the `SimpleResultRs` instance encapsulated by `SimpleResult`.
    ///
    /// # Methods
    /// - `word_id(&self) -> u64`: Returns the `word_id` associated with the result.
    /// - `word(&self) -> &str`: Returns the `word` associated with the result.
    ///
    /// # Return Values
    /// - `word_id(&self) -> u64`: The unique identifier for the matched word.
    /// - `word(&self) -> &str`: The matched word as a string slice.
    fn word_id(&self) -> u64 {
        self.0.word_id()
    }
    fn word(&self) -> &str {
        self.0.word.as_ref()
    }
}

#[pyclass(module = "matcher_py")]
/// A Python class that wraps the `MatcherRs` struct from the `matcher_rs` crate.
///
/// This class provides methods to perform matching operations using a precomputed
/// match table, which is deserialized from a byte array. It allows checking
/// whether a given text matches, performing word matches, and batching these
/// operations for arrays of text inputs.
///
/// # Fields
/// - `matcher`: An instance of the `MatcherRs` struct, which performs the core matching logic.
/// - `match_table_map_bytes`: A serialized byte array representing the match table map, used for
///   reconstructing the `MatcherRs` instance during deserialization.
///
/// # Examples
/// Basic usage:
///
/// ```python
/// from matcher_py import Matcher
/// from some_byte_source import get_match_table_map_bytes
///
/// # Create a new Matcher instance by providing match_table_map_bytes
/// matcher = Matcher(get_match_table_map_bytes())
///
/// # Check if a text matches
/// result = matcher.is_match("some text")
///
/// # Perform word matching
/// word_result = matcher.word_match("some text")
///
/// # Batch process word matching for a list of texts
/// batch_result = matcher.batch_word_match_as_dict(["text1", "text2"])
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
    /// # Parameters
    /// - `_py`: The Python interpreter state.
    /// - `match_table_map_bytes`: A reference to a `PyBytes` object containing the serialized
    ///   byte array of the match table map.
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
    /// ```python
    /// from matcher_py import Matcher
    /// from some_byte_source import get_match_table_map_bytes
    ///
    /// # Create a new Matcher instance by providing match_table_map_bytes
    /// matcher = Matcher(get_match_table_map_bytes())
    /// ```
    fn new(_py: Python, match_table_map_bytes: &Bound<'_, PyBytes>) -> PyResult<Matcher> {
        // Deserialize the provided byte array into a `MatchTableMapRs` object.
        let match_table_map: MatchTableMapRs =
            // Use `rmp_serde` to deserialize the byte array, retrieving the match table map.
            match rmp_serde::from_slice(match_table_map_bytes.as_bytes()) {
                // If deserialization is successful, assign the resulting object to `match_table_map`.
                Ok(match_table_map) => match_table_map,

                // If deserialization fails, return a `PyValueError` with a detailed error message.
                Err(e) => {
                    return Err(PyValueError::new_err(format!(
                "Deserialize match_table_map_bytes failed, Please check the input data.\nErr: {}",
                e.to_string()
            )))
                }
            };

        // Return a new `Matcher` instance initialized with the deserialized `MatchTableMapRs` object.
        Ok(Matcher {
            // Initialize the `matcher` field with a new `MatcherRs` instance.
            matcher: MatcherRs::new(&match_table_map),

            // Clone the provided byte array reference and assign it to `match_table_map_bytes`.
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
        self.matcher =
            MatcherRs::new(&rmp_serde::from_slice(match_table_map_bytes.as_bytes()).unwrap());
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
    fn is_match(&self, _py: Python, text: &Bound<'_, PyAny>) -> bool {
        // Attempt to downcast the provided PyAny reference to a PyString.
        text.downcast::<PyString>().map_or(
            // If downcasting fails, return false.
            false,
            // If downcasting succeeds, check if the matcher instance considers the string a match.
            |text| {
                self.matcher
                    // Call the is_match method on the matcher instance, passing the text as a string slice.
                    .is_match(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
            },
        )
    }

    #[pyo3(signature=(text))]
    /// Performs a word match on the given text using the `matcher` instance.
    ///
    /// This method attempts to downcast the provided `PyAny` object to a `PyString`.
    /// If the downcast is successful, it calls the `word_match` method on the `matcher`
    /// instance, passing the text as a string slice.
    /// If the downcast fails, it returns an empty `HashMap`.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `_py`: The Python interpreter state.
    /// - `text`: A reference to a `PyAny` object which is expected to be a `PyString`.
    ///
    /// # Returns
    /// - `HashMap<&str, String>`: A `HashMap` containing the word match results.
    ///   If the input `text` is not a `PyString`, an empty `HashMap` is returned.
    fn word_match(&self, _py: Python, text: &Bound<'_, PyAny>) -> HashMap<&str, String> {
        // Attempt to downcast the provided `PyAny` reference to a `PyString`.
        text.downcast::<PyString>()
            // If downcasting fails, return an empty `HashMap`.
            .map_or(
                HashMap::new(),
                // If downcasting succeeds, perform word matching using `matcher`.
                |text| {
                    self.matcher
                        // Call the `word_match` method on the `matcher` instance,
                        // passing the text as a string slice.
                        .word_match(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
                },
            )
    }

    #[pyo3(signature=(text))]
    /// Converts the word match result of the given text to a JSON string.
    ///
    /// This method attempts to downcast the provided `PyAny` object to a `PyString`.
    /// If the downcast is successful, it calls the `word_match_as_string` method on the `matcher`
    /// instance, passing the text as a string slice, and returns the resulting JSON string as a
    /// `PyString`. If the downcast fails, it returns an empty JSON object represented as a
    /// `PyString`.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text`: A reference to a `PyAny` object which is expected to be a `PyString`.
    ///
    /// # Returns
    /// - `Py<PyString>`: A `PyString` containing the JSON representation of the word match result.
    ///   If the input `text` is not a `PyString`, an empty JSON object (`{}`) is returned.
    ///
    /// # Example
    /// ```python
    /// from matcher_py import Matcher
    /// matcher = Matcher(get_match_table_map_bytes())
    /// result = matcher.word_match_as_string("some text")
    /// print(result)  # Output: JSON string
    /// ```
    fn word_match_as_string(&self, py: Python, text: &Bound<'_, PyAny>) -> Py<PyString> {
        // Attempt to downcast the provided `PyAny` reference to a `PyString`.
        text.downcast::<PyString>()
            // If downcasting fails, create an interned `PyString` representing an empty JSON object (`{}`).
            .map_or(PyString::intern_bound(py, "{}"), |text| {
                // If downcasting succeeds, perform word matching using `matcher` and convert the result to a JSON string.
                PyString::intern_bound(
                    py,
                    &self
                        .matcher
                        // Call the `word_match_as_string` method on the `matcher` instance,
                        // passing the text as a string slice and converting the result to a JSON string.
                        .word_match_as_string(unsafe { text.to_cow().as_ref().unwrap_unchecked() }),
                )
            })
            // Convert the resulting `PyString` to a `PyObject` and return it.
            .into()
    }

    #[pyo3(signature=(text_array))]
    /// Batch processes a list of texts and performs word matching on each text.
    ///
    /// This method iterates over a list of texts, performs word matching for each text,
    /// and collects the results into a new list. The result for each text is obtained
    /// by calling the `word_match` method, producing a `HashMap<&str, String>` for each text.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyList` containing texts to be processed.
    ///
    /// # Returns
    /// - `Py<PyList>`: A `PyList` containing the word match results for each text.
    ///
    /// # Example
    /// ```python
    /// from matcher_py import Matcher
    /// matcher = Matcher(get_match_table_map_bytes())
    /// text_list = ["text1", "text2", "text3"]
    /// result = matcher.batch_word_match_as_dict(text_list)
    /// print(result)  # Output: A list of dictionaries with word match results
    /// ```
    fn batch_word_match_as_dict(&self, py: Python, text_array: &Bound<'_, PyList>) -> Py<PyList> {
        // Create an empty Python list within the specified Python interpreter state.
        let result_list = PyList::empty_bound(py);

        // Iterate over each item in the provided PyList reference.
        text_array.iter().for_each(|text| {
            // Append the result of the word_match method to the result_list.
            result_list.append(self.word_match(py, &text)).unwrap();
        });

        // Return the populated result_list as a Python object.
        result_list.into()
    }

    #[pyo3(signature=(text_array))]
    /// Batch processes a list of texts and performs word matching on each text,
    /// returning the results as JSON strings.
    ///
    /// This method iterates over a list of texts, performs word matching for each text,
    /// and collects the results into a new list. The result for each text is obtained
    /// by calling the `word_match_as_string` method, which returns the match results
    /// as a JSON string.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyList` containing texts to be processed.
    ///
    /// # Returns
    /// - `Py<PyList>`: A `PyList` containing the word match results for each text,
    ///   represented as JSON strings.
    ///
    /// # Example
    /// ```python
    /// from matcher_py import Matcher
    /// matcher = Matcher(get_match_table_map_bytes())
    /// text_list = ["text1", "text2", "text3"]
    /// result = matcher.batch_word_match_as_string(text_list)
    /// print(result)  # Output: A list of JSON strings with word match results
    /// ```
    fn batch_word_match_as_string(&self, py: Python, text_array: &Bound<'_, PyList>) -> Py<PyList> {
        // Create an empty Python list within the specified Python interpreter state.
        let result_list = PyList::empty_bound(py);

        // Iterate over each item in the provided PyList reference.
        text_array.iter().for_each(|text| {
            // Append the result of the word_match_as_string method to the result_list.
            result_list
                .append(self.word_match_as_string(py, &text))
                .unwrap();
        });

        // Return the populated result_list as a Python object.
        result_list.into()
    }

    #[pyo3(signature=(text_array, inplace = false))]
    /// Batch processes a NumPy 1-D array of texts and performs word matching
    /// on each text, returning the results as a new NumPy array or modifying
    /// the original array in-place.
    ///
    /// This method iterates over a NumPy 1-D array of texts, performs word matching
    /// for each text, and collects the results into a new NumPy array. If `inplace` is
    /// set to `true`, the original array is modified directly. The result for each text
    /// is obtained by calling the `word_match` method, producing a `HashMap<&str, String>`
    /// for each text.
    ///
    /// # Parameters
    /// - `self`: The `Matcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyArray1` containing texts to be processed.
    /// - `inplace`: A boolean flag indicating whether to modify the original array in-place.
    ///
    /// # Returns
    /// - `Option<Py<PyArray1<PyObject>>>`: If `inplace` is `false`, a new `PyArray1` containing
    ///   the word match results for each text. If `inplace` is `true`, returns `None` as the original
    ///   array is modified in-place.
    ///
    /// # Example
    /// ```python
    /// from matcher_py import Matcher
    /// import numpy as np
    ///
    /// matcher = Matcher(get_match_table_map_bytes())
    /// text_array = np.array(["text1", "text2", "text3"], dtype=np.object)
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
        // Check if the operation is in-place.
        if inplace {
            // Unsafe block to mutate the elements of the numpy array in place.
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                // Assign the result of `word_match` to each element, converting it into a Python object.
                *text = self.word_match(py, text.bind(py)).into_py(py);
            });
            // Since the operation is in-place, return `None`.
            None
        } else {
            // Create and return a new numpy array from the results of `word_match`.
            Some(
                // Create a new numpy array from the owned array elements.
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    // Unsafe block to read the elements of the numpy array.
                    unsafe { text_array.as_array() }
                        // Map each element to the result of `word_match`, converting it into a Python object.
                        .map(|text| self.word_match(py, &text.bind(py)).into_py(py)),
                )
                .into(),
            )
        }
    }

    #[pyo3(signature=(text_array, inplace = false))]
    /// Batch processes a NumPy 1-D array of texts and performs word matching
    /// on each text, returning the results as JSON strings.
    ///
    /// This method iterates over a NumPy 1-D array of texts, performs word matching
    /// for each text, and collects the results into a new NumPy array. If `inplace` is
    /// set to `true`, the original array is modified directly. The result for each text
    /// is obtained by calling the `word_match_as_string` method, producing a JSON string
    /// for each text.
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
    /// ```python
    /// from matcher_py import Matcher
    /// import numpy as np
    ///
    /// matcher = Matcher(get_match_table_map_bytes())
    /// text_array = np.array(["text1", "text2", "text3"], dtype=np.object)
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
        // Check if the operation should be performed in-place.
        if inplace {
            // Perform an unsafe mutable access to the elements of the NumPy array.
            unsafe { text_array.as_array_mut() }
                // Use `map_inplace` to apply a function to each element.
                .map_inplace(|text| {
                    // For each element, call `word_match_as_string`, bind it to Python,
                    // convert the result to a Python object and modify the element in-place.
                    *text = self.word_match_as_string(py, &text.bind(py)).into_py(py);
                });
            // As we modified the original array in-place, return None.
            None
        } else {
            // If not in-place, create a new NumPy array from the results of `word_match_as_string`.
            Some(
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    // Perform an unsafe immutable access to the elements of the NumPy array.
                    unsafe { text_array.as_array() }
                        // Map each element to the result of `word_match_as_string`,
                        // bind it to Python, and convert it to a Python object.
                        .map(|text| self.word_match_as_string(py, &text.bind(py)).into_py(py)),
                )
                // Convert the resulting NumPy array to a Python object and return it.
                .into(),
            )
        }
    }
}

#[pyclass(module = "matcher_py")]
/// A Python class that wraps the `SimpleMatcherRs` struct from the `matcher_rs` crate.
///
/// This class provides methods for simple text matching operations using a predefined
/// set of word mappings, which are deserialized from a byte array. It allows performing
/// various matching methods, including single text matches, batch processing, and processing
/// using NumPy arrays.
///
/// # Fields
/// - `simple_matcher`: An instance of the `SimpleMatcherRs` struct that performs the core simple matching logic.
/// - `simple_match_type_word_map_bytes`: A serialized byte array representing the simple match type word map,
///   used for reconstructing the `SimpleMatcherRs` instance during deserialization.
///
/// # Examples
/// Basic usage:
///
/// ```python
/// from matcher_py import SimpleMatcher
/// from some_byte_source import get_simple_match_type_word_map_bytes
///
/// # Create a new SimpleMatcher instance by providing simple_match_type_word_map_bytes
/// simple_matcher = SimpleMatcher(get_simple_match_type_word_map_bytes())
///
/// # Check if a text matches
/// result = simple_matcher.is_match("some text")
///
/// # Process a text and get simple match results
/// simple_results = simple_matcher.simple_process("some text")
///
/// # Batch process texts using a list
/// text_list = ["text1", "text2"]
/// batch_results = simple_matcher.batch_simple_process(text_list)
///
/// # Batch process texts using a NumPy array
/// import numpy as np
/// text_array = np.array(["text1", "text2"], dtype=np.object)
/// numpy_results = simple_matcher.numpy_simple_process(text_array)
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
    /// # Parameters
    /// - `_py`: The Python interpreter state.
    /// - `simple_match_type_word_map_bytes`: A reference to a `PyBytes` object containing the serialized
    ///   byte array of the simple match type word map.
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
    /// ```python
    /// from matcher_py import SimpleMatcher
    /// from some_byte_source import get_simple_match_type_word_map_bytes
    ///
    /// # Create a new SimpleMatcher instance by providing simple_match_type_word_map_bytes
    /// simple_matcher = SimpleMatcher(get_simple_match_type_word_map_bytes())
    /// ```
    fn new(
        _py: Python,
        simple_match_type_word_map_bytes: &Bound<'_, PyBytes>,
    ) -> PyResult<SimpleMatcher> {
        // Attempt to deserialize the provided byte array into a `SimpleMatchTypeWordMapRs` object.
        let simple_match_type_word_map: SimpleMatchTypeWordMapRs =
            match rmp_serde::from_slice(simple_match_type_word_map_bytes.as_bytes()) {
                // If deserialization is successful, assign the resulting object to `simple_match_type_word_map`.
                Ok(simple_match_type_word_map) => simple_match_type_word_map,

                // If deserialization fails, return a `PyValueError` with a detailed error message.
                Err(e) => return Err(PyValueError::new_err(
                    format!("Deserialize simple_match_type_word_map_bytes failed, Please check the input data.\n Err: {}", e.to_string()),
                )),
            };

        // Return a new `SimpleMatcher` instance initialized with the deserialized `SimpleMatchTypeWordMapRs` object.
        Ok(SimpleMatcher {
            // Initialize the `simple_matcher` field with a new `SimpleMatcherRs` instance.
            simple_matcher: SimpleMatcherRs::new(&simple_match_type_word_map),

            // Clone the provided byte array reference and assign it to `simple_match_type_word_map_bytes`.
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
            &rmp_serde::from_slice(simple_match_type_word_map_bytes.as_bytes()).unwrap(),
        );
    }

    #[pyo3(signature=(text))]
    /// Checks if the given text matches using the `simple_matcher` instance.
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
    /// - `bool`: `true` if the text matches; `false` otherwise.
    ///
    /// # Example
    /// ```python
    /// from matcher_py import SimpleMatcher
    /// simple_matcher = SimpleMatcher(get_simple_match_type_word_map_bytes())
    /// result = simple_matcher.is_match("some text")
    /// print(result)  # Output: True or False based on matching result
    /// ```
    fn is_match(&self, _py: Python, text: &Bound<'_, PyAny>) -> bool {
        // Attempt to downcast the provided `PyAny` reference to a `PyString`.
        text.downcast::<PyString>()
            // If downcasting fails, return `false`.
            .map_or(
                false,
                // If downcasting succeeds, check if the `simple_matcher` instance considers the string a match.
                |text| {
                    self.simple_matcher
                        // Call the `is_match` method on the `simple_matcher` instance,
                        // passing the text as a string slice. The `unsafe` block is used
                        // to obtain a string slice from the `PyString` without additional safety checks.
                        .is_match(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
                },
            )
    }

    #[pyo3(signature=(text))]
    /// Processes a given text using the `simple_matcher` instance and returns a vector of `SimpleResult`.
    ///
    /// This method attempts to downcast the provided `PyAny` object to a `PyString`.
    /// If the downcast is successful, it processes the text using the `simple_matcher` instance,
    /// converting the results into a vector of `SimpleResult` instances.
    /// If the downcast fails, it returns an empty vector.
    ///
    /// # Parameters
    /// - `self`: The `SimpleMatcher` instance.
    /// - `_py`: The Python interpreter state.
    /// - `text`: A reference to a `PyAny` object which is expected to be a `PyString`.
    ///
    /// # Returns
    /// - `Vec<SimpleResult>`: A vector of `SimpleResult` instances containing the match results.
    ///   If the input `text` is not a `PyString`, an empty vector is returned.
    ///
    /// # Example
    /// ```python
    /// from matcher_py import SimpleMatcher
    /// simple_matcher = SimpleMatcher(get_simple_match_type_word_map_bytes())
    /// result = simple_matcher.simple_process("some text")
    /// print(result)  # Output: A list of SimpleResult instances
    /// ```
    fn simple_process(&self, _py: Python, text: &Bound<'_, PyAny>) -> Vec<SimpleResult> {
        // Attempt to downcast the provided `PyAny` reference to a `PyString`.
        // If the downcast fails, return an empty `Vec<SimpleResult>`.
        text.downcast::<PyString>().map_or(Vec::new(), |text| {
            // If downcasting succeeds, process the text using the `simple_matcher` instance.
            self.simple_matcher
                // Call the `process` method on the `simple_matcher` instance,
                // passing the text as a string slice. The `unsafe` block is used
                // to obtain a string slice from the `PyString` without additional safety checks.
                .process(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
                // Convert the results into an iterator, map each `SimpleResultRs` instance
                // to a `SimpleResult` instance, and collect the results into a `Vec<SimpleResult>`.
                .into_iter()
                .map(|simple_result| SimpleResult(simple_result))
                // Collect the mapped results into a vector and return it.
                .collect::<Vec<_>>()
        })
    }

    #[pyo3(signature=(text_array))]
    /// Batch processes a list of texts using the `simple_matcher` instance and returns
    /// a list of results for each text.
    ///
    /// This method iterates over a list of texts, processes each text using the
    /// `simple_matcher` instance, and collects the results into a new list. The result
    /// for each text is obtained by calling the `simple_process` method, which returns
    /// a vector of `SimpleResult` instances for each text.
    ///
    /// # Parameters
    /// - `self`: The `SimpleMatcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyList` containing texts to be processed.
    ///
    /// # Returns
    /// - `Py<PyList>`: A `PyList` containing the results of the simple processing
    ///   for each text. Each element in the list is a vector of `SimpleResult` instances.
    ///
    /// # Example
    /// ```python
    /// from matcher_py import SimpleMatcher
    /// simple_matcher = SimpleMatcher(get_simple_match_type_word_map_bytes())
    /// text_list = ["text1", "text2", "text3"]
    /// result = simple_matcher.batch_simple_process(text_list)
    /// print(result)  # Output: A list of results for each text
    /// ```
    fn batch_simple_process(&self, py: Python, text_array: &Bound<'_, PyList>) -> Py<PyList> {
        // Create an empty Python list within the specified Python interpreter state.
        let result_list = PyList::empty_bound(py);

        // Iterate over each item in the provided PyList reference.
        text_array.iter().for_each(|text| {
            // Append the result of the simple_process method to the result_list
            // by converting it into a Python object.
            result_list
                .append(self.simple_process(py, &text).into_py(py))
                .unwrap();
        });

        // Return the populated result_list as a Python object.
        result_list.into()
    }

    #[pyo3(signature=(text_array, inplace = false))]
    /// Batch processes a NumPy 1-D array of texts using the `simple_matcher` instance
    /// and returns the results as a new NumPy array or modifying the original array in-place.
    ///
    /// This method iterates over a NumPy 1-D array of texts, processes each text using the
    /// `simple_matcher` instance, and collects the results into a new NumPy array or modifies
    /// the original array in-place based on the `inplace` parameter. The result for each text
    /// is obtained by calling the `simple_process` method, producing a vector of `SimpleResult`
    /// instances for each text.
    ///
    /// # Parameters
    /// - `self`: The `SimpleMatcher` instance.
    /// - `py`: The Python interpreter state.
    /// - `text_array`: A reference to a `PyArray1` containing texts to be processed.
    /// - `inplace`: A boolean flag indicating whether to modify the original array in-place.
    ///
    /// # Returns
    /// - `Option<Py<PyArray1<PyObject>>>`: If `inplace` is `false`, a new `PyArray1` containing
    ///   the processing results for each text. If `inplace` is `true`, returns `None` as the original
    ///   array is modified in-place.
    ///
    /// # Example
    /// ```python
    /// from matcher_py import SimpleMatcher
    /// import numpy as np
    ///
    /// simple_matcher = SimpleMatcher(get_simple_match_type_word_map_bytes())
    /// text_array = np.array(["text1", "text2", "text3"], dtype=np.object)
    /// result = simple_matcher.numpy_simple_process(text_array)
    /// print(result)  # Output: A new NumPy array with processing results as lists of SimpleResult instances
    ///
    /// inplace_result = simple_matcher.numpy_simple_process(text_array, inplace=True)
    /// print(text_array)  # Output: The original NumPy array modified with processing results
    /// ```
    fn numpy_simple_process(
        &self,
        py: Python,
        text_array: &Bound<'_, PyArray1<PyObject>>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            // If inplace is true, modify the original NumPy array directly.
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                // For each element in the NumPy array, process the text using simple_process,
                // bind it to the Python interpreter, and convert the result to a PyObject.
                *text = self.simple_process(py, &text.bind(py)).into_py(py);
            });
            // Since we have modified the original array in-place, return None.
            None
        } else {
            // If inplace is false, create a new NumPy array with the processed results.
            Some(
                // Create a new NumPy array from the processed elements.
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    // Unsafe access to the elements of the NumPy array in read-only mode.
                    unsafe { text_array.as_array() }
                        // For each element, process the text using simple_process,
                        // bind it to the Python interpreter, and convert the result to a PyObject.
                        .map(|text| self.simple_process(py, &text.bind(py)).into_py(py)),
                )
                // Convert the resulting NumPy array to a PyObject and return it wrapped in Some.
                .into(),
            )
        }
    }
}

#[pymodule]
/// Defines a Python module named `matcher_py`.
///
/// This function initializes the `matcher_py` module, adding the `Matcher`
/// and `SimpleMatcher` classes to it. This allows the classes to be accessible
/// from the Python side when the module is imported.
///
/// # Parameters
/// - `_py`: The Python interpreter state.
/// - `m`: A reference to a `PyModule` object representing the module to be initialized.
///
/// # Returns
/// - `PyResult<()>`: Returns `Ok(())` if the module initialization is successful,
///   otherwise returns an error indicating the failure.
fn matcher_py(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Add the `Matcher` class to the module.
    m.add_class::<Matcher>()?;

    // Add the `SimpleMatcher` class to the module.
    m.add_class::<SimpleMatcher>()?;

    // Indicate successful initialization of the module.
    Ok(())
}
