# CPython conformance suite

Vendored from CPython v3.13.5:

- `vendored_test_re.py` — `Lib/test/test_re.py`, verbatim except for its
  provenance header.
- `vendored_re_tests.py` — `Lib/test/re_tests.py`, the external data suite.

Both are PSF License Version 2; see https://docs.python.org/3/license.html. The
only modification is the provenance header on each file.

`support_stub.py` and `conftest.py` are ours: a minimal `test.support` stand-in,
a `re` shim that points the suite at the `unirex` extension, and the `EXPECTED`
ledger. Every test that does not pass is listed there with a bucket and a
reason, so the suite is a compatibility ledger rather than a claim that
everything works. Closing a gap means deleting its entry.

Run it with the built extension importable:

    python3 -m pytest tests/cpython/vendored_test_re.py -q

The shim replaces `sys.modules["re"]` for the whole process, so this file must
run in its own pytest invocation; `tests/re_guard.py` exists to catch the case
where it does not.
