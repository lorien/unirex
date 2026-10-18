# CPython UCS Contract

Date: 2026-10-18
Status: accepted

## Context

The engine's eventual value depends on reading CPython `str` storage directly
and returning results without an encoding step. CPython's object layout is not
uniform: the `PyObject` header differs between GIL and free-threaded builds,
`PyCompactUnicodeObject` carried `wstr_length` before 3.12, and the `state`
bit shifts on free-threaded builds. A wrong offset can read a refcount as
data. UCS-2 storage can also contain lone surrogates, which the current
UTF-8-based `rurex` path rejects with `UnicodeEncodeError`.

## Decision

- Treat each UCS-2 unit as a code point, including lone surrogates. Do not pair
  surrogates: CPython stores real supplementary characters as UCS-4, so a
  valid surrogate pair cannot occur in compact UCS-2 storage.
- UCS-4 values above U+10FFFF are outside the contract. The core does not
  validate them on the hot path; classification must not index out of bounds
  for them, and the binding never passes them.
- Detect the PEP 393 layout of the running interpreter and validate it against
  CPython's own behavior. If detection or validation fails, fall back to
  CPython's UTF-8 API or the existing `rurex` path.
- Never read an unvalidated field or assume a fixed offset.
- Build result strings from the original UCS buffer, using
  `PyUnicode_Substring` where the ABI target provides it and
  `PyUnicode_FromKindAndData` as the documented fallback.
- Match under the GIL for short strings. Release the GIL around large scans
  while keeping the `str` object alive, and never touch Python objects inside
  the detached region.
- Support CPython 3.10 through 3.15, both GIL and free-threaded 3.14t.

## Alternatives rejected

- Hard-coding PEP 393 offsets: silently wrong on free-threaded builds and on
  interpreters before 3.12.
- Calling `PyUnicode_AsUTF8AndSize` on the hot path: that is the transcode cost
  the project exists to remove.
- Rejecting lone surrogates as the current `rurex` path does: `re` matches
  them, and a UCS-native engine has no encoding step that could fail.
- Pairing surrogates in UCS-2: wrong for compact storage, where each unit is
  an independent code point.
- Copying every haystack to a canonical `u32` buffer: removes the layout
  problem by paying a copy on every scan.
- Reading the UCS buffer without validation: a layout change would cause
  memory unsafety instead of a slow fallback.
