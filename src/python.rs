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
use pyo3::types::{PyAny, PyBytes, PyDict, PyList, PyString, PyTuple};

use crate::{Match as CoreMatch, Pattern as CorePattern, Scratch, Ucs};

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
#[pyclass(name = "Pattern", module = "unirex", weakref)]
pub struct PyPattern {
    inner: CorePattern,
    /// Stable identity used to match a thread-local scratch to this pattern.
    id: u64,
    /// The source pattern, as passed to `compile`.
    pattern: String,
    /// The `re` flag bits the pattern was compiled with.
    flags: u32,
    /// Compiled from a `bytes` pattern, so subjects and results are bytes.
    bytes: bool,
}

impl PyPattern {
    fn with_flags(pattern: &str, flags: u32) -> PyResult<PyPattern> {
        PyPattern::build(pattern, flags, false)
    }

    fn build(pattern: &str, flags: u32, bytes: bool) -> PyResult<PyPattern> {
        // `re` reports UNICODE for str patterns compiled without flags.
        let flags = if bytes { flags } else { flags | FLAG_UNICODE };
        let prefix = flags_prefix(flags)?;
        let source = if prefix.is_empty() {
            pattern.to_string()
        } else {
            format!("{prefix}{pattern}")
        };
        let compiled = if bytes {
            CorePattern::new_bytes(&source.chars().map(|ch| ch as u8).collect::<Vec<u8>>())
        } else {
            CorePattern::new(&source)
        };
        let inner =
            compiled.map_err(|err| Python::attach(|py| compile_error(py, pattern, &err)))?;
        Ok(PyPattern {
            inner,
            id: NEXT_PATTERN_ID.fetch_add(1, Ordering::Relaxed),
            pattern: pattern.to_string(),
            flags,
            bytes,
        })
    }

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

impl Drop for PyPattern {
    fn drop(&mut self) {
        let id = self.id;
        let _ = SCRATCHES.try_with(|cell| {
            cell.borrow_mut().retain(|(owner, _)| *owner != id);
        });
    }
}

#[pymethods]
impl PyPattern {
    #[new]
    fn new(pattern: &str) -> PyResult<PyPattern> {
        PyPattern::with_flags(pattern, 0)
    }

    /// Support `pickle` and `copy` by rebuilding from the source pattern.
    fn __reduce__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyTuple>> {
        let module = py.import("unirex")?;
        let compile = module.getattr("compile")?;
        let source: Bound<'_, PyAny> = if self.bytes {
            let raw: Vec<u8> = self.pattern.chars().map(|ch| ch as u8).collect();
            PyBytes::new(py, &raw).into_any()
        } else {
            PyString::new(py, &self.pattern).into_any()
        };
        let args = PyTuple::new(py, [source, self.flags.into_pyobject(py)?.into_any()])?;
        PyTuple::new(py, [compile, args.into_any()])
    }

    /// A cursor over `string`, as in `re.Pattern.scanner`.
    #[pyo3(signature = (string, pos = 0))]
    fn scanner(
        &self,
        py: Python<'_>,
        string: &Bound<'_, PyString>,
        pos: usize,
    ) -> PyResult<Py<PyScanner>> {
        let endpos = string.to_cow().map(|c| c.chars().count()).unwrap_or(0);
        Py::new(
            py,
            PyScanner {
                text: string.clone().unbind(),
                pattern_text: self.pattern.clone(),
                flags: self.flags,
                position: pos,
                endpos,
            },
        )
    }

    /// The pattern this was compiled from; `bytes` when the input was bytes.
    #[getter]
    fn pattern<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        if self.bytes {
            let raw: Vec<u8> = self.pattern.chars().map(|ch| ch as u8).collect();
            return Ok(PyBytes::new(py, &raw).into_any());
        }
        Ok(PyString::new(py, &self.pattern).into_any())
    }

    /// The `re` flag bits this pattern was compiled with.
    #[getter]
    fn flags(&self) -> u32 {
        self.flags
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
    #[pyo3(signature = (string, pos = 0, endpos = None))]
    fn findall<'py>(
        &self,
        py: Python<'py>,
        string: &Bound<'_, PyAny>,
        pos: usize,
        endpos: Option<usize>,
    ) -> PyResult<Py<PyList>> {
        if let Some(converted) = as_byte_subject(py, string)? {
            return self.findall(py, converted.as_any(), pos, endpos);
        }
        self.check_subject(string)?;
        let _ = endpos;
        let text = string;
        if pos != 0 {
            let text = text.cast::<PyString>().map_err(|_| {
                pyo3::exceptions::PyTypeError::new_err("windowed findall needs a str subject")
            })?;
            let hay = text.to_cow()?.into_owned();
            let total = hay.chars().count();
            let end = endpos.unwrap_or(total).min(total);
            let window: String = hay
                .chars()
                .skip(pos)
                .take(end.saturating_sub(pos))
                .collect();
            let slice = PyString::new(py, &window);
            let inner = self.findall(py, slice.as_any(), 0, None)?;
            return Ok(inner);
        }

        if let Some(raw) = py_bytes(text) {
            let list = PyList::empty(py);
            let mut scratch = self.take_scratch();
            let width = 2 * (self.inner.group_count() + 1);
            let spans: Vec<(usize, usize, Vec<usize>)> = {
                let found: Vec<crate::Match> =
                    self.inner.find_iter_bytes_with(&mut scratch, raw).collect();
                let mut slots = vec![usize::MAX; width];
                let mut out = Vec::with_capacity(found.len());
                for m in found {
                    slots.fill(usize::MAX);
                    if self
                        .inner
                        .captures(&mut scratch, Ucs::Ucs1(raw), m, &mut slots)
                    {
                        out.push((m.start(), m.end(), slots.clone()));
                    }
                }
                out
            };
            self.put_scratch(scratch);
            for (start, end, slots) in spans {
                if self.inner.group_count() == 0 {
                    list.append(PyBytes::new(py, &raw[start..end]))?;
                } else {
                    let mut values: Vec<Py<PyBytes>> = Vec::new();
                    for index in 1..=(self.inner.group_count()) {
                        let (lo, hi) = (slots[2 * index], slots[2 * index + 1]);
                        let slice: &[u8] = if lo == usize::MAX || hi == usize::MAX {
                            &[]
                        } else {
                            &raw[lo..hi]
                        };
                        values.push(PyBytes::new(py, slice).unbind());
                    }
                    if values.len() == 1 {
                        list.append(Py::clone_ref(&values[0], py))?;
                    } else {
                        list.append(PyTuple::new(py, values)?)?;
                    }
                }
            }
            return Ok(list.unbind());
        }
        let text = text
            .cast::<PyString>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected str or bytes"))?;
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

    /// Return the first match at or after `pos`, before `endpos`.
    #[pyo3(signature = (string, pos = 0, endpos = None))]
    fn search(
        &self,
        py: Python<'_>,
        string: &Bound<'_, PyAny>,
        pos: usize,
        endpos: Option<usize>,
    ) -> PyResult<Option<Py<PyMatch>>> {
        let text = string;
        if let Some(converted) = as_byte_subject(py, text)? {
            return self.search(py, converted.as_any(), pos, endpos);
        }
        self.check_subject(text)?;
        if pos != 0 || endpos.is_some() {
            if py_bytes(text).is_some() {
                return Ok(self.bytes_search(py, text, pos, endpos, false, false));
            }
            return self.windowed(py, text, pos, endpos, false, false);
        }
        if let Some(raw) = py_bytes(text) {
            return self.bytes_match_with_position(py, raw, false, false);
        }
        let text = text
            .cast::<PyString>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected str or bytes"))?;
        self.match_with_position(py, text, false, false)
    }

    /// Match only at the start of `text`.
    #[pyo3(name = "match")]
    #[pyo3(signature = (string, pos = 0, endpos = None))]
    fn match_(
        &self,
        py: Python<'_>,
        string: &Bound<'_, PyAny>,
        pos: usize,
        endpos: Option<usize>,
    ) -> PyResult<Option<Py<PyMatch>>> {
        let text = string;
        if let Some(converted) = as_byte_subject(py, text)? {
            return self.match_(py, converted.as_any(), pos, endpos);
        }
        self.check_subject(text)?;
        if let Some(raw) = py_bytes(text) {
            if pos != 0 || endpos.is_some() {
                return Ok(self.bytes_search(py, text, pos, endpos, true, false));
            }
            return self.bytes_match_with_position(py, raw, true, false);
        }
        if pos != 0 || endpos.is_some() {
            return self.windowed(py, text, pos, endpos, true, false);
        }
        let text = text
            .cast::<PyString>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected str or bytes"))?;
        self.match_with_position(py, text, true, false)
    }

    /// Match the whole of `text`.
    #[pyo3(signature = (string, pos = 0, endpos = None))]
    fn fullmatch(
        &self,
        py: Python<'_>,
        string: &Bound<'_, PyAny>,
        pos: usize,
        endpos: Option<usize>,
    ) -> PyResult<Option<Py<PyMatch>>> {
        let text = string;
        if let Some(converted) = as_byte_subject(py, text)? {
            return self.search(py, converted.as_any(), pos, endpos);
        }
        self.check_subject(text)?;
        if pos != 0 || endpos.is_some() {
            if py_bytes(text).is_some() {
                return Ok(self.bytes_search(py, text, pos, endpos, true, true));
            }
            return self.windowed(py, text, pos, endpos, true, true);
        }
        if let Some(raw) = py_bytes(text) {
            return self.bytes_match_with_position(py, raw, true, true);
        }
        let text = text
            .cast::<PyString>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected str or bytes"))?;
        self.match_with_position(py, text, true, true)
    }

    /// Reject mixing a bytes pattern with a str subject and the reverse.
    fn check_subject(&self, text: &Bound<'_, PyAny>) -> PyResult<()> {
        if text.cast::<PyBytes>().is_ok() != self.bytes && text.cast::<PyString>().is_ok() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "cannot use a bytes pattern on a string-like object",
            ));
        }
        if text.cast::<PyBytes>().is_ok() && !self.bytes {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "cannot use a string pattern on a bytes-like object",
            ));
        }
        Ok(())
    }

    /// `finditer` restricted to `pos..endpos`, by materialising the window.
    fn finditer_window(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyAny>,
        pos: usize,
        endpos: Option<usize>,
    ) -> PyResult<PyFindIter> {
        let hay = text.cast::<PyString>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("windowed finditer needs a str subject")
        })?;
        let hay = hay.to_cow()?.into_owned();
        let total = hay.chars().count();
        let endpos = endpos.unwrap_or(total).min(total);
        let window: String = hay
            .chars()
            .skip(pos)
            .take(endpos.saturating_sub(pos))
            .collect();
        let slice = PyString::new(py, &window);
        let mut inner = self.finditer(py, slice.as_any(), 0, None)?;
        for slots in inner.matches.iter_mut() {
            shift_slots(slots, pos);
        }
        let text = text.cast::<PyString>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("windowed finditer needs a str subject")
        })?;
        inner.text = text.clone().unbind();
        Ok(inner)
    }

    /// Windowed search for both subject types.
    fn bytes_search(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyAny>,
        pos: usize,
        endpos: Option<usize>,
        anchored_start: bool,
        anchored_end: bool,
    ) -> Option<Py<PyMatch>> {
        let raw = py_bytes(text)?;
        let endpos = endpos.unwrap_or(raw.len()).min(raw.len());
        if pos > endpos {
            return None;
        }
        let window = &raw[pos..endpos];
        let found = self
            .bytes_match_with_position(py, window, anchored_start, anchored_end)
            .ok()??;
        {
            let mut inner = found.borrow_mut(py);
            shift_slots(&mut inner.slots, pos);
            inner.raw = raw.to_vec();
            inner.pos = pos;
            inner.endpos = endpos;
        }
        Some(found)
    }

    /// Run an anchored or unanchored search inside `pos..endpos`.
    ///
    /// The window is materialised as a substring, so `\A` and `^` treat its
    /// start as the beginning of the subject. CPython keeps `\A` anchored to
    /// the real start; that difference is recorded in `semantics.md`.
    fn windowed(
        &self,
        py: Python<'_>,
        text: &Bound<'_, PyAny>,
        pos: usize,
        endpos: Option<usize>,
        anchored_start: bool,
        anchored_end: bool,
    ) -> PyResult<Option<Py<PyMatch>>> {
        let text = text.cast::<PyString>().map_err(|_| {
            pyo3::exceptions::PyTypeError::new_err("windowed search needs a str subject")
        })?;
        let hay = text.to_cow()?.into_owned();
        let total = hay.chars().count();
        let endpos = endpos.unwrap_or(total).min(total);
        if pos > endpos {
            return Ok(None);
        }
        let window: String = hay.chars().skip(pos).take(endpos - pos).collect();
        let slice = PyString::new(py, &window);
        let found = self.match_with_position(py, &slice, anchored_start, anchored_end)?;
        let Some(found) = found else { return Ok(None) };
        {
            let mut inner = found.borrow_mut(py);
            shift_slots(&mut inner.slots, pos);
            inner.text = text.clone().unbind();
            inner.pos = pos;
            inner.endpos = endpos;
        }
        Ok(Some(found))
    }

    /// Bytes counterpart of `match_with_position`.
    fn bytes_match_with_position(
        &self,
        py: Python<'_>,
        raw: &[u8],
        anchored_start: bool,
        anchored_end: bool,
    ) -> PyResult<Option<Py<PyMatch>>> {
        let mut scratch = self.take_scratch();
        let width = 2 * (self.inner.group_count() + 1);
        let mut slots = vec![usize::MAX; width];
        let result = match self.inner.find_bytes_with(&mut scratch, raw) {
            Some(found) => {
                let ok = (!anchored_start || found.start() == 0)
                    && (!anchored_end || found.end() == raw.len());
                if ok
                    && self
                        .inner
                        .captures(&mut scratch, Ucs::Ucs1(raw), found, &mut slots)
                {
                    Some(bytes_match(
                        py,
                        raw,
                        slots.clone(),
                        &self.names(),
                        &self.pattern,
                        self.flags,
                    )?)
                } else {
                    None
                }
            }
            None => None,
        };
        self.put_scratch(scratch);
        Ok(result)
    }

    /// Replace matches, returning the new string.
    #[pyo3(signature = (repl, text, count = 0))]
    fn sub(
        &self,
        py: Python<'_>,
        repl: &Bound<'_, PyAny>,
        text: &Bound<'_, PyAny>,
        count: usize,
    ) -> PyResult<Py<PyAny>> {
        self.check_subject(text)?;
        if let Some(raw) = py_bytes(text) {
            return self
                .sub_bytes(py, raw, repl, count, false)
                .map(|(result, _)| result);
        }
        let text = text
            .cast::<PyString>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected str or bytes"))?;
        self.sub_impl(py, repl, text, count)
            .map(|(result, _)| result.into_any())
    }

    /// Replace matches, returning the new string and the replacement count.
    #[pyo3(signature = (repl, text, count = 0))]
    fn subn(
        &self,
        py: Python<'_>,
        repl: &Bound<'_, PyAny>,
        text: &Bound<'_, PyAny>,
        count: usize,
    ) -> PyResult<(Py<PyAny>, usize)> {
        self.check_subject(text)?;
        if let Some(raw) = py_bytes(text) {
            return self.sub_bytes(py, raw, repl, count, true);
        }
        let text = text
            .cast::<PyString>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected str or bytes"))?;
        self.sub_impl(py, repl, text, count)
            .map(|(result, n)| (result.into_any(), n))
    }

    /// Bytes counterpart of `sub_impl`; returns bytes plus the replacement count.
    fn sub_bytes(
        &self,
        py: Python<'_>,
        raw: &[u8],
        repl: &Bound<'_, PyAny>,
        count: usize,
        _counted: bool,
    ) -> PyResult<(Py<PyAny>, usize)> {
        let all = self.bytes_slots(py, raw)?;
        let names = self.names();
        let limit = if count == 0 {
            all.len()
        } else {
            count.min(all.len())
        };
        let mut out: Vec<u8> = Vec::with_capacity(raw.len());
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
            out.extend_from_slice(&raw[previous..start]);
            let hay: String = raw.iter().map(|&byte| byte as char).collect();
            if repl.is_callable() {
                let m = bytes_match(py, raw, slots.clone(), &names, &self.pattern, self.flags)?;
                let value: Vec<u8> = repl.call1((m,))?.extract()?;
                out.extend_from_slice(&value);
            } else {
                let template: Vec<u8> = repl.extract()?;
                let template: String = template.iter().map(|&byte| byte as char).collect();
                let expanded = expand_replacement(&template, &hay, slots, &names)
                    .map_err(pyo3::exceptions::PyValueError::new_err)?;
                out.extend(expanded.chars().map(|ch| ch as u8));
            }
            previous = end;
            replaced += 1;
        }
        out.extend_from_slice(&raw[previous..]);
        Ok((PyBytes::new(py, &out).into_any().unbind(), replaced))
    }

    /// Split `text` on matches, keeping capture groups.
    #[pyo3(signature = (string, maxsplit = 0))]
    fn split(
        &self,
        py: Python<'_>,
        string: &Bound<'_, PyAny>,
        maxsplit: isize,
    ) -> PyResult<Py<PyList>> {
        let text = string;
        self.check_subject(text)?;
        if let Some(raw) = py_bytes(text) {
            return self.split_bytes(py, raw, maxsplit);
        }
        let text = text
            .cast::<PyString>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected str or bytes"))?;
        self.split_impl(py, text, maxsplit)
    }

    /// Bytes counterpart of `split_impl`.
    fn split_bytes(&self, py: Python<'_>, raw: &[u8], maxsplit: isize) -> PyResult<Py<PyList>> {
        let all = self.bytes_slots(py, raw)?;
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
            list.append(PyBytes::new(py, &raw[previous..start]))?;
            for index in 1..(slots.len() / 2) {
                let (lo, hi) = (slots[2 * index], slots[2 * index + 1]);
                if lo == usize::MAX || hi == usize::MAX {
                    list.append(py.None())?;
                } else {
                    list.append(PyBytes::new(py, &raw[lo..hi]))?;
                }
            }
            previous = end;
        }
        list.append(PyBytes::new(py, &raw[previous..]))?;
        Ok(list.unbind())
    }

    /// All match slots for a bytes subject.
    fn bytes_slots(&self, _py: Python<'_>, raw: &[u8]) -> PyResult<Vec<Vec<usize>>> {
        let mut scratch = self.take_scratch();
        let width = 2 * (self.inner.group_count() + 1);
        let found: Vec<crate::Match> = self.inner.find_iter_bytes_with(&mut scratch, raw).collect();
        let mut slots = vec![usize::MAX; width];
        let mut out = Vec::with_capacity(found.len());
        for m in found {
            slots.fill(usize::MAX);
            if self
                .inner
                .captures(&mut scratch, Ucs::Ucs1(raw), m, &mut slots)
            {
                out.push(slots.clone());
            }
        }
        self.put_scratch(scratch);
        Ok(out)
    }

    /// Iterate over matches, yielding [`PyMatch`] objects.
    #[pyo3(signature = (string, pos = 0, endpos = None))]
    fn finditer(
        &self,
        py: Python<'_>,
        string: &Bound<'_, PyAny>,
        pos: usize,
        endpos: Option<usize>,
    ) -> PyResult<PyFindIter> {
        let text = string;
        self.check_subject(text)?;
        if pos != 0 || endpos.is_some() {
            return self.finditer_window(py, text, pos, endpos);
        }
        if let Some(raw) = py_bytes(text) {
            let mut scratch = self.take_scratch();
            let width = 2 * (self.inner.group_count() + 1);
            let found: Vec<crate::Match> =
                self.inner.find_iter_bytes_with(&mut scratch, raw).collect();
            let mut slots = vec![usize::MAX; width];
            let mut matches = Vec::with_capacity(found.len());
            for m in found {
                slots.fill(usize::MAX);
                if self
                    .inner
                    .captures(&mut scratch, Ucs::Ucs1(raw), m, &mut slots)
                {
                    matches.push(slots.clone());
                }
            }
            self.put_scratch(scratch);
            return Ok(PyFindIter {
                text: PyString::new(py, "").unbind(),
                raw: raw.to_vec(),
                bytes: true,
                matches,
                position: 0,
                names: self.names(),
                pattern_text: self.pattern.clone(),
                flags: self.flags,
                endpos: raw.len(),
            });
        }
        let text = text
            .cast::<PyString>()
            .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected str or bytes"))?;
        let ucs = py_ucs(text)?;
        let mut scratch = self.take_scratch();
        let width = 2 * (self.inner.group_count() + 1);
        let found = collect_spans_and_slots(&self.inner, &mut scratch, ucs, py, width);
        self.put_scratch(scratch);
        let matches = found.into_iter().map(|(_, _, slots)| slots).collect();
        Ok(PyFindIter {
            text: text.clone().unbind(),
            raw: Vec::new(),
            bytes: false,
            matches,
            position: 0,
            pattern_text: self.pattern.clone(),
            flags: self.flags,
            endpos: text.to_cow().map(|c| c.chars().count()).unwrap_or(0),
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
                    let mut last = None;
                    if self.inner.captures_with_last(
                        &mut scratch,
                        ucs,
                        found,
                        &mut slots,
                        &mut last,
                    ) {
                        Some(Py::new(
                            py,
                            PyMatch {
                                text: text.clone().unbind(),
                                raw: Vec::new(),
                                bytes: false,
                                slots,
                                names: self.names(),
                                pattern_text: self.pattern.clone(),
                                flags: self.flags,
                                pos: 0,
                                endpos: ucs.len(),
                                last_index: last,
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
                        raw: Vec::new(),
                        bytes: false,
                        slots: slots.clone(),
                        names: names.clone(),
                        pattern_text: self.pattern.clone(),
                        flags: self.flags,
                        pos: 0,
                        endpos: text.to_cow().map(|c| c.chars().count()).unwrap_or(0),
                        last_index: None,
                    },
                )?;
                let out = repl.call1((m,))?;
                out.extract::<String>()?
            } else {
                let template = repl.extract::<String>()?;
                expand_replacement(&template, &hay, slots, &names)
                    .map_err(pyo3::exceptions::PyValueError::new_err)?
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
        "unirex.Pattern".to_string()
    }
}

fn collect_spans(
    regex: &CorePattern,
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
    regex: &CorePattern,
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
/// Borrow a byte subject, or report that it is not bytes.
fn py_bytes<'a>(text: &'a Bound<'_, PyAny>) -> Option<&'a [u8]> {
    text.cast::<PyBytes>().ok().map(|raw| raw.as_bytes())
}

/// Build a `PyMatch` for a bytes subject.
fn bytes_match(
    py: Python<'_>,
    raw: &[u8],
    slots: Vec<usize>,
    names: &[(String, usize)],
    pattern_text: &str,
    flags: u32,
) -> PyResult<Py<PyMatch>> {
    Py::new(
        py,
        PyMatch {
            text: PyString::new(py, "").unbind(),
            raw: raw.to_vec(),
            bytes: true,
            slots,
            names: names.to_vec(),
            pattern_text: pattern_text.to_string(),
            flags,
            pos: 0,
            endpos: raw.len(),
            last_index: None,
        },
    )
}

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

/// Flag values matching CPython's `re` module.
const FLAG_IGNORECASE: u32 = 2;
const FLAG_LOCALE: u32 = 4;
const FLAG_MULTILINE: u32 = 8;
const FLAG_DOTALL: u32 = 16;
const FLAG_UNICODE: u32 = 32;
const FLAG_VERBOSE: u32 = 64;
const FLAG_DEBUG: u32 = 128;
const FLAG_ASCII: u32 = 256;

/// Translate `re` flags into an inline flag prefix.
///
/// Unicode semantics are already the default, so `UNICODE` is a no-op. Flags
/// this engine cannot honour are rejected rather than ignored.
fn flags_prefix(flags: u32) -> PyResult<String> {
    if flags & FLAG_LOCALE != 0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "LOCALE flag is not supported",
        ));
    }
    if flags & FLAG_DEBUG != 0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "DEBUG flag is not supported",
        ));
    }
    if flags & FLAG_ASCII != 0 {
        return Err(pyo3::exceptions::PyValueError::new_err(
            "ASCII flag is not supported: classes follow CPython's Unicode tables",
        ));
    }
    let mut prefix = String::new();
    if flags & FLAG_IGNORECASE != 0 {
        prefix.push('i');
    }
    if flags & FLAG_MULTILINE != 0 {
        prefix.push('m');
    }
    if flags & FLAG_DOTALL != 0 {
        prefix.push('s');
    }
    if flags & FLAG_VERBOSE != 0 {
        prefix.push('x');
    }
    if prefix.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!("(?{prefix})"))
    }
}

#[pyfunction]
#[pyo3(signature = (pattern, flags = 0))]
fn compile(pattern: &Bound<'_, PyAny>, flags: u32) -> PyResult<Py<PyPattern>> {
    // A compiled pattern is returned unchanged, so `re.compile(p) is p`, and
    // passing flags with it is an error rather than a silent ignore.
    if let Ok(existing) = pattern.cast::<PyPattern>() {
        if flags != 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "cannot process flags argument with a compiled pattern",
            ));
        }
        return Ok(existing.clone().unbind());
    }
    // Cache compiled patterns, as re does: `re.compile(p) is re.compile(p)`.
    let py = pattern.py();
    let cache = py.import("unirex")?.getattr("_cache")?;
    let cache = cache.cast::<PyDict>()?;
    // Dict lookup compares str/bytes by value, so equal patterns share an
    // entry and the cache cannot confuse a str pattern with a bytes one.
    let composite = (pattern, flags);
    if let Some(hit) = cache.get_item(composite)? {
        if let Ok(found) = hit.cast::<PyPattern>() {
            return Ok(found.clone().unbind());
        }
    }
    if let Ok(text) = pattern.cast::<PyString>() {
        let built = PyPattern::with_flags(&text.to_cow()?, flags)?;
        let handle = Py::new(py, built)?;
        if cache.len() > 512 {
            cache.clear();
        }
        cache.set_item(composite, handle.clone_ref(py))?;
        return Ok(handle);
    }
    if let Ok(raw) = pattern.cast::<PyBytes>() {
        // Bytes patterns are decoded as latin-1 so byte values survive the
        // parser, then compiled with ASCII class semantics.
        let decoded: String = raw.as_bytes().iter().map(|&byte| byte as char).collect();
        let built = PyPattern::build(&decoded, flags, true)?;
        let handle = Py::new(py, built)?;
        if cache.len() > 512 {
            cache.clear();
        }
        cache.set_item(composite, handle.clone_ref(py))?;
        return Ok(handle);
    }
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "first argument must be a string or compiled pattern, not '{}'",
        pattern
            .get_type()
            .name()
            .map(|n| n.to_string())
            .unwrap_or_default()
    )))
}

fn compile_with(flags: u32, pattern: &Bound<'_, PyAny>) -> PyResult<Py<PyPattern>> {
    compile(pattern, flags)
}

macro_rules! module_fn {
    ($name:ident, $method:ident, $pyname:literal) => {
        #[pyfunction]
        #[pyo3(name = $pyname, signature = (pattern, text, flags = 0))]
        fn $name(
            py: Python<'_>,
            pattern: &Bound<'_, PyAny>,
            text: &Bound<'_, PyAny>,
            flags: u32,
        ) -> PyResult<Option<Py<PyMatch>>> {
            compile_with(flags, pattern)?
                .borrow(py)
                .$method(py, text, 0, None)
        }
    };
}

macro_rules! module_iter_fn {
    ($name:ident, $pyname:literal) => {
        #[pyfunction]
        #[pyo3(name = $pyname, signature = (pattern, text, flags = 0))]
        fn $name(
            py: Python<'_>,
            pattern: &Bound<'_, PyAny>,
            text: &Bound<'_, PyAny>,
            flags: u32,
        ) -> PyResult<PyFindIter> {
            compile_with(flags, pattern)?
                .borrow(py)
                .finditer(py, text, 0, None)
        }
    };
}

module_fn!(search, search, "search");
module_fn!(match_fn, match_, "match");
module_fn!(fullmatch, fullmatch, "fullmatch");
// Assigned below through the generated wrapper; the module signature keeps the
// `flags` keyword, so the call passes explicit window defaults.
module_iter_fn!(finditer, "finditer");

#[pyfunction]
#[pyo3(signature = (pattern, text, flags = 0))]
fn findall(
    py: Python<'_>,
    pattern: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
    flags: u32,
) -> PyResult<Py<PyList>> {
    compile_with(flags, pattern)?
        .borrow(py)
        .findall(py, text, 0, None)
}

#[pyfunction]
#[pyo3(signature = (pattern, repl, text, count = 0, flags = 0))]
fn sub(
    py: Python<'_>,
    pattern: &Bound<'_, PyAny>,
    repl: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
    count: usize,
    flags: u32,
) -> PyResult<Py<PyAny>> {
    compile_with(flags, pattern)?
        .borrow(py)
        .sub(py, repl, text, count)
}

#[pyfunction]
#[pyo3(signature = (pattern, repl, text, count = 0, flags = 0))]
fn subn(
    py: Python<'_>,
    pattern: &Bound<'_, PyAny>,
    repl: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
    count: usize,
    flags: u32,
) -> PyResult<(Py<PyAny>, usize)> {
    compile_with(flags, pattern)?
        .borrow(py)
        .subn(py, repl, text, count)
}

#[pyfunction]
#[pyo3(signature = (pattern, text, maxsplit = 0, flags = 0))]
fn split(
    py: Python<'_>,
    pattern: &Bound<'_, PyAny>,
    text: &Bound<'_, PyAny>,
    maxsplit: isize,
    flags: u32,
) -> PyResult<Py<PyList>> {
    compile_with(flags, pattern)?
        .borrow(py)
        .split(py, text, maxsplit)
}

/// A `re.Scanner`-style cursor over a subject.
#[pyclass(name = "Scanner", module = "unirex")]
pub struct PyScanner {
    text: Py<PyString>,
    pattern_text: String,
    flags: u32,
    position: usize,
    endpos: usize,
}

#[pymethods]
impl PyScanner {
    /// Anchored attempt at the current cursor, advancing on success.
    #[pyo3(name = "match")]
    fn match_(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyMatch>>> {
        let hay = self.text.bind(py).to_cow()?.into_owned();
        let tail: String = hay
            .chars()
            .skip(self.position)
            .take(self.endpos.saturating_sub(self.position))
            .collect();
        let pattern = PyPattern::with_flags(&self.pattern_text, self.flags)?;
        let Some(found) = pattern.match_(py, &PyString::new(py, &tail).into_any(), 0, None)? else {
            return Ok(None);
        };
        let bound = found.bind(py);
        let (start, end) = bound.borrow().span_of(0).unwrap_or((0, 0));
        self.position += end;
        let _ = start;
        Ok(Some(found))
    }

    /// Unanchored attempt from the current cursor, advancing on success.
    fn search(&mut self, py: Python<'_>) -> PyResult<Option<Py<PyMatch>>> {
        let hay = self.text.bind(py).to_cow()?.into_owned();
        let tail: String = hay
            .chars()
            .skip(self.position)
            .take(self.endpos.saturating_sub(self.position))
            .collect();
        let pattern = PyPattern::with_flags(&self.pattern_text, self.flags)?;
        let Some(found) = pattern.search(py, &PyString::new(py, &tail).into_any(), 0, None)? else {
            return Ok(None);
        };
        let bound = found.bind(py);
        let (_, end) = bound.borrow().span_of(0).unwrap_or((0, 0));
        self.position += end;
        Ok(Some(found))
    }
}

/// Escape a string the way CPython's `re.escape` does.
#[pyfunction]
fn escape<'py>(py: Python<'py>, text: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    if let Ok(raw) = text.cast::<PyBytes>() {
        let mut escaped = Vec::with_capacity(raw.as_bytes().len() * 2);
        for &byte in raw.as_bytes() {
            if is_special_escape(byte as char) {
                escaped.push(b'\\');
            }
            escaped.push(byte);
        }
        return Ok(PyBytes::new(py, &escaped).into_any().unbind());
    }
    let text = text
        .cast::<PyString>()
        .map_err(|_| pyo3::exceptions::PyTypeError::new_err("expected str or bytes"))?;
    Ok(PyString::new(py, &escape_str(&text.to_cow()?))
        .into_any()
        .unbind())
}

/// Characters `re.escape` prefixes with a backslash.
fn is_special_escape(ch: char) -> bool {
    matches!(
        ch,
        '(' | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '?'
            | '*'
            | '+'
            | '-'
            | '|'
            | '^'
            | '$'
            | '\\'
            | '.'
            | '&'
            | '~'
            | '#'
            | ' '
            | '\t'
            | '\n'
            | '\r'
            | '\x0b'
            | '\x0c'
    )
}

fn escape_str(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let special = matches!(
            ch,
            '(' | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '?'
                | '*'
                | '+'
                | '-'
                | '|'
                | '^'
                | '$'
                | '\\'
                | '.'
                | '&'
                | '~'
                | '#'
                | ' '
                | '\t'
                | '\n'
                | '\r'
                | '\x0b'
                | '\x0c'
        );
        if special {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// Clear the compiled-pattern cache, as `re.purge` does.
#[pyfunction]
fn purge(py: Python<'_>) -> PyResult<()> {
    let cache = py.import("unirex")?.getattr("_cache")?;
    cache.cast::<PyDict>()?.clear();
    Ok(())
}

/// A single match with its capture spans.
#[pyclass(name = "Match", module = "unirex", weakref)]
pub struct PyMatch {
    text: Py<PyString>,
    /// Raw subject when the match came from bytes.
    raw: Vec<u8>,
    bytes: bool,
    slots: Vec<usize>,
    names: Vec<(String, usize)>,
    pattern_text: String,
    flags: u32,
    pos: usize,
    endpos: usize,
    last_index: Option<usize>,
}

/// As [`resolve_index`], rejecting indices beyond `groups`.
fn resolve_index_with(
    names: &[(String, usize)],
    index: Option<&Bound<'_, PyAny>>,
    groups: usize,
) -> PyResult<usize> {
    let Some(index) = index else {
        return Ok(0);
    };
    if let Ok(number) = index.extract::<isize>() {
        if number < 0 {
            return Err(pyo3::exceptions::PyIndexError::new_err(
                "no such group: negative index",
            ));
        }
        let number = number as usize;
        if number > groups {
            return Err(pyo3::exceptions::PyIndexError::new_err("no such group"));
        }
        return Ok(number);
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
    if index.is_instance_of::<pyo3::types::PyInt>() {
        // Values beyond isize raise IndexError in re, not TypeError.
        return Err(pyo3::exceptions::PyIndexError::new_err("no such group"));
    }
    Err(pyo3::exceptions::PyIndexError::new_err("no such group"))
}

impl PyMatch {
    /// One group as `str` or `bytes`, depending on the subject type.
    fn value<'py>(&self, py: Python<'py>, index: usize) -> PyResult<Option<Py<PyAny>>> {
        let Some((lo, hi)) = self.span_of(index) else {
            return Ok(None);
        };
        if self.bytes {
            return Ok(Some(
                PyBytes::new(py, &self.raw[lo..hi]).into_any().unbind(),
            ));
        }
        let text = self.text.bind(py);
        substring(py, text, lo, hi).map(|s| Some(s.unbind().into_any()))
    }

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
        let index = resolve_index_with(&self.names, index, self.slots.len() / 2 - 1)?;
        // Python reports `(-1, -1)` for a group that did not participate.
        Ok(match self.span_of(index) {
            Some((start, end)) => (start as isize, end as isize),
            None => (-1, -1),
        })
    }

    #[pyo3(signature = (index = None))]
    fn start(&self, index: Option<&Bound<'_, PyAny>>) -> PyResult<isize> {
        let index = resolve_index_with(&self.names, index, self.slots.len() / 2 - 1)?;
        Ok(self.span_of(index).map_or(-1, |(start, _)| start as isize))
    }

    #[pyo3(signature = (index = None))]
    fn end(&self, index: Option<&Bound<'_, PyAny>>) -> PyResult<isize> {
        let index = resolve_index_with(&self.names, index, self.slots.len() / 2 - 1)?;
        Ok(self.span_of(index).map_or(-1, |(_, end)| end as isize))
    }

    #[pyo3(signature = (*indices))]
    fn group(&self, py: Python<'_>, indices: &Bound<'_, PyTuple>) -> PyResult<Py<PyAny>> {
        if indices.is_empty() {
            return Ok(match self.value(py, 0)? {
                Some(value) => value,
                None => py.None(),
            });
        }
        if indices.len() == 1 {
            let index = resolve_index_with(
                &self.names,
                Some(indices.get_item(0)?.as_any()),
                self.slots.len() / 2 - 1,
            )?;
            return Ok(match self.value(py, index)? {
                Some(value) => value,
                None => py.None(),
            });
        }
        let mut values: Vec<Py<PyAny>> = Vec::with_capacity(indices.len());
        for position in 0..indices.len() {
            let item = indices.get_item(position)?;
            let index =
                resolve_index_with(&self.names, Some(item.as_any()), self.slots.len() / 2 - 1)?;
            values.push(match self.value(py, index)? {
                Some(value) => value,
                None => py.None(),
            });
        }
        Ok(PyTuple::new(py, values)?.into_any().unbind())
    }

    #[pyo3(signature = (default = None))]
    fn groups<'py>(
        &self,
        py: Python<'py>,
        default: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyTuple>> {
        let count = self.slots.len() / 2;
        let mut items: Vec<Option<Py<PyAny>>> = Vec::with_capacity(count.saturating_sub(1));
        for index in 1..count {
            let value = self.value(py, index)?;
            items.push(match (value, default) {
                (None, Some(fallback)) => Some(fallback.clone().unbind()),
                (value, _) => value,
            });
        }
        Ok(PyTuple::new(py, items)?.unbind())
    }

    fn groupdict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyDict>> {
        let dict = PyDict::new(py);
        for (name, index) in &self.names {
            dict.set_item(name, self.value(py, *index)?)?;
        }
        Ok(dict)
    }

    /// The compiled pattern that produced this match.
    #[getter]
    fn re(&self, py: Python<'_>) -> PyResult<Py<PyPattern>> {
        Py::new(py, PyPattern::with_flags(&self.pattern_text, self.flags)?)
    }

    /// The subject string.
    #[getter]
    fn string(&self, py: Python<'_>) -> Py<PyString> {
        self.text.clone_ref(py)
    }

    /// Start of the search window.
    #[getter]
    fn pos(&self) -> usize {
        self.pos
    }

    /// End of the search window.
    #[getter]
    fn endpos(&self) -> usize {
        self.endpos
    }

    /// Index of the last group that matched, or `None`.
    #[getter]
    fn lastindex(&self) -> Option<usize> {
        self.last_index
            .filter(|index| self.span_of(*index).is_some())
    }

    /// Name of the last group that matched, when it has one.
    #[getter]
    fn lastgroup(&self) -> Option<String> {
        let index = self.lastindex()?;
        self.names
            .iter()
            .find(|(_, group)| *group == index)
            .map(|(name, _)| name.clone())
    }

    /// `m[index]` is `m.group(index)`, as in `re`.
    fn __getitem__(&self, py: Python<'_>, index: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let single = PyTuple::new(py, [index])?;
        self.group(py, &single)
    }

    /// Expand `\1` and `\g<name>` references against this match.
    fn expand<'py>(&self, py: Python<'py>, template: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        if self.bytes {
            let template: &[u8] = template.extract()?;
            let text: String = template.iter().map(|&byte| byte as char).collect();
            let hay: String = self.raw.iter().map(|&byte| byte as char).collect();
            let expanded = expand_replacement(&text, &hay, &self.slots, &self.names)
                .map_err(pyo3::exceptions::PyValueError::new_err)?;
            let raw: Vec<u8> = expanded.chars().map(|ch| ch as u8).collect();
            return Ok(PyBytes::new(py, &raw).into_any().unbind());
        }
        let hay = self.text.bind(py).to_cow()?.into_owned();
        let template = template.extract::<String>()?;
        let expanded = expand_replacement(&template, &hay, &self.slots, &self.names)
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        Ok(PyString::new(py, &expanded).into_any().unbind())
    }

    fn __repr__(&self) -> String {
        match self.span_of(0) {
            Some((start, end)) => format!("<unirex.Match span=({start}, {end})>"),
            None => "<unirex.Match unmatched>".to_string(),
        }
    }
}

/// Iterator over matches produced by [`PyPattern::finditer`].
///
/// Spans and capture slots are collected up front, so iteration itself is
/// cheap and does not hold the GIL beyond building each `Match`.
#[pyclass(name = "FindIter", module = "unirex")]
pub struct PyFindIter {
    text: Py<PyString>,
    raw: Vec<u8>,
    bytes: bool,
    matches: Vec<Vec<usize>>,
    position: usize,
    names: Vec<(String, usize)>,
    pattern_text: String,
    flags: u32,
    endpos: usize,
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
                    raw: slf.raw.clone(),
                    bytes: slf.bytes,
                    slots,
                    names: slf.names.clone(),
                    pattern_text: slf.pattern_text.clone(),
                    flags: slf.flags,
                    pos: 0,
                    endpos: slf.endpos,
                    last_index: None,
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
    let bare = err.message();
    let offset = match err {
        crate::Error::Parse(inner) => Some(inner.span().start.offset),
        other => other.offset(),
    };
    let ty = py.get_type::<Error>();
    let instance = match ty.call1((message.clone(),)) {
        Ok(value) => value,
        Err(err) => return err,
    };
    let _ = instance.setattr("msg", bare);
    let _ = instance.setattr("pattern", pattern);
    let _ = instance.setattr("pos", offset);
    let _ = instance.setattr("lineno", 1);
    let _ = instance.setattr("colno", offset.map(|pos| pos + 1));
    PyErr::from_value(instance)
}

#[pymodule]
fn unirex(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("_cache", PyDict::new(m.py()))?;
    m.add_class::<PyPattern>()?;
    m.add_class::<PyMatch>()?;
    m.add_class::<PyFindIter>()?;
    m.add_class::<PyScanner>()?;
    m.add("error", m.py().get_type::<Error>())?;
    m.add("PatternError", m.py().get_type::<Error>())?;
    m.add_function(wrap_pyfunction!(compile, m)?)?;
    m.add_function(wrap_pyfunction!(search, m)?)?;
    m.add_function(wrap_pyfunction!(match_fn, m)?)?;
    m.add_function(wrap_pyfunction!(fullmatch, m)?)?;
    m.add_function(wrap_pyfunction!(findall, m)?)?;
    m.add_function(wrap_pyfunction!(finditer, m)?)?;
    m.add_function(wrap_pyfunction!(sub, m)?)?;
    m.add_function(wrap_pyfunction!(subn, m)?)?;
    m.add_function(wrap_pyfunction!(split, m)?)?;
    m.add_function(wrap_pyfunction!(escape, m)?)?;
    m.add_function(wrap_pyfunction!(purge, m)?)?;
    // `re` exposes an IntFlag, not bare ints; build it with the stdlib so
    // comparisons and `re.RegexFlag(x)` behave the same.
    let globals = m.dict();
    let code = std::ffi::CString::new(
        "import enum as _enum\n\
         RegexFlag = _enum.IntFlag('RegexFlag', {\n\
             'NOFLAG': 0, 'TEMPLATE': 1, 'IGNORECASE': 2, 'I': 2, 'LOCALE': 4, 'L': 4,\n\
             'MULTILINE': 8, 'M': 8, 'DOTALL': 16, 'S': 16, 'UNICODE': 32, 'U': 32,\n\
             'VERBOSE': 64, 'X': 64, 'DEBUG': 128, 'ASCII': 256, 'A': 256,\n\
         })\n\
         NOFLAG, IGNORECASE, I = RegexFlag.NOFLAG, RegexFlag.IGNORECASE, RegexFlag.I\n\
         LOCALE, L = RegexFlag.LOCALE, RegexFlag.L\n\
         MULTILINE, M = RegexFlag.MULTILINE, RegexFlag.M\n\
         DOTALL, S = RegexFlag.DOTALL, RegexFlag.S\n\
         UNICODE, U = RegexFlag.UNICODE, RegexFlag.U\n\
         VERBOSE, X = RegexFlag.VERBOSE, RegexFlag.X\n\
         DEBUG, ASCII, A = RegexFlag.DEBUG, RegexFlag.ASCII, RegexFlag.A\n\
         RegexFlag.__repr__ = lambda self: (f're.{self.name}' if self.name else f're.RegexFlag({int(self)})')\n\
         RegexFlag.__str__ = RegexFlag.__repr__\n\
         del _enum\n",
    )
    .expect("flag snippet");
    unsafe {
        pyo3::ffi::PyRun_StringFlags(
            code.as_ptr(),
            pyo3::ffi::Py_file_input,
            globals.as_ptr(),
            globals.as_ptr(),
            std::ptr::null_mut(),
        );
    }
    Ok(())
}

/// Shift every capture slot by `offset`, leaving unmatched slots alone.
fn shift_slots(slots: &mut [usize], offset: usize) {
    for value in slots.iter_mut() {
        if *value != usize::MAX {
            *value += offset;
        }
    }
}

/// Accept bytearray, memoryview, and other buffer objects as byte subjects.
///
/// CPython's `re` treats anything supporting the buffer protocol as bytes; this
/// copies it once, which is what a `Match` would otherwise have to pin.
fn as_byte_subject<'py>(
    py: Python<'py>,
    text: &Bound<'_, PyAny>,
) -> PyResult<Option<Bound<'py, PyBytes>>> {
    if text.cast::<PyString>().is_ok() || text.cast::<PyBytes>().is_ok() {
        return Ok(None);
    }
    match text.extract::<Vec<u8>>() {
        Ok(buffer) => Ok(Some(PyBytes::new(py, &buffer))),
        Err(_) => Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "expected string or bytes-like object, got '{}'",
            text.get_type()
                .name()
                .map(|n| n.to_string())
                .unwrap_or_default()
        ))),
    }
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
) -> Result<String, String> {
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
                match index {
                    Some(index) => {
                        if let (Some(&lo), Some(&hi)) =
                            (slots.get(2 * index), slots.get(2 * index + 1))
                        {
                            if lo != usize::MAX && hi != usize::MAX {
                                out.push_str(&char_slice(hay, lo, hi));
                            }
                        } else {
                            return Err(format!("invalid group reference {index}"));
                        }
                    }
                    None => return Err("invalid group reference".to_string()),
                }
            }
            Some('a') => {
                chars.next();
                out.push('\x07');
            }
            Some('b') => {
                // In a replacement template `\b` is a backspace.
                chars.next();
                out.push('\x08');
            }
            Some('f') => {
                chars.next();
                out.push('\x0c');
            }
            Some('n') => {
                chars.next();
                out.push('\n');
            }
            Some('r') => {
                chars.next();
                out.push('\r');
            }
            Some('t') => {
                chars.next();
                out.push('\t');
            }
            Some('v') => {
                chars.next();
                out.push('\x0b');
            }
            Some('0') => {
                let mut value = 0u32;
                let mut digits = 0;
                while digits < 3 {
                    match chars.peek().copied() {
                        Some(digit) if digit.is_digit(8) => {
                            value = value * 8 + digit.to_digit(8).unwrap_or(0);
                            chars.next();
                            digits += 1;
                        }
                        _ => break,
                    }
                }
                if let Some(ch) = char::from_u32(value) {
                    out.push(ch);
                }
            }
            Some(digit) if digit.is_ascii_digit() => {
                // CPython: a leading `0` is always octal (up to three digits);
                // otherwise one or two digits are a group reference and three
                // octal digits are an octal escape.
                let mut digits = String::new();
                digits.push(digit);
                chars.next();
                while digits.len() < 3 {
                    match chars.peek().copied() {
                        Some(next) if next.is_digit(8) => {
                            digits.push(next);
                            chars.next();
                        }
                        _ => break,
                    }
                }
                let octal = digit == '0' || digits.len() == 3;
                if octal {
                    let value = u32::from_str_radix(&digits, 8).unwrap_or(0);
                    if let Some(ch) = char::from_u32(value) {
                        out.push(ch);
                    }
                } else if let Ok(index) = digits.parse::<usize>() {
                    if let (Some(&lo), Some(&hi)) = (slots.get(2 * index), slots.get(2 * index + 1))
                    {
                        if lo != usize::MAX && hi != usize::MAX {
                            out.push_str(&char_slice(hay, lo, hi));
                        }
                    } else {
                        return Err(format!("invalid group reference {index}"));
                    }
                }
            }
            Some(other) => {
                chars.next();
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    Ok(out)
}
