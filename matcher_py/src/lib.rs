use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::{
    Py, PyAny, PyModule, PyResult, Python, pyclass, pymethods, pymodule, wrap_pyfunction,
};
use pyo3::types::{PyAnyMethods, PyModuleMethods, PyString};
use pyo3::{Bound, pyfunction};
use std::borrow::Cow;

use matcher_rs::{ProcessType, SimpleMatcher, SimpleTableSerde, reduce_text_process, text_process};

#[pyclass(name = "ProcessType", eq, from_py_object)]
#[derive(Clone, PartialEq, Eq)]
pub struct PyProcessType(ProcessType);

#[pymethods]
impl PyProcessType {
    #[classattr]
    const NONE: u8 = ProcessType::None.bits();

    #[classattr]
    const FANJIAN: u8 = ProcessType::Fanjian.bits();

    #[classattr]
    const DELETE: u8 = ProcessType::Delete.bits();

    #[classattr]
    const NORMALIZE: u8 = ProcessType::Normalize.bits();

    #[classattr]
    const DELETE_NORMALIZE: u8 = ProcessType::DeleteNormalize.bits();

    #[classattr]
    const FANJIAN_DELETE_NORMALIZE: u8 = ProcessType::FanjianDeleteNormalize.bits();

    #[classattr]
    const PINYIN: u8 = ProcessType::PinYin.bits();

    #[classattr]
    const PINYIN_CHAR: u8 = ProcessType::PinYinChar.bits();

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

#[pyclass(name = "SimpleResult")]
pub struct PySimpleResult {
    #[pyo3(get)]
    pub word_id: u32,
    #[pyo3(get)]
    pub word: Py<PyString>,
}

#[pyfunction(name = "text_process")]
#[pyo3(signature=(process_type, text))]
fn py_text_process<'a>(process_type: Bound<'_, PyAny>, text: &'a str) -> PyResult<Cow<'a, str>> {
    let p_type = if let Ok(bits) = process_type.extract::<u8>() {
        ProcessType::from_bits_retain(bits)
    } else if let Ok(pt) = process_type.extract::<PyProcessType>() {
        pt.0
    } else {
        return Err(PyTypeError::new_err(
            "process_type must be ProcessType or int",
        ));
    };

    Ok(text_process(p_type, text))
}

#[pyfunction(name = "reduce_text_process")]
#[pyo3(signature=(process_type, text))]
fn py_reduce_text_process<'a>(
    process_type: Bound<'_, PyAny>,
    text: &'a str,
) -> PyResult<Vec<Cow<'a, str>>> {
    let p_type = if let Ok(bits) = process_type.extract::<u8>() {
        ProcessType::from_bits_retain(bits)
    } else if let Ok(pt) = process_type.extract::<PyProcessType>() {
        pt.0
    } else {
        return Err(PyTypeError::new_err(
            "process_type must be ProcessType or int",
        ));
    };

    Ok(reduce_text_process(p_type, text).into_iter().collect())
}

#[pyclass(name = "SimpleMatcher")]
pub struct PySimpleMatcher {
    simple_matcher: SimpleMatcher,
    simple_table_bytes: Vec<u8>,
}

#[pymethods]
impl PySimpleMatcher {
    #[new]
    #[pyo3(signature=(simple_table_bytes))]
    fn new(_py: Python, simple_table_bytes: &[u8]) -> PyResult<PySimpleMatcher> {
        let simple_table: SimpleTableSerde = match sonic_rs::from_slice(simple_table_bytes) {
            Ok(simple_table) => simple_table,
            Err(e) => {
                return Err(PyValueError::new_err(format!(
                    "Deserialize simple_table_bytes failed, Please check the input data.\n Err: {}",
                    e
                )));
            }
        };

        Ok(PySimpleMatcher {
            simple_matcher: SimpleMatcher::new(&simple_table),
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
    fn __setstate__(&mut self, simple_table_bytes: &[u8]) -> PyResult<()> {
        let simple_table: SimpleTableSerde = match sonic_rs::from_slice(simple_table_bytes) {
            Ok(simple_table) => simple_table,
            Err(e) => {
                return Err(PyValueError::new_err(format!(
                    "Deserialize simple_table_bytes failed: {}",
                    e
                )));
            }
        };
        self.simple_matcher = SimpleMatcher::new(&simple_table);
        self.simple_table_bytes = simple_table_bytes.to_vec();
        Ok(())
    }

    #[pyo3(signature=(text))]
    fn is_match(&self, text: &str) -> bool {
        self.simple_matcher.is_match(text)
    }

    #[pyo3(signature=(text))]
    fn process(&self, py: Python<'_>, text: &str) -> Vec<PySimpleResult> {
        self.simple_matcher
            .process(text)
            .into_iter()
            .map(|res| PySimpleResult {
                word_id: res.word_id,
                word: PyString::new(py, &res.word).into(),
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
