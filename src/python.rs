//! PyO3 binding for the UCS-native engine.
//!
//! The binding uses CPython's own `PyUnicode_KIND`, `PyUnicode_DATA`, and
//! `PyUnicode_GET_LENGTH` accessors instead of private PEP 393 offsets. It
//! never calls the UTF-8 encoder. Result strings come from
//! `PyUnicode_Substring`, so spans are reused directly.

use std::sync::Mutex;

use pyo3::Bound;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyString;

use crate::{Regex as CoreRegex, Scratch, Ucs};

/// Long haystacks release the GIL around the scan. Short strings stay
/// GIL-held, matching CPython `re` and the `rurex` short-string path.
const DETACH_THRESHOLD: usize = 4096;

/// A compiled pattern exposed to Python.
#[pyclass(name = "Regex", module = "exp_rurex")]
pub struct PyRegex {
    inner: CoreRegex,
    scratch_pool: Mutex<Vec<Scratch>>,
}

impl PyRegex {
    fn take_scratch(&self) -> Scratch {
        self.scratch_pool
            .lock()
            .ok()
            .and_then(|mut pool| pool.pop())
            .unwrap_or_else(|| self.inner.scratch())
    }

    fn put_scratch(&self, scratch: Scratch) {
        if let Ok(mut pool) = self.scratch_pool.lock() {
            pool.push(scratch);
        }
    }
}

#[pymethods]
impl PyRegex {
    #[new]
    fn new(pattern: &str) -> PyResult<PyRegex> {
        let inner = CoreRegex::new(pattern)
            .map_err(|err| pyo3::exceptions::PyValueError::new_err(err.to_string()))?;
        Ok(PyRegex {
            inner,
            scratch_pool: Mutex::new(Vec::new()),
        })
    }

    /// Return all non-overlapping matches as strings.
    ///
    /// The current engine exposes group 0 only; patterns with capture groups
    /// return the whole match, not group tuples.
    fn findall(&self, py: Python<'_>, text: &Bound<'_, PyString>) -> PyResult<Vec<Py<PyString>>> {
        let ucs = py_ucs(text)?;
        let mut scratch = self.take_scratch();
        let result = if ucs.len() < DETACH_THRESHOLD {
            let mut matches = Vec::new();
            for found in self.inner.find_iter_with(&mut scratch, ucs) {
                matches.push(substring(py, text, found.start(), found.end())?);
            }
            Ok(matches)
        } else {
            let spans = collect_spans(&self.inner, &mut scratch, ucs, py);
            spans
                .into_iter()
                .map(|(start, end)| substring(py, text, start, end))
                .collect()
        };
        self.put_scratch(scratch);
        result
    }

    /// Return the first match as a string, or `None`.
    fn find(&self, py: Python<'_>, text: &Bound<'_, PyString>) -> PyResult<Option<Py<PyString>>> {
        let ucs = py_ucs(text)?;
        let mut scratch = self.take_scratch();
        let found = {
            let mut run = || self.inner.find_with(&mut scratch, ucs);
            if ucs.len() >= DETACH_THRESHOLD {
                py.detach(run)
            } else {
                run()
            }
        };
        self.put_scratch(scratch);
        found
            .map(|m| substring(py, text, m.start(), m.end()))
            .transpose()
    }

    /// Whether the pattern matches anywhere in `text`.
    fn is_match(&self, py: Python<'_>, text: &Bound<'_, PyString>) -> PyResult<bool> {
        let ucs = py_ucs(text)?;
        let mut scratch = self.take_scratch();
        let found = {
            let mut run = || self.inner.is_match_with(&mut scratch, ucs);
            if ucs.len() >= DETACH_THRESHOLD {
                py.detach(run)
            } else {
                run()
            }
        };
        self.put_scratch(scratch);
        Ok(found)
    }

    fn __repr__(&self) -> String {
        "exp_rurex.Regex".to_string()
    }
}

fn collect_spans(
    regex: &CoreRegex,
    scratch: &mut Scratch,
    ucs: Ucs<'_>,
    py: Python<'_>,
) -> Vec<(usize, usize)> {
    let mut run = || {
        let mut spans = Vec::new();
        for found in regex.find_iter_with(scratch, ucs) {
            spans.push((found.start(), found.end()));
        }
        spans
    };
    if ucs.len() >= DETACH_THRESHOLD {
        py.detach(run)
    } else {
        run()
    }
}

fn substring(
    py: Python<'_>,
    text: &Bound<'_, PyString>,
    start: usize,
    end: usize,
) -> PyResult<Py<PyString>> {
    let start = isize::try_from(start)
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("match start overflow"))?;
    let end = isize::try_from(end)
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("match end overflow"))?;
    let ptr = unsafe { ffi::PyUnicode_Substring(text.as_ptr(), start, end) };
    if ptr.is_null() {
        return Err(PyErr::fetch(py));
    }
    unsafe {
        let bound = Bound::from_owned_ptr(py, ptr);
        Ok(bound.cast_into::<PyString>()?.unbind())
    }
}

/// Borrow CPython's UCS buffer as a `Ucs` view.
///
/// # Safety
///
/// `text` is a live `str` object. CPython strings are immutable, and the
/// returned slices live no longer than the `Bound` borrow. `PyUnicode_READY`
/// ensures legacy strings are materialized before `DATA` is read. No Python
/// API is called while the slices are used.
fn py_ucs<'a>(text: &'a Bound<'_, PyString>) -> PyResult<Ucs<'a>> {
    let ptr = text.as_ptr();
    unsafe {
        if ffi::PyUnicode_READY(ptr) != 0 {
            return Err(PyErr::fetch(text.py()));
        }
        let length = ffi::PyUnicode_GET_LENGTH(ptr);
        let length = usize::try_from(length)
            .map_err(|_| pyo3::exceptions::PyValueError::new_err("negative string length"))?;
        if length == 0 {
            return Ok(match ffi::PyUnicode_KIND(ptr) {
                ffi::PyUnicode_1BYTE_KIND => Ucs::Ucs1(&[]),
                ffi::PyUnicode_2BYTE_KIND => Ucs::Ucs2(&[]),
                ffi::PyUnicode_4BYTE_KIND => Ucs::Ucs4(&[]),
                _ => {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "unsupported CPython string kind",
                    ));
                }
            });
        }
        let data = ffi::PyUnicode_DATA(ptr);
        match ffi::PyUnicode_KIND(ptr) {
            ffi::PyUnicode_1BYTE_KIND => Ok(Ucs::Ucs1(std::slice::from_raw_parts(
                data.cast::<u8>(),
                length,
            ))),
            ffi::PyUnicode_2BYTE_KIND => Ok(Ucs::Ucs2(std::slice::from_raw_parts(
                data.cast::<u16>(),
                length,
            ))),
            ffi::PyUnicode_4BYTE_KIND => Ok(Ucs::Ucs4(std::slice::from_raw_parts(
                data.cast::<u32>(),
                length,
            ))),
            _ => Err(pyo3::exceptions::PyValueError::new_err(
                "unsupported CPython string kind",
            )),
        }
    }
}

#[pyfunction]
fn compile(pattern: &str) -> PyResult<PyRegex> {
    PyRegex::new(pattern)
}

#[pymodule]
fn exp_rurex(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyRegex>()?;
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    Ok(())
}
