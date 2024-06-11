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

impl MatchResultTrait<'_> for SimpleResult<'_> {
    fn word_id(&self) -> usize {
        self.0.word_id()
    }
    fn word(&self) -> &str {
        self.0.word.as_ref()
    }
}

#[pyclass(module = "matcher_py")]
struct Matcher {
    matcher: MatcherRs,
    match_table_map_bytes: Py<PyBytes>,
}

#[pymethods]
impl Matcher {
    #[new]
    fn new(_py: Python, match_table_map_bytes: &Bound<'_, PyBytes>) -> PyResult<Matcher> {
        let match_table_map: MatchTableMapRs =
            match rmp_serde::from_slice(match_table_map_bytes.as_bytes()) {
                Ok(match_table_map) => match_table_map,
                Err(e) => {
                    return Err(PyValueError::new_err(format!(
                "Deserialize match_table_map_bytes failed, Please check the input data.\nErr: {}",
                e.to_string()
            )))
                }
            };

        Ok(Matcher {
            matcher: MatcherRs::new(&match_table_map),
            match_table_map_bytes: match_table_map_bytes.as_unbound().to_owned(),
        })
    }

    fn __getnewargs__(&self, py: Python) -> Py<PyBytes> {
        self.match_table_map_bytes.clone_ref(py)
    }

    fn __getstate__(&self, py: Python) -> Py<PyBytes> {
        self.match_table_map_bytes.clone_ref(py)
    }

    fn __setstate__(&mut self, _py: Python, match_table_map_bytes: &Bound<'_, PyBytes>) {
        self.matcher =
            MatcherRs::new(&rmp_serde::from_slice(match_table_map_bytes.as_bytes()).unwrap());
    }

    #[pyo3(signature=(text))]
    fn is_match(&self, _py: Python, text: &Bound<'_, PyAny>) -> bool {
        text.downcast::<PyString>().map_or(false, |text| {
            self.matcher
                .is_match(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
        })
    }

    #[pyo3(signature=(text))]
    fn word_match(&self, _py: Python, text: &Bound<'_, PyAny>) -> HashMap<&str, String> {
        text.downcast::<PyString>().map_or(HashMap::new(), |text| {
            self.matcher
                .word_match(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
        })
    }

    #[pyo3(signature=(text))]
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
    fn batch_word_match_as_dict(&self, py: Python, text_array: &Bound<'_, PyList>) -> Py<PyList> {
        let result_list = PyList::empty_bound(py);

        text_array.iter().for_each(|text| {
            result_list.append(self.word_match(py, &text)).unwrap();
        });

        result_list.into()
    }

    #[pyo3(signature=(text_array))]
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
                        .map(|text| self.word_match(py, &text.bind(py)).into_py(py)),
                )
                .into(),
            )
        }
    }

    #[pyo3(signature=(text_array, inplace = false))]
    fn numpy_word_match_as_string(
        &self,
        py: Python,
        text_array: &Bound<'_, PyArray1<PyObject>>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                *text = self.word_match_as_string(py, &text.bind(py)).into_py(py);
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    unsafe { text_array.as_array() }
                        .map(|text| self.word_match_as_string(py, &text.bind(py)).into_py(py)),
                )
                .into(),
            )
        }
    }
}

#[pyclass(module = "matcher_py")]
struct SimpleMatcher {
    simple_matcher: SimpleMatcherRs,
    simple_match_type_word_map_bytes: Py<PyBytes>,
}

#[pymethods]
impl SimpleMatcher {
    #[new]
    fn new(
        _py: Python,
        simple_match_type_word_map_bytes: &Bound<'_, PyBytes>,
    ) -> PyResult<SimpleMatcher> {
        let simple_match_type_word_map: SimpleMatchTypeWordMapRs =
            match rmp_serde::from_slice(simple_match_type_word_map_bytes.as_bytes()) {
                Ok(simple_match_type_word_map) => simple_match_type_word_map,
                Err(e) => return Err(PyValueError::new_err(
                    format!("Deserialize simple_match_type_word_map_bytes failed, Please check the input data.\n Err: {}", e.to_string()),
                )),
            };

        Ok(SimpleMatcher {
            simple_matcher: SimpleMatcherRs::new(&simple_match_type_word_map),
            simple_match_type_word_map_bytes: simple_match_type_word_map_bytes
                .as_unbound()
                .to_owned(),
        })
    }

    fn __getnewargs__(&self, py: Python) -> Py<PyBytes> {
        self.simple_match_type_word_map_bytes.clone_ref(py)
    }

    fn __getstate__(&self, py: Python) -> Py<PyBytes> {
        self.simple_match_type_word_map_bytes.clone_ref(py)
    }

    fn __setstate__(&mut self, _py: Python, simple_match_type_word_map_bytes: &Bound<'_, PyBytes>) {
        self.simple_matcher = SimpleMatcherRs::new(
            &rmp_serde::from_slice(simple_match_type_word_map_bytes.as_bytes()).unwrap(),
        );
    }

    #[pyo3(signature=(text))]
    fn is_match(&self, _py: Python, text: &Bound<'_, PyAny>) -> bool {
        text.downcast::<PyString>().map_or(false, |text| {
            self.simple_matcher
                .is_match(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
        })
    }

    #[pyo3(signature=(text))]
    fn simple_process(&self, _py: Python, text: &Bound<'_, PyAny>) -> Vec<SimpleResult> {
        text.downcast::<PyString>().map_or(Vec::new(), |text| {
            self.simple_matcher
                .process(unsafe { text.to_cow().as_ref().unwrap_unchecked() })
                .into_iter()
                .map(|simple_result| SimpleResult(simple_result))
                .collect::<Vec<_>>()
        })
    }

    #[pyo3(signature=(text_array))]
    fn batch_simple_process(&self, py: Python, text_array: &Bound<'_, PyList>) -> Py<PyList> {
        let result_list = PyList::empty_bound(py);

        text_array.iter().for_each(|text| {
            result_list
                .append(self.simple_process(py, &text).into_py(py))
                .unwrap();
        });

        result_list.into()
    }

    #[pyo3(signature=(text_array, inplace = false))]
    fn numpy_simple_process(
        &self,
        py: Python,
        text_array: &Bound<'_, PyArray1<PyObject>>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                *text = self.simple_process(py, &text.bind(py)).into_py(py);
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array_bound(
                    py,
                    unsafe { text_array.as_array() }
                        .map(|text| self.simple_process(py, &text.bind(py)).into_py(py)),
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
