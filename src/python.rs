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
use pyo3::types::{PyAny, PyDict, PyList, PyString, PyTuple};

use crate::{Match as CoreMatch, Regex as CoreRegex, Scratch, Ucs};

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
            .map_err(|err| Python::attach(|py| compile_error(py, pattern, &err)))?;
        Ok(PyRegex {
            inner,
            id: NEXT_PATTERN_ID.fetch_add(1, Ordering::Relaxed),
        })
    }

    /// Number of capture groups, excluding group 0.
    #[getter]
    fn groups(&self) -> usize {
        self.inner.group_count()
    }

    /// Mapping of capture name to group index, as in `re.Pattern.groupindex`.
    #[getter]
    fn groupindex<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let dict = PyDict::new(py);
        for (name, index) in self.inner.capture_names() {
            dict.set_item(name, index)?;
        }
        Ok(dict)
    }

    /// Return all non-overlapping matches.
    ///
    /// With no capture groups this is a list of matched strings, matching
    /// `re.findall`. With groups it is a list of group strings, or a list of
    /// tuples of group strings when the pattern has more than one group.
    /// Unmatched groups appear as empty strings, as in `re.findall`.
    fn findall<'py>(&self, py: Python<'py>, text: &Bound<'_, PyString>) -> PyResult<Py<PyList>> {
        let ucs = py_ucs(text)?;
        let mut scratch = self.take_scratch();
        let list = PyList::empty(py);
        let groups = self.inner.group_count();
        let result = if groups == 0 {
            if ucs.len() < DETACH_THRESHOLD {
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
            }
        } else {
            let width = 2 * (groups + 1);
            let found = collect_spans_and_slots(&self.inner, &mut scratch, ucs, py, width);
            for (start, end, slots) in found {
                if groups == 1 {
                    list.append(group_value(py, text, start, &slots, 1)?)?;
                } else {
                    let mut values = Vec::with_capacity(groups);
                    for index in 1..=groups {
                        values.push(group_value(py, text, start, &slots, index)?);
                    }
                    list.append(PyTuple::new(py, values)?)?;
                }
                let _ = end;
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

    /// Return the first match at or after position zero.
    fn search(&self, py: Python<'_>, text: &Bound<'_, PyString>) -> PyResult<Option<Py<PyMatch>>> {
        self.match_with_position(py, text, false, false)
    }

    /// Match only at the start of `text`.
    #[pyo3(name = "match")]
    fn match_(&self, py: Python<'_>, text: &Bound<'_, PyString>) -> PyResult<Option<Py<PyMatch>>> {
        self.match_with_position(py, text, true, false)
    }

    /// Match the whole of `text`.
    fn fullmatch(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyString>,
    ) -> PyResult<Option<Py<PyMatch>>> {
        self.match_with_position(py, text, true, true)
    }

    /// Replace matches, returning the new string.
    #[pyo3(signature = (repl, text, count = 0))]
    fn sub(
        &self,
        py: Python<'_>,
        repl: &Bound<'_, PyAny>,
        text: &Bound<'_, PyString>,
        count: usize,
    ) -> PyResult<Py<PyString>> {
        self.sub_impl(py, repl, text, count)
            .map(|(result, _)| result)
    }

    /// Replace matches, returning the new string and the replacement count.
    #[pyo3(signature = (repl, text, count = 0))]
    fn subn(
        &self,
        py: Python<'_>,
        repl: &Bound<'_, PyAny>,
        text: &Bound<'_, PyString>,
        count: usize,
    ) -> PyResult<(Py<PyString>, usize)> {
        self.sub_impl(py, repl, text, count)
    }

    /// Split `text` on matches, keeping capture groups.
    #[pyo3(signature = (text, maxsplit = 0))]
    fn split(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyString>,
        maxsplit: isize,
    ) -> PyResult<Py<PyList>> {
        self.split_impl(py, text, maxsplit)
    }

    /// Iterate over matches, yielding [`PyMatch`] objects.
    fn finditer(&self, py: Python<'_>, text: &Bound<'_, PyString>) -> PyResult<PyFindIter> {
        let ucs = py_ucs(text)?;
        let mut scratch = self.take_scratch();
        let width = 2 * (self.inner.group_count() + 1);
        let found = collect_spans_and_slots(&self.inner, &mut scratch, ucs, py, width);
        self.put_scratch(scratch);
        let matches = found.into_iter().map(|(_, _, slots)| slots).collect();
        Ok(PyFindIter {
            text: text.clone().unbind(),
            matches,
            position: 0,
            names: self
                .inner
                .capture_names()
                .map(|(name, index)| (name.to_string(), index))
                .collect(),
        })
    }

    fn match_with_position(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyString>,
        anchored_start: bool,
        anchored_end: bool,
    ) -> PyResult<Option<Py<PyMatch>>> {
        let ucs = py_ucs(text)?;
        let mut scratch = self.take_scratch();
        let width = 2 * (self.inner.group_count() + 1);
        let found = self.inner.find_with(&mut scratch, ucs);
        let result = match found {
            Some(found) => {
                let ok = (!anchored_start || found.start() == 0)
                    && (!anchored_end || found.end() == ucs.len());
                if ok {
                    let mut slots = vec![usize::MAX; width];
                    if self.inner.captures(&mut scratch, ucs, found, &mut slots) {
                        Some(Py::new(
                            py,
                            PyMatch {
                                text: text.clone().unbind(),
                                slots,
                                names: self.names(),
                            },
                        )?)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            None => None,
        };
        self.put_scratch(scratch);
        Ok(result)
    }

    fn names(&self) -> Vec<(String, usize)> {
        self.inner
            .capture_names()
            .map(|(name, index)| (name.to_string(), index))
            .collect()
    }

    fn matches_with_slots(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyString>,
    ) -> PyResult<Vec<Vec<usize>>> {
        let ucs = py_ucs(text)?;
        let mut scratch = self.take_scratch();
        let width = 2 * (self.inner.group_count() + 1);
        let found = collect_spans_and_slots(&self.inner, &mut scratch, ucs, py, width);
        self.put_scratch(scratch);
        Ok(found.into_iter().map(|(_, _, slots)| slots).collect())
    }

    fn sub_impl(
        &self,
        py: Python<'_>,
        repl: &Bound<'_, PyAny>,
        text: &Bound<'_, PyString>,
        count: usize,
    ) -> PyResult<(Py<PyString>, usize)> {
        let all = self.matches_with_slots(py, text)?;
        let names = self.names();
        let hay = text.to_cow()?.into_owned();
        let limit = if count == 0 {
            all.len()
        } else {
            count.min(all.len())
        };
        let mut pieces: Vec<String> = Vec::new();
        let mut previous = 0usize;
        let mut replaced = 0usize;
        for (position, slots) in all.iter().enumerate() {
            let (start, end) = (slots[0], slots[1]);
            if start == usize::MAX || end == usize::MAX {
                continue;
            }
            if position >= limit {
                break;
            }
            let prefix: String = char_slice(&hay, previous, start);
            pieces.push(prefix);
            let value = if repl.is_callable() {
                let m = Py::new(
                    py,
                    PyMatch {
                        text: text.clone().unbind(),
                        slots: slots.clone(),
                        names: names.clone(),
                    },
                )?;
                let out = repl.call1((m,))?;
                out.extract::<String>()?
            } else {
                let template = repl.extract::<String>()?;
                expand_replacement(&template, &hay, slots, &names)
            };
            pieces.push(value);
            previous = end;
            replaced += 1;
        }
        pieces.push(char_slice(&hay, previous, hay.chars().count()));
        let result = pieces.concat();
        Ok((PyString::new(py, &result).unbind(), replaced))
    }

    fn split_impl(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyString>,
        maxsplit: isize,
    ) -> PyResult<Py<PyList>> {
        let all = self.matches_with_slots(py, text)?;
        let hay = text.to_cow()?.into_owned();
        let list = PyList::empty(py);
        let limit = if maxsplit <= 0 {
            all.len()
        } else {
            (maxsplit as usize).min(all.len())
        };
        let mut previous = 0usize;
        for (position, slots) in all.iter().enumerate() {
            if position >= limit {
                break;
            }
            let (start, end) = (slots[0], slots[1]);
            if start == usize::MAX || end == usize::MAX {
                continue;
            }
            list.append(PyString::new(py, &char_slice(&hay, previous, start)))?;
            for index in 1..(slots.len() / 2) {
                let (lo, hi) = (slots[2 * index], slots[2 * index + 1]);
                let value = if lo == usize::MAX || hi == usize::MAX {
                    None
                } else {
                    Some(char_slice(&hay, lo, hi))
                };
                list.append(value)?;
            }
            previous = end;
        }
        list.append(PyString::new(
            py,
            &char_slice(&hay, previous, hay.chars().count()),
        ))?;
        Ok(list.unbind())
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

/// Collect match spans and their capture slots, releasing the GIL for long
/// haystacks.
fn collect_spans_and_slots(
    regex: &CoreRegex,
    scratch: &mut Scratch,
    ucs: Ucs<'_>,
    py: Python<'_>,
    width: usize,
) -> Vec<(usize, usize, Vec<usize>)> {
    let mut run = || {
        let mut spans = Vec::new();
        for found in regex.find_iter_with(scratch, ucs) {
            spans.push((found.start(), found.end()));
        }
        let mut slots = vec![usize::MAX; width];
        let mut out = Vec::with_capacity(spans.len());
        for (start, end) in spans {
            slots.fill(usize::MAX);
            let m = CoreMatch::new(start, end);
            if regex.captures(scratch, ucs, m, &mut slots) {
                out.push((start, end, slots.clone()));
            }
        }
        out
    };
    if ucs.len() >= DETACH_THRESHOLD {
        py.detach(run)
    } else {
        run()
    }
}

/// A single group as a `str`, with unmatched groups becoming `""`.
///
/// `start` is the whole-match start, used to produce the empty string without
/// an extra allocation path.
fn group_value<'py>(
    py: Python<'py>,
    text: &Bound<'_, PyString>,
    start: usize,
    slots: &[usize],
    index: usize,
) -> PyResult<Bound<'py, PyString>> {
    let lo = slots[2 * index];
    let hi = slots[2 * index + 1];
    if lo == usize::MAX || hi == usize::MAX {
        substring(py, text, start, start)
    } else {
        substring(py, text, lo, hi)
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

/// A single match with its capture spans.
#[pyclass(name = "Match", module = "unirex")]
pub struct PyMatch {
    text: Py<PyString>,
    slots: Vec<usize>,
    names: Vec<(String, usize)>,
}

fn resolve_index(names: &[(String, usize)], index: Option<&Bound<'_, PyAny>>) -> PyResult<usize> {
    let Some(index) = index else {
        return Ok(0);
    };
    if let Ok(number) = index.extract::<isize>() {
        if number < 0 {
            return Err(pyo3::exceptions::PyIndexError::new_err(
                "no such group: negative index",
            ));
        }
        return Ok(number as usize);
    }
    if let Ok(name) = index.extract::<String>() {
        return names
            .iter()
            .find(|(existing, _)| *existing == name)
            .map(|(_, index)| *index)
            .ok_or_else(|| {
                pyo3::exceptions::PyIndexError::new_err(format!("no such group: {name}"))
            });
    }
    Err(pyo3::exceptions::PyTypeError::new_err(
        "group index must be an int or a str",
    ))
}

impl PyMatch {
    fn span_of(&self, index: usize) -> Option<(usize, usize)> {
        let lo = *self.slots.get(2 * index)?;
        let hi = *self.slots.get(2 * index + 1)?;
        if lo == usize::MAX || hi == usize::MAX {
            None
        } else {
            Some((lo, hi))
        }
    }
}

#[pymethods]
impl PyMatch {
    #[pyo3(signature = (index = None))]
    fn span(&self, index: Option<&Bound<'_, PyAny>>) -> PyResult<(isize, isize)> {
        let index = resolve_index(&self.names, index)?;
        // Python reports `(-1, -1)` for a group that did not participate.
        Ok(match self.span_of(index) {
            Some((start, end)) => (start as isize, end as isize),
            None => (-1, -1),
        })
    }

    #[pyo3(signature = (index = None))]
    fn start(&self, index: Option<&Bound<'_, PyAny>>) -> PyResult<isize> {
        let index = resolve_index(&self.names, index)?;
        Ok(self.span_of(index).map_or(-1, |(start, _)| start as isize))
    }

    #[pyo3(signature = (index = None))]
    fn end(&self, index: Option<&Bound<'_, PyAny>>) -> PyResult<isize> {
        let index = resolve_index(&self.names, index)?;
        Ok(self.span_of(index).map_or(-1, |(_, end)| end as isize))
    }

    #[pyo3(signature = (index = None))]
    fn group(
        &self,
        py: Python<'_>,
        index: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Option<Py<PyString>>> {
        let index = resolve_index(&self.names, index)?;
        let Some((start, end)) = self.span_of(index) else {
            return Ok(None);
        };
        substring(py, self.text.bind(py), start, end).map(|s| Some(s.unbind()))
    }

    fn groups<'py>(&self, py: Python<'py>) -> PyResult<Py<PyTuple>> {
        let count = self.slots.len() / 2;
        let mut items: Vec<Option<Py<PyString>>> = Vec::with_capacity(count.saturating_sub(1));
        for index in 1..count {
            let value = match self.span_of(index) {
                Some((start, end)) => Some(substring(py, self.text.bind(py), start, end)?.unbind()),
                None => None,
            };
            items.push(value);
        }
        Ok(PyTuple::new(py, items)?.unbind())
    }

    fn groupdict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let dict = PyDict::new(py);
        for (name, index) in &self.names {
            let value = match self.span_of(*index) {
                Some((start, end)) => Some(substring(py, self.text.bind(py), start, end)?.unbind()),
                None => None,
            };
            dict.set_item(name, value)?;
        }
        Ok(dict)
    }

    fn __repr__(&self) -> String {
        match self.span_of(0) {
            Some((start, end)) => format!("<unirex.Match span=({start}, {end})>"),
            None => "<unirex.Match unmatched>".to_string(),
        }
    }
}

/// Iterator over matches produced by [`PyRegex::finditer`].
///
/// Spans and capture slots are collected up front, so iteration itself is
/// cheap and does not hold the GIL beyond building each `Match`.
#[pyclass(name = "FindIter", module = "unirex")]
pub struct PyFindIter {
    text: Py<PyString>,
    matches: Vec<Vec<usize>>,
    position: usize,
    names: Vec<(String, usize)>,
}

#[pymethods]
impl PyFindIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<'_, Self>, py: Python<'_>) -> Option<Py<PyMatch>> {
        let slots = slf.matches.get(slf.position)?.clone();
        slf.position += 1;
        Some(
            Py::new(
                py,
                PyMatch {
                    text: slf.text.clone_ref(py),
                    slots,
                    names: slf.names.clone(),
                },
            )
            .expect("Match construction cannot fail"),
        )
    }
}

pyo3::create_exception!(
    unirex,
    Error,
    pyo3::exceptions::PyException,
    "Pattern compilation error."
);

/// Build a `unirex.error` carrying the same attributes as `re.error`.
fn compile_error(py: Python<'_>, pattern: &str, err: &crate::Error) -> PyErr {
    let message = err.to_string();
    let offset = match err {
        crate::Error::Parse(inner) => Some(inner.span().start.offset),
        other => other.offset(),
    };
    let ty = py.get_type::<Error>();
    let instance = match ty.call1((message.clone(),)) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let _ = instance.setattr("msg", message);
    let _ = instance.setattr("pattern", pattern);
    let _ = instance.setattr("pos", offset);
    let _ = instance.setattr("lineno", 1);
    let _ = instance.setattr("colno", offset.map(|pos| pos + 1));
    PyErr::from_value(instance)
}

#[pymodule]
fn unirex(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_class::<PyRegex>()?;
    m.add_class::<PyMatch>()?;
    m.add_class::<PyFindIter>()?;
    m.add("error", m.py().get_type::<Error>())?;
    m.add("PatternError", m.py().get_type::<Error>())?;
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    Ok(())
}

/// Character slice of `hay` between two code-point offsets.
fn char_slice(hay: &str, start: usize, end: usize) -> String {
    hay.chars()
        .skip(start)
        .take(end.saturating_sub(start))
        .collect()
}

/// Expand `\1`, `\g<name>`, and `\\` in a replacement template.
fn expand_replacement(
    template: &str,
    hay: &str,
    slots: &[usize],
    names: &[(String, usize)],
) -> String {
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.peek().copied() {
            Some('\\') => {
                chars.next();
                out.push('\\');
            }
            Some('g') => {
                chars.next();
                let mut name = String::new();
                if chars.peek() == Some(&'<') {
                    chars.next();
                    for c in chars.by_ref() {
                        if c == '>' {
                            break;
                        }
                        name.push(c);
                    }
                }
                let index = name
                    .parse::<usize>()
                    .ok()
                    .or_else(|| names.iter().find(|(n, _)| *n == name).map(|(_, i)| *i));
                if let Some(index) = index {
                    if let (Some(&lo), Some(&hi)) = (slots.get(2 * index), slots.get(2 * index + 1))
                    {
                        if lo != usize::MAX && hi != usize::MAX {
                            out.push_str(&char_slice(hay, lo, hi));
                        }
                    }
                }
            }
            Some(digit) if digit.is_ascii_digit() => {
                chars.next();
                if let Some(index) = digit.to_digit(10) {
                    let index = index as usize;
                    if let (Some(&lo), Some(&hi)) = (slots.get(2 * index), slots.get(2 * index + 1))
                    {
                        if lo != usize::MAX && hi != usize::MAX {
                            out.push_str(&char_slice(hay, lo, hi));
                        }
                    }
                }
            }
            Some('n') => {
                chars.next();
                out.push('\n');
            }
            Some('t') => {
                chars.next();
                out.push('\t');
            }
            Some(other) => {
                chars.next();
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}
