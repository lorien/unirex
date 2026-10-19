//! PyO3 binding for the UCS-native engine.
//!
//! The binding uses CPython's own `PyUnicode_KIND`, `PyUnicode_DATA`, and
//! `PyUnicode_GET_LENGTH` accessors instead of private PEP 393 offsets. It
//! never calls the UTF-8 encoder. Result strings come from
//! `PyUnicode_Substring`, so spans are reused directly.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

use pyo3::Bound;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyList, PyString};

use crate::{Regex as CoreRegex, Scratch, Ucs};

/// Long haystacks release the GIL around the scan. Short strings stay
/// GIL-held, matching CPython `re` and the `rurex` short-string path.
const DETACH_THRESHOLD: usize = 4096;

/// Retained scratches per thread. A scratch owns the lazy DFA cache, so the
/// cap bounds memory when a thread uses several patterns.
const MAX_THREAD_SCRATCHES: usize = 4;

static NEXT_PATTERN_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    static SCRATCHES: RefCell<Vec<(u64, Scratch)>> = const { RefCell::new(Vec::new()) };
}

/// A compiled pattern exposed to Python.
#[pyclass(name = "Regex", module = "unirex")]
pub struct PyRegex {
    inner: CoreRegex,
    /// Stable identity used to match a thread-local scratch to this pattern.
    id: u64,
}

impl PyRegex {
    fn take_scratch(&self) -> Scratch {
        let id = self.id;
        let found = SCRATCHES.try_with(|cell| {
            let mut pool = cell.borrow_mut();
            let index = pool.iter().position(|(owner, _)| *owner == id);
            index.map(|index| pool.swap_remove(index).1)
        });
        match found {
            Ok(Some(scratch)) => scratch,
            _ => self.inner.scratch(),
        }
    }

    fn put_scratch(&self, scratch: Scratch) {
        let id = self.id;
        let _ = SCRATCHES.try_with(|cell| {
            let mut pool = cell.borrow_mut();
            if pool.len() >= MAX_THREAD_SCRATCHES {
                pool.remove(0);
            }
            pool.push((id, scratch));
        });
    }
}

impl Drop for PyRegex {
    fn drop(&mut self) {
        let id = self.id;
        let _ = SCRATCHES.try_with(|cell| {
            cell.borrow_mut().retain(|(owner, _)| *owner != id);
        });
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
            id: NEXT_PATTERN_ID.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// Return all non-overlapping matches as strings.
    ///
    /// The current engine exposes group 0 only; patterns with capture groups
    /// return the whole match, not group tuples.
    fn findall<'py>(&self, py: Python<'py>, text: &Bound<'_, PyString>) -> PyResult<Py<PyList>> {
        let ucs = py_ucs(text)?;
        let mut scratch = self.take_scratch();
        let list = PyList::empty(py);
        let result = if ucs.len() < DETACH_THRESHOLD {
            for found in self.inner.find_iter_with(&mut scratch, ucs) {
                list.append(substring(py, text, found.start(), found.end())?)?;
            }
            Ok(list)
        } else {
            let spans = collect_spans(&self.inner, &mut scratch, ucs, py);
            for (start, end) in spans {
                list.append(substring(py, text, start, end)?)?;
            }
            Ok(list)
        };
        self.put_scratch(scratch);
        result.map(Bound::unbind)
    }

    /// Return the first match as a string, or `None`.
    fn find<'py>(
        &self,
        py: Python<'py>,
        text: &Bound<'_, PyString>,
    ) -> PyResult<Option<Py<PyString>>> {
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
            .map(|m| substring(py, text, m.start(), m.end()).map(Bound::unbind))
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
        "unirex.Regex".to_string()
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

fn substring<'py>(
    py: Python<'py>,
    text: &Bound<'_, PyString>,
    start: usize,
    end: usize,
) -> PyResult<Bound<'py, PyString>> {
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
        Ok(bound.cast_into::<PyString>()?)
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
fn unirex(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyRegex>()?;
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    Ok(())
}
