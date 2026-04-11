//! Python bindings for the [`matcher_rs`] pattern-matching engine via PyO3.
//!
//! Exports four classes ([`ProcessType`](PyProcessType),
//! [`SimpleMatcher`](PySimpleMatcher),
//! [`SimpleMatcherBuilder`](PySimpleMatcherBuilder),
//! [`SimpleResult`](PySimpleResult))
//! and two standalone functions ([`text_process`], [`reduce_text_process`]).
//!
//! All matcher operations release the GIL, so multiple Python threads can call
//! [`SimpleMatcher::is_match`] / [`SimpleMatcher::process`] concurrently on the
//! same immutable matcher instance.
//!
//! # Quick start
//!
//! ```python
//! from matcher_py import SimpleMatcher, ProcessType
//!
//! table = {ProcessType.NONE: {1: "hello&world"}}
//! matcher = SimpleMatcher.from_dict(table)
//! assert matcher.is_match("hello beautiful world")
//! ```

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use matcher_rs::{
    ProcessType, SimpleMatcher, SimpleMatcherBuilder, SimpleTableSerde, reduce_text_process,
    text_process,
};
use pyo3::{
    Bound,
    exceptions::{PyTypeError, PyValueError},
    prelude::{
        Py, PyAny, PyModule, PyResult, Python, pyclass, pymethods, pymodule, wrap_pyfunction,
    },
    pybacked::PyBackedStr,
    pyfunction,
    types::{PyAnyMethods, PyBytes, PyDict, PyDictMethods, PyModuleMethods, PyString, PyType},
};

/// Extracts a [`ProcessType`] from a Python `ProcessType` instance or raw
/// `int`.
fn extract_process_type(obj: &Bound<'_, PyAny>) -> PyResult<ProcessType> {
    if let Ok(bits) = obj.extract::<u8>() {
        Ok(ProcessType::from_bits_retain(bits))
    } else if let Ok(pt) = obj.extract::<PyProcessType>() {
        Ok(pt.0)
    } else {
        Err(PyTypeError::new_err(
            "process_type must be ProcessType or int",
        ))
    }
}

/// Deserializes JSON bytes into a [`SimpleTableSerde`], mapping parse errors to
/// [`PyValueError`].
fn deserialize_table(bytes: &[u8]) -> PyResult<SimpleTableSerde<'_>> {
    sonic_rs::from_slice(bytes)
        .map_err(|e| PyValueError::new_err(format!("Deserialize simple_table_bytes failed: {e}")))
}

/// Collects `(process_type_bits, word_id, pattern)` triples from a deserialized
/// table, for storage in [`PySimpleMatcher::words`].
fn collect_words(table: &SimpleTableSerde<'_>) -> Vec<(u8, u32, String)> {
    table
        .iter()
        .flat_map(|(pt, inner)| {
            let bits = pt.bits();
            inner
                .iter()
                .map(move |(&id, word)| (bits, id, word.to_string()))
        })
        .collect()
}

/// Bitflag enum controlling which text normalizations to apply before matching.
///
/// Compose flags with the `|` operator: `ProcessType.DELETE |
/// ProcessType.NORMALIZE`. Usable as dict keys in the JSON table passed to
/// `SimpleMatcher`.
#[pyclass(name = "ProcessType", module = "matcher_py", eq, from_py_object)]
#[derive(Clone, PartialEq, Eq)]
pub struct PyProcessType(ProcessType);

#[pymethods]
impl PyProcessType {
    /// No transformation — match against raw text.
    #[classattr]
    const NONE: u8 = ProcessType::None.bits();

    /// CJK variant normalization (Traditional → Simplified, Kyūjitai →
    /// Shinjitai).
    #[classattr]
    const VARIANT_NORM: u8 = ProcessType::VariantNorm.bits();

    /// Remove configured noise codepoints (punctuation, symbols).
    #[classattr]
    const DELETE: u8 = ProcessType::Delete.bits();

    /// Fullwidth → halfwidth, uppercase → lowercase normalization.
    #[classattr]
    const NORMALIZE: u8 = ProcessType::Normalize.bits();

    /// Shorthand for `DELETE | NORMALIZE`.
    #[classattr]
    const DELETE_NORMALIZE: u8 = ProcessType::DeleteNormalize.bits();

    /// Shorthand for `VARIANT_NORM | DELETE | NORMALIZE`.
    #[classattr]
    const VARIANT_NORM_DELETE_NORMALIZE: u8 = ProcessType::VariantNormDeleteNormalize.bits();

    /// CJK → Latin romanization (word-level pinyin/romaji/revised
    /// romanization).
    #[classattr]
    const ROMANIZE: u8 = ProcessType::Romanize.bits();

    /// CJK → Latin romanization (character-level, no inter-syllable spaces).
    #[classattr]
    const ROMANIZE_CHAR: u8 = ProcessType::RomanizeChar.bits();

    /// Emoji → English word normalization via CLDR short names.
    #[classattr]
    const EMOJI_NORM: u8 = ProcessType::EmojiNorm.bits();

    /// Construct a `ProcessType` from raw `u8` bits.
    #[new]
    fn new(bits: u8) -> Self {
        PyProcessType(ProcessType::from_bits_retain(bits))
    }

    /// Combine two process types: `ProcessType.DELETE | ProcessType.NORMALIZE`.
    fn __or__(&self, other: &Self) -> Self {
        PyProcessType(self.0 | other.0)
    }

    /// Intersect two process types.
    fn __and__(&self, other: &Self) -> Self {
        PyProcessType(self.0 & other.0)
    }

    /// Bitwise complement.
    fn __invert__(&self) -> Self {
        PyProcessType(!self.0)
    }

    /// Return the raw `u8` value.
    fn __int__(&self) -> u8 {
        self.0.bits()
    }

    /// Support `int()` conversion and sequence indexing.
    fn __index__(&self) -> u8 {
        self.0.bits()
    }

    /// Debug string showing the active flags.
    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

/// A single match result containing the rule identifier and the matched
/// pattern.
#[pyclass(name = "SimpleResult", module = "matcher_py")]
pub struct PySimpleResult {
    /// Caller-assigned rule identifier (the dict key in the matcher table).
    #[pyo3(get)]
    pub word_id: u32,
    /// The original pattern string that matched.
    #[pyo3(get)]
    pub word: Py<PyString>,
}

/// Apply the text transformation pipeline and return the final result.
///
/// `process_type` accepts a `ProcessType` instance or a raw `int`.
#[pyfunction(name = "text_process")]
#[pyo3(signature=(process_type, text))]
fn py_text_process<'a>(process_type: Bound<'_, PyAny>, text: &'a str) -> PyResult<Cow<'a, str>> {
    Ok(text_process(extract_process_type(&process_type)?, text))
}

/// Apply the transformation pipeline, returning all intermediate text variants.
///
/// The first element is always the original input; subsequent elements are the
/// output of each transformation step that changed the text.
#[pyfunction(name = "reduce_text_process")]
#[pyo3(signature=(process_type, text))]
fn py_reduce_text_process<'a>(
    process_type: Bound<'_, PyAny>,
    text: &'a str,
) -> PyResult<Vec<Cow<'a, str>>> {
    Ok(reduce_text_process(
        extract_process_type(&process_type)?,
        text,
    ))
}

/// Fluent builder for constructing a [`SimpleMatcher`] without serialization.
///
/// ```python
/// from matcher_py import SimpleMatcherBuilder, ProcessType
///
/// builder = SimpleMatcherBuilder()
/// builder.add_word(ProcessType.NONE, 1, "hello")
/// builder.add_word(ProcessType.NONE, 2, "world")
/// matcher = builder.build()
/// assert matcher.is_match("hello world")
/// ```
#[pyclass(name = "SimpleMatcherBuilder", module = "matcher_py")]
pub struct PySimpleMatcherBuilder {
    words: Vec<(u8, u32, String)>,
}

#[pymethods]
impl PySimpleMatcherBuilder {
    #[new]
    fn new() -> Self {
        PySimpleMatcherBuilder { words: Vec::new() }
    }

    /// Register a pattern under the given process type and word ID.
    #[pyo3(signature=(process_type, word_id, word))]
    fn add_word(
        &mut self,
        process_type: &Bound<'_, PyAny>,
        word_id: u32,
        word: &str,
    ) -> PyResult<()> {
        let pt = extract_process_type(process_type)?;
        self.words.push((pt.bits(), word_id, word.to_owned()));
        Ok(())
    }

    /// Compile accumulated patterns into a [`SimpleMatcher`].
    ///
    /// Drains the builder — a second `build()` raises `ValueError`.
    fn build(&mut self) -> PyResult<PySimpleMatcher> {
        let words = std::mem::take(&mut self.words);
        build_from_words(words)
    }
}

/// Shared construction logic: builds a [`PySimpleMatcher`] from word triples.
fn build_from_words(words: Vec<(u8, u32, String)>) -> PyResult<PySimpleMatcher> {
    let mut builder = SimpleMatcherBuilder::new();
    for &(bits, id, ref word) in &words {
        builder = builder.add_word(ProcessType::from_bits_retain(bits), id, word.as_str());
    }
    let simple_matcher = builder
        .build()
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PySimpleMatcher {
        simple_matcher,
        words,
    })
}

/// Immutable compiled pattern matcher. Thread-safe — all query methods release
/// the GIL.
///
/// Constructed from a JSON table mapping `process_type` (int) → `word_id` (int)
/// → `pattern` (str). Patterns support logical operators: `&` (AND), `~` (NOT),
/// `|` (OR), and `\b` (word boundary).
///
/// ```python
/// import json
/// from matcher_py import SimpleMatcher, ProcessType
///
/// table = {ProcessType.NONE: {1: "hello&world"}}
/// matcher = SimpleMatcher(json.dumps(table).encode())
/// assert matcher.is_match("hello beautiful world")
/// ```
#[pyclass(name = "SimpleMatcher", module = "matcher_py")]
pub struct PySimpleMatcher {
    simple_matcher: SimpleMatcher,
    words: Vec<(u8, u32, String)>,
}

#[pymethods]
impl PySimpleMatcher {
    /// Construct a matcher from JSON bytes.
    ///
    /// The JSON format is `{process_type_u8: {word_id_int: "pattern_str"}}`.
    /// Use `json.dumps(table).encode()` to produce the bytes.
    ///
    /// Raises `ValueError` on invalid JSON or pattern syntax errors.
    #[new]
    #[pyo3(signature=(simple_table_bytes))]
    fn new(_py: Python, simple_table_bytes: &[u8]) -> PyResult<PySimpleMatcher> {
        let simple_table = deserialize_table(simple_table_bytes)?;
        let words = collect_words(&simple_table);
        Ok(PySimpleMatcher {
            simple_matcher: SimpleMatcher::new(&simple_table)
                .map_err(|e| PyValueError::new_err(e.to_string()))?,
            words,
        })
    }

    /// Construct a matcher from a Python dict.
    ///
    /// Iterates the dict directly — no JSON serialization overhead.
    /// `SimpleMatcher.from_dict({ProcessType.NONE: {1: "hello"}})`.
    #[classmethod]
    #[pyo3(signature=(table))]
    fn from_dict(
        _cls: &Bound<'_, PyType>,
        _py: Python,
        table: &Bound<'_, PyDict>,
    ) -> PyResult<PySimpleMatcher> {
        let mut words = Vec::new();
        for (pt_key, inner_val) in table.iter() {
            let pt = extract_process_type(&pt_key)?;
            let inner: &Bound<'_, PyDict> = inner_val
                .cast()
                .map_err(|_| PyTypeError::new_err("table values must be dict[int, str]"))?;
            for (id_key, word_val) in inner.iter() {
                let word_id: u32 = id_key
                    .extract()
                    .map_err(|_| PyTypeError::new_err("word_id keys must be int"))?;
                let word: String = word_val
                    .extract()
                    .map_err(|_| PyTypeError::new_err("pattern values must be str"))?;
                words.push((pt.bits(), word_id, word));
            }
        }
        build_from_words(words)
    }

    /// Pickle support: returns constructor args for `__new__`.
    fn __getnewargs__(&self, py: Python) -> PyResult<(Py<PyBytes>,)> {
        let mut table: HashMap<u8, HashMap<u32, &str>> = HashMap::new();
        for &(pt, id, ref word) in &self.words {
            table.entry(pt).or_default().insert(id, word.as_str());
        }
        let bytes = sonic_rs::to_vec(&table)
            .map_err(|e| PyValueError::new_err(format!("Pickle serialization failed: {e}")))?;
        Ok((PyBytes::new(py, &bytes).into(),))
    }

    /// Debug representation showing matcher internals.
    fn __repr__(&self) -> String {
        format!("{:?}", self.simple_matcher)
    }

    /// Returns `{"rule_count": int, "process_types": list[int]}` summarizing
    /// the matcher configuration.
    fn stats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        dict.set_item("rule_count", self.words.len())?;
        let mut pts: Vec<u8> = self
            .words
            .iter()
            .map(|(pt, _, _)| *pt)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        pts.sort_unstable();
        dict.set_item("process_types", pts)?;
        Ok(dict)
    }

    /// Estimated heap memory in bytes used by the compiled matcher internals.
    fn heap_bytes(&self) -> usize {
        self.simple_matcher.heap_bytes()
    }

    /// Return `True` if `text` matches any rule. Releases the GIL.
    #[pyo3(signature=(text))]
    fn is_match(&self, py: Python<'_>, text: &str) -> bool {
        let matcher = &self.simple_matcher;
        py.detach(|| matcher.is_match(text))
    }

    /// Return all matching rules as `list[SimpleResult]`. Releases the GIL.
    #[pyo3(signature=(text))]
    fn process(&self, py: Python<'_>, text: &str) -> Vec<PySimpleResult> {
        let matcher = &self.simple_matcher;
        let owned: Vec<(u32, String)> = py.detach(|| {
            matcher
                .process(text)
                .into_iter()
                .map(|r| (r.word_id, r.word.into_owned()))
                .collect()
        });
        owned
            .into_iter()
            .map(|(word_id, word)| PySimpleResult {
                word_id,
                word: PyString::new(py, &word).into(),
            })
            .collect()
    }

    /// Return the first matching rule as `SimpleResult`, or `None`. Releases
    /// the GIL.
    #[pyo3(signature=(text))]
    fn find_match(&self, py: Python<'_>, text: &str) -> Option<PySimpleResult> {
        let matcher = &self.simple_matcher;
        let owned: Option<(u32, String)> = py.detach(|| {
            matcher
                .find_match(text)
                .map(|r| (r.word_id, r.word.into_owned()))
        });
        owned.map(|(word_id, word)| PySimpleResult {
            word_id,
            word: PyString::new(py, &word).into(),
        })
    }

    /// Batch `is_match`: `list[str] → list[bool]`. Single GIL release for the
    /// entire batch. Uses rayon for parallel matching across CPU cores.
    ///
    /// Input uses `PyBackedStr` (zero-copy borrow from Python strings).
    #[pyo3(signature=(texts))]
    fn batch_is_match(&self, py: Python<'_>, texts: Vec<PyBackedStr>) -> Vec<bool> {
        let matcher = &self.simple_matcher;
        py.detach(|| {
            let refs: Vec<&str> = texts.iter().map(|s| &**s).collect();
            matcher.batch_is_match(&refs)
        })
    }

    /// Batch `process`: `list[str] → list[list[SimpleResult]]`. Single GIL
    /// release. Uses rayon for parallel matching across CPU cores.
    ///
    /// Input uses `PyBackedStr` (zero-copy borrow from Python strings).
    #[pyo3(signature=(texts))]
    fn batch_process(&self, py: Python<'_>, texts: Vec<PyBackedStr>) -> Vec<Vec<PySimpleResult>> {
        let matcher = &self.simple_matcher;
        let all: Vec<Vec<(u32, String)>> = py.detach(|| {
            let refs: Vec<&str> = texts.iter().map(|s| &**s).collect();
            matcher
                .batch_process(&refs)
                .into_iter()
                .map(|results| {
                    results
                        .into_iter()
                        .map(|r| (r.word_id, r.word.into_owned()))
                        .collect()
                })
                .collect()
        });
        all.into_iter()
            .map(|results| {
                results
                    .into_iter()
                    .map(|(word_id, word)| PySimpleResult {
                        word_id,
                        word: PyString::new(py, &word).into(),
                    })
                    .collect()
            })
            .collect()
    }

    /// Batch `find_match`: `list[str] → list[Optional[SimpleResult]]`. Single
    /// GIL release. Uses rayon for parallel matching across CPU cores.
    ///
    /// Input uses `PyBackedStr` (zero-copy borrow from Python strings).
    #[pyo3(signature=(texts))]
    fn batch_find_match(
        &self,
        py: Python<'_>,
        texts: Vec<PyBackedStr>,
    ) -> Vec<Option<PySimpleResult>> {
        let matcher = &self.simple_matcher;
        let all: Vec<Option<(u32, String)>> = py.detach(|| {
            let refs: Vec<&str> = texts.iter().map(|s| &**s).collect();
            matcher
                .batch_find_match(&refs)
                .into_iter()
                .map(|opt| opt.map(|r| (r.word_id, r.word.into_owned())))
                .collect()
        });
        all.into_iter()
            .map(|opt| {
                opt.map(|(word_id, word)| PySimpleResult {
                    word_id,
                    word: PyString::new(py, &word).into(),
                })
            })
            .collect()
    }
}

/// PyO3 module entry point. Registers classes and functions.
#[pymodule]
fn matcher_py(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyProcessType>()?;
    m.add_class::<PySimpleMatcher>()?;
    m.add_class::<PySimpleMatcherBuilder>()?;
    m.add_class::<PySimpleResult>()?;
    m.add_function(wrap_pyfunction!(py_reduce_text_process, m)?)?;
    m.add_function(wrap_pyfunction!(py_text_process, m)?)?;
    Ok(())
}
