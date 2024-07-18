use std::borrow::Cow;
use std::collections::HashMap;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::{
    pyclass, pymethods, pymodule, wrap_pyfunction, PyModule, PyObject, PyResult, Python,
};
use pyo3::types::{PyDict, PyDictMethods, PyModuleMethods};
use pyo3::{intern, pyfunction, Bound, IntoPy};

use matcher_rs::{
    reduce_text_process as reduce_text_process_rs, text_process as text_process_rs,
    MatchResult as MatchResultRs, MatchTableMap as MatchTableMapRs, Matcher as MatcherRs,
    ProcessType, SimpleMatcher as SimpleMatcherRs, SimpleResult as SimpleResultRs,
    SimpleTable as SimpleTableRs, TextMatcherTrait,
};

struct SimpleResult<'a>(SimpleResultRs<'a>);

impl<'a> IntoPy<PyObject> for SimpleResult<'a> {
    fn into_py(self, py: Python<'_>) -> PyObject {
        let dict = PyDict::new_bound(py);

        dict.set_item(intern!(py, "word_id"), self.0.word_id)
            .unwrap();
        dict.set_item(intern!(py, "word"), self.0.word.as_ref())
            .unwrap();

        dict.into()
    }
}

struct MatchResult<'a>(MatchResultRs<'a>);

impl<'a> IntoPy<PyObject> for MatchResult<'a> {
    fn into_py(self, py: Python<'_>) -> PyObject {
        let dict = PyDict::new_bound(py);

        dict.set_item(intern!(py, "match_id"), self.0.match_id)
            .unwrap();
        dict.set_item(intern!(py, "table_id"), self.0.table_id)
            .unwrap();
        dict.set_item(intern!(py, "word"), self.0.word.as_ref())
            .unwrap();

        dict.into()
    }
}

#[pyfunction]
#[pyo3(signature=(process_type, text))]
fn text_process(process_type: u8, text: &str) -> PyResult<Cow<'_, str>> {
    let process_type = ProcessType::from_bits(process_type).unwrap_or(ProcessType::None);
    match text_process_rs(process_type, text) {
        Ok(result) => Ok(result),
        Err(e) => Err(PyValueError::new_err(e)),
    }
}

#[pyfunction]
#[pyo3(signature=(process_type, text))]
fn reduce_text_process(process_type: u8, text: &str) -> Vec<Cow<'_, str>> {
    let process_type = ProcessType::from_bits(process_type).unwrap_or(ProcessType::None);
    reduce_text_process_rs(process_type, text)
        .into_iter()
        .collect()
}

#[pyclass(module = "matcher_py")]
struct Matcher {
    matcher: MatcherRs,
    match_table_map_bytes: Vec<u8>,
}

#[pymethods]
impl Matcher {
    #[new]
    #[pyo3(signature=(match_table_map_bytes))]
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

    fn __getnewargs__(&self) -> (&[u8],) {
        (&self.match_table_map_bytes,)
    }

    fn __getstate__(&self) -> &[u8] {
        &self.match_table_map_bytes
    }

    #[pyo3(signature=(match_table_map_bytes))]
    fn __setstate__(&mut self, match_table_map_bytes: &[u8]) {
        self.matcher = MatcherRs::new(
            &rmp_serde::from_slice::<MatchTableMapRs>(match_table_map_bytes).unwrap(),
        );
    }

    #[pyo3(signature=(text))]
    fn is_match(&self, text: &str) -> bool {
        self.matcher.is_match(text)
    }

    #[pyo3(signature=(text))]
    fn process<'a>(&'a self, text: &'a str) -> Vec<MatchResult<'_>> {
        self.matcher
            .process(text)
            .into_iter()
            .map(MatchResult)
            .collect()
    }

    #[pyo3(signature=(text))]
    fn word_match<'a>(&'a self, text: &'a str) -> HashMap<u32, Vec<MatchResult<'_>>> {
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
    fn word_match_as_string(&self, text: &str) -> String {
        self.matcher.word_match_as_string(text)
    }
}

#[pyclass(module = "matcher_py")]
struct SimpleMatcher {
    simple_matcher: SimpleMatcherRs,
    simple_table_bytes: Vec<u8>,
}

#[pymethods]
impl SimpleMatcher {
    #[new]
    #[pyo3(signature=(simple_table_bytes))]
    fn new(_py: Python, simple_table_bytes: &[u8]) -> PyResult<SimpleMatcher> {
        let simple_table: SimpleTableRs = match rmp_serde::from_slice(simple_table_bytes) {
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

    fn __getnewargs__(&self) -> (&[u8],) {
        (&self.simple_table_bytes,)
    }

    fn __getstate__(&self) -> &[u8] {
        &self.simple_table_bytes
    }

    #[pyo3(signature=(simple_table_bytes))]
    fn __setstate__(&mut self, simple_table_bytes: &[u8]) {
        self.simple_matcher = SimpleMatcherRs::new(
            &rmp_serde::from_slice::<SimpleTableRs>(simple_table_bytes).unwrap(),
        );
    }

    #[pyo3(signature=(text))]
    fn is_match(&self, text: &str) -> bool {
        self.simple_matcher.is_match(text)
    }

    #[pyo3(signature=(text))]
    fn process<'a>(&'a self, text: &'a str) -> Vec<SimpleResult> {
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
