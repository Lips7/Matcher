use std::{borrow::Cow, collections::HashMap};

use matcher_rs::{ProcessType, SimpleMatcher, SimpleTableSerde, reduce_text_process, text_process};
use pyo3::{
    Bound,
    exceptions::{PyTypeError, PyValueError},
    prelude::{
        Py, PyAny, PyModule, PyResult, Python, pyclass, pymethods, pymodule, wrap_pyfunction,
    },
    pyfunction,
    types::{PyAnyMethods, PyDict, PyModuleMethods, PyString, PyType},
};

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

fn deserialize_table(bytes: &[u8]) -> PyResult<SimpleTableSerde<'_>> {
    sonic_rs::from_slice(bytes)
        .map_err(|e| PyValueError::new_err(format!("Deserialize simple_table_bytes failed: {e}")))
}

#[pyclass(name = "ProcessType", module = "matcher_py", eq, from_py_object)]
#[derive(Clone, PartialEq, Eq)]
pub struct PyProcessType(ProcessType);

#[pymethods]
impl PyProcessType {
    #[classattr]
    const NONE: u8 = ProcessType::None.bits();

    #[classattr]
    const VARIANT_NORM: u8 = ProcessType::VariantNorm.bits();

    #[classattr]
    const DELETE: u8 = ProcessType::Delete.bits();

    #[classattr]
    const NORMALIZE: u8 = ProcessType::Normalize.bits();

    #[classattr]
    const DELETE_NORMALIZE: u8 = ProcessType::DeleteNormalize.bits();

    #[classattr]
    const VARIANT_NORM_DELETE_NORMALIZE: u8 = ProcessType::VariantNormDeleteNormalize.bits();

    #[classattr]
    const ROMANIZE: u8 = ProcessType::Romanize.bits();

    #[classattr]
    const ROMANIZE_CHAR: u8 = ProcessType::RomanizeChar.bits();

    #[classattr]
    const EMOJI_NORM: u8 = ProcessType::EmojiNorm.bits();

    #[new]
    fn new(bits: u8) -> Self {
        PyProcessType(ProcessType::from_bits_retain(bits))
    }

    fn __or__(&self, other: &Self) -> Self {
        PyProcessType(self.0 | other.0)
    }

    fn __and__(&self, other: &Self) -> Self {
        PyProcessType(self.0 & other.0)
    }

    fn __invert__(&self) -> Self {
        PyProcessType(!self.0)
    }

    fn __int__(&self) -> u8 {
        self.0.bits()
    }

    fn __index__(&self) -> u8 {
        self.0.bits()
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }
}

#[pyclass(name = "SimpleResult", module = "matcher_py")]
pub struct PySimpleResult {
    #[pyo3(get)]
    pub word_id: u32,
    #[pyo3(get)]
    pub word: Py<PyString>,
}

#[pyfunction(name = "text_process")]
#[pyo3(signature=(process_type, text))]
fn py_text_process<'a>(process_type: Bound<'_, PyAny>, text: &'a str) -> PyResult<Cow<'a, str>> {
    Ok(text_process(extract_process_type(&process_type)?, text))
}

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

#[pyclass(name = "SimpleMatcher", module = "matcher_py")]
pub struct PySimpleMatcher {
    simple_matcher: SimpleMatcher,
    simple_table_bytes: Vec<u8>,
}

#[pymethods]
impl PySimpleMatcher {
    #[new]
    #[pyo3(signature=(simple_table_bytes))]
    fn new(_py: Python, simple_table_bytes: &[u8]) -> PyResult<PySimpleMatcher> {
        let simple_table = deserialize_table(simple_table_bytes)?;
        Ok(PySimpleMatcher {
            simple_matcher: SimpleMatcher::new(&simple_table)
                .map_err(|e| PyValueError::new_err(e.to_string()))?,
            simple_table_bytes: simple_table_bytes.to_vec(),
        })
    }

    #[classmethod]
    #[pyo3(signature=(table))]
    fn from_dict(_cls: &Bound<'_, PyType>, py: Python, table: &Bound<'_, PyAny>) -> PyResult<Self> {
        let json_mod = py.import("json")?;
        let json_str = json_mod.call_method1("dumps", (table,))?;
        let bytes_str = json_str.call_method1("encode", ("utf-8",))?;
        let bytes: Vec<u8> = bytes_str.extract()?;

        let simple_table = deserialize_table(&bytes)?;
        Ok(PySimpleMatcher {
            simple_matcher: SimpleMatcher::new(&simple_table)
                .map_err(|e| PyValueError::new_err(e.to_string()))?,
            simple_table_bytes: bytes,
        })
    }

    fn __getnewargs__(&self) -> (&[u8],) {
        (&self.simple_table_bytes,)
    }

    fn __getstate__(&self) -> &[u8] {
        &self.simple_table_bytes
    }

    #[pyo3(signature=(simple_table_bytes))]
    fn __setstate__(&mut self, simple_table_bytes: &[u8]) -> PyResult<()> {
        let simple_table = deserialize_table(simple_table_bytes)?;
        self.simple_matcher =
            SimpleMatcher::new(&simple_table).map_err(|e| PyValueError::new_err(e.to_string()))?;
        self.simple_table_bytes = simple_table_bytes.to_vec();
        Ok(())
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.simple_matcher)
    }

    fn stats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);

        let table: HashMap<u8, HashMap<u32, String>> =
            sonic_rs::from_slice(&self.simple_table_bytes).unwrap_or_default();

        let rule_count: usize = table.values().map(|m| m.len()).sum();
        dict.set_item("rule_count", rule_count)?;

        let mut process_types: Vec<u8> = table.keys().copied().collect();
        process_types.sort_unstable();
        dict.set_item("process_types", process_types)?;

        Ok(dict)
    }

    #[pyo3(signature=(text))]
    fn is_match(&self, py: Python<'_>, text: &str) -> bool {
        let matcher = &self.simple_matcher;
        py.detach(|| matcher.is_match(text))
    }

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

    #[pyo3(signature=(texts))]
    fn batch_is_match(&self, py: Python<'_>, texts: Vec<String>) -> Vec<bool> {
        let matcher = &self.simple_matcher;
        py.detach(|| texts.iter().map(|t| matcher.is_match(t)).collect())
    }

    #[pyo3(signature=(texts))]
    fn batch_process(&self, py: Python<'_>, texts: Vec<String>) -> Vec<Vec<PySimpleResult>> {
        let matcher = &self.simple_matcher;
        let all: Vec<Vec<(u32, String)>> = py.detach(|| {
            let mut buf = Vec::new();
            texts
                .iter()
                .map(|t| {
                    buf.clear();
                    matcher.process_into(t, &mut buf);
                    buf.drain(..)
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
    #[pyo3(signature=(texts))]
    fn batch_find_match(&self, py: Python<'_>, texts: Vec<String>) -> Vec<Option<PySimpleResult>> {
        let matcher = &self.simple_matcher;
        let all: Vec<Option<(u32, String)>> = py.detach(|| {
            texts
                .iter()
                .map(|t| {
                    matcher
                        .find_match(t)
                        .map(|r| (r.word_id, r.word.into_owned()))
                })
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

#[pymodule]
fn matcher_py(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyProcessType>()?;
    m.add_class::<PySimpleMatcher>()?;
    m.add_class::<PySimpleResult>()?;
    m.add_function(wrap_pyfunction!(py_reduce_text_process, m)?)?;
    m.add_function(wrap_pyfunction!(py_text_process, m)?)?;
    Ok(())
}
