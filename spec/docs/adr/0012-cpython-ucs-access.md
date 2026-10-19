# UCS Access Through the CPython C API

Date: 2026-10-18
Status: accepted (revised 2026-10-19: the binding is built and smoke-tested on
CPython 3.10 through 3.15 and on the free-threaded 3.14t and 3.15t builds; the
UCS accessors are read under the same rules in both modes because `str` is
immutable) (scratch pooling revised by ADR-0016)

## Context

The Python binding must obtain UCS-1, UCS-2, or UCS-4 slices from a live
`str` without converting to UTF-8. ADR-0007 originally proposed detecting and
validating the private PEP 393 field offsets, following the `rurex` layout
detector. CPython exposes the same information through supported C API
accessors, and using them avoids private offsets entirely.

## Decision

- Obtain the storage kind, data pointer, and length with
  `PyUnicode_KIND`, `PyUnicode_DATA`, and `PyUnicode_GET_LENGTH`.
- Call `PyUnicode_READY` first for Python versions before 3.12; on 3.12 and
  later it is a no-op.
- Construct borrowed `&[u8]`, `&[u16]`, or `&[u32]` slices with a documented
  safety argument: the `str` is alive through a `Bound<PyString>`, strings are
  immutable, and the slices do not outlive that borrow.
- Build result strings with `PyUnicode_Substring`, using the match's
  code-point offsets directly. This avoids UTF-8 encoding and extra copies.
- Keep one scratch pool per compiled `Regex`. A call takes a scratch, scans,
  returns it, and never locks during the scan.
- Match under the GIL for short strings. For haystacks of at least 4,096 code
  units, release the GIL around the scan, then build result strings under the
  GIL. No Python object is touched inside the detached region.
- Build a non-abi3 extension for the current interpreter. A limited-API
  fallback would need a different result-construction path.

## Alternatives rejected

- A private PEP 393 offset detector: more code and more risk for no gain now
  that the supported accessors exist. It remains the fallback if a future
  CPython removes the accessors.
- `PyUnicode_AsUCS4Copy`: allocates and widens every haystack, which is the
  cost the project exists to avoid.
- `PyUnicode_AsUTF8AndSize`: that is exactly the transcode the UCS path
  avoids.
- Always release the GIL: the handoff cost is larger than the scan for short
  Telegram posts, and CPython `re` does not do it.
- abi3-only build: the limited API does not expose the UCS data accessors and
  would force a copy or a fallback path.

## Consequences

- The binding is tied to CPython's non-limited C API. Version-specific wheels
  are required; no single abi3 wheel.
- Correctness of the slices depends on the safety invariant. It must stay
  documented and tested with UCS-1, UCS-2, UCS-4, and lone surrogates.
- Group 0 is the only exposed result. Capture groups currently return the whole
  match, not `re.findall`'s group tuples.
- Free-threaded builds and CPython 3.10 through 3.15 still need explicit
  verification.
