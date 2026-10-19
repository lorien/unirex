# CPython conformance suite

Vendored per supported release under `3.10/` through `3.15/`, each copied from
its own CPython tag (`Lib/test/test_re.py` and `Lib/test/re_tests.py`):

- `<version>/vendored_test_re.py` — `Lib/test/test_re.py` from that tag.
- `<version>/vendored_re_tests.py` — `Lib/test/re_tests.py` from that tag.
- `<version>/EXPECTED.py` — that version's ledger.

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
