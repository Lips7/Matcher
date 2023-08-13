use std::collections::HashMap;

use numpy::PyArray1;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::{pyclass, pymethods, pymodule, Py, PyModule, PyObject, PyResult, Python};
use pyo3::types::{PyBytes, PyDict, PyList, PyString};
use pyo3::{intern, IntoPy, PyAny};

use matcher_rs::{
    MatchTableDict as MatchTableDictRs, Matcher as MatcherRs, SimpleMatcher as SimpleMatcherRs,
    SimpleResult as SimpleResultRs, SimpleWordlistDict as SimpleWordlistDictRs, TextMatcherTrait,
};

struct SimpleResult<'a>(SimpleResultRs<'a>);

impl<'a> IntoPy<PyObject> for SimpleResult<'a> {
    fn into_py(self, py: Python<'_>) -> PyObject {
        let dict = PyDict::new(py);

        dict.set_item(intern!(py, "word_id"), self.0.word_id)
            .unwrap();
        dict.set_item(intern!(py, "word"), self.0.word.as_ref())
            .unwrap();

        dict.into()
    }
}

#[pyclass(module = "matcher_py", unsendable)]
struct Matcher {
    matcher: MatcherRs,
    match_table_dict_bytes: Py<PyBytes>,
}

#[pymethods]
impl Matcher {
    #[new]
    fn new(_py: Python, match_table_dict_bytes: &PyBytes) -> PyResult<Matcher> {
        // 之所以用msgpack而不是json，是因为serde json在做zero copy deserialization时，无法分辨一些特殊字符，eg. "It's /\/\y duty"
        let match_table_dict: MatchTableDictRs =
            match rmp_serde::from_slice(match_table_dict_bytes.as_bytes()) {
                Ok(match_table_dict) => match_table_dict,
                Err(e) => {
                    return Err(PyValueError::new_err(format!(
                "Deserialize match_table_dict_bytes failed, Please check the input data.\nErr: {}",
                e.to_string()
            )))
                }
            };

        Ok(Matcher {
            matcher: MatcherRs::new(&match_table_dict),
            match_table_dict_bytes: match_table_dict_bytes.into(),
        })
    }

    // __getnewargs__, __getstate__, __setstate__ 3个函数都是为pickle实现的，spark executor在调用这些方法时，需要用pickle序列化反序列化这些实例
    fn __getnewargs__(&self, py: Python) -> Py<PyBytes> {
        self.match_table_dict_bytes.clone_ref(py)
    }

    fn __getstate__(&self, py: Python) -> Py<PyBytes> {
        self.match_table_dict_bytes.clone_ref(py)
    }

    fn __setstate__(&mut self, match_table_dict_bytes: &PyBytes) -> PyResult<()> {
        self.matcher =
            MatcherRs::new(&rmp_serde::from_slice(match_table_dict_bytes.as_bytes()).unwrap());

        Ok(())
    }

    fn is_match(&self, _py: Python, text: &PyAny) -> bool {
        text.downcast::<PyString>().map_or(false, |text| {
            self.matcher
                .is_match(unsafe { text.to_str().unwrap_unchecked() })
        })
    }

    fn word_match(&self, _py: Python, text: &PyAny) -> HashMap<&str, String> {
        text.downcast::<PyString>().map_or(HashMap::new(), |text| {
            self.matcher
                .word_match(unsafe { text.to_str().unwrap_unchecked() })
        })
    }

    fn word_match_as_string(&self, py: Python, text: &PyAny) -> Py<PyString> {
        text.downcast::<PyString>()
            .map_or(PyString::intern(py, "{}"), |text| {
                PyString::intern(
                    py,
                    &self
                        .matcher
                        .word_match_as_string(unsafe { text.to_str().unwrap_unchecked() }),
                )
            })
            .into()
    }

    fn batch_word_match_as_dict(&self, py: Python, text_array: &PyList) -> Py<PyList> {
        let result_list = PyList::empty(py);

        text_array.iter().for_each(|text| {
            result_list.append(self.word_match(py, text)).unwrap();
        });

        result_list.into()
    }

    fn batch_word_match_as_string(&self, py: Python, text_array: &PyList) -> Py<PyList> {
        let result_list = PyList::empty(py);

        text_array.iter().for_each(|text| {
            result_list
                .append(self.word_match_as_string(py, text))
                .unwrap();
        });

        result_list.into()
    }

    #[pyo3(signature=(text_array, inplace = false))]
    fn numpy_word_match_as_dict(
        &self,
        py: Python,
        text_array: &PyArray1<PyObject>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                *text = self.word_match(py, text.as_ref(py)).into_py(py);
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array(
                    py,
                    unsafe { text_array.as_array() }
                        .map(|text| self.word_match(py, text.as_ref(py)).into_py(py)),
                )
                .into(),
            )
        }
    }

    #[pyo3(signature=(text_array, inplace = false))]
    fn numpy_word_match_as_string(
        &self,
        py: Python,
        text_array: &PyArray1<PyObject>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                *text = self.word_match_as_string(py, text.as_ref(py)).into_py(py);
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array(
                    py,
                    unsafe { text_array.as_array() }
                        .map(|text| self.word_match_as_string(py, text.as_ref(py)).into_py(py)),
                )
                .into(),
            )
        }
    }
}

#[pyclass(module = "matcher_py")]
struct SimpleMatcher {
    simple_matcher: SimpleMatcherRs,
    simple_wordlist_dict_bytes: Py<PyBytes>,
}

#[pymethods]
impl SimpleMatcher {
    #[new]
    fn new(simple_wordlist_dict_bytes: &PyBytes) -> PyResult<SimpleMatcher> {
        let simple_wordlist_dict: SimpleWordlistDictRs =
            match rmp_serde::from_slice(simple_wordlist_dict_bytes.as_bytes()) {
                Ok(simple_wordlist_dict) => simple_wordlist_dict,
                Err(e) => return Err(PyValueError::new_err(
                    format!("Deserialize simple_wordlist_dict_bytes failed, Please check the input data.\n Err: {}", e.to_string()),
                )),
            };

        Ok(SimpleMatcher {
            simple_matcher: SimpleMatcherRs::new(&simple_wordlist_dict),
            simple_wordlist_dict_bytes: simple_wordlist_dict_bytes.into(),
        })
    }

    fn __getnewargs__(&self, py: Python) -> (Py<PyBytes>,) {
        (self.simple_wordlist_dict_bytes.clone_ref(py),)
    }

    fn __getstate__(&self, py: Python) -> Py<PyBytes> {
        self.simple_wordlist_dict_bytes.clone_ref(py)
    }

    fn __setstate__(&mut self, simple_wordlist_dict_bytes: &PyBytes) {
        self.simple_matcher = SimpleMatcherRs::new(
            &rmp_serde::from_slice(simple_wordlist_dict_bytes.as_bytes()).unwrap(),
        );
        self.simple_wordlist_dict_bytes = simple_wordlist_dict_bytes.into();
    }

    fn is_match(&self, _py: Python, text: &PyAny) -> bool {
        text.downcast::<PyString>().map_or(false, |text| {
            self.simple_matcher
                .is_match(unsafe { text.to_str().unwrap_unchecked() })
        })
    }

    fn simple_process(&self, _py: Python, text: &PyAny) -> Vec<SimpleResult> {
        text.downcast::<PyString>().map_or(Vec::new(), |text| {
            self.simple_matcher
                .process(unsafe { text.to_str().unwrap_unchecked() })
                .into_iter()
                .map(|simple_result| SimpleResult(simple_result))
                .collect::<Vec<_>>()
        })
    }

    fn batch_simple_process(&self, py: Python, text_array: &PyList) -> Py<PyList> {
        let result_list = PyList::empty(py);

        text_array.iter().for_each(|text| {
            result_list
                .append(self.simple_process(py, text).into_py(py))
                .unwrap();
        });

        result_list.into()
    }

    #[pyo3(signature=(text_array, inplace = false))]
    fn numpy_simple_process(
        &self,
        py: Python,
        text_array: &PyArray1<PyObject>,
        inplace: bool,
    ) -> Option<Py<PyArray1<PyObject>>> {
        if inplace {
            unsafe { text_array.as_array_mut() }.map_inplace(|text| {
                *text = self.simple_process(py, text.as_ref(py)).into_py(py);
            });
            None
        } else {
            Some(
                PyArray1::<PyObject>::from_owned_array(
                    py,
                    unsafe { text_array.as_array() }
                        .map(|text| self.simple_process(py, text.as_ref(py)).into_py(py)),
                )
                .into(),
            )
        }
    }
}

#[pymodule]
fn matcher_py(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Matcher>()?;
    m.add_class::<SimpleMatcher>()?;
    Ok(())
}
