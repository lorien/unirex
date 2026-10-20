r"""Conformance harness: run CPython's vendored `test_re.py` against
unirex by shimming `re` -> unirex and `test.support` -> support_stub.

Every test that does not pass must be recorded in EXPECTED below with
a reason, so the suite stays an honest compatibility ledger: what runs
green is compatible, what does not is accounted for. Buckets:

  by-design   — unirex intentionally rejects/behaves differently
                (no backrefs/lookaround/atomic/possessive, `\Z`,
                octal escapes, Pattern(...) constructible; see
                spec/docs/api.md "Semantic differences" and ADRs).
  internals   — tests of CPython implementation internals (re._parser,
                _compiler, _cache, pickle reconstruction via re._compile).
  harness     — artifacts of running the file outside CPython's own
                test runner (subprocess/locale specifics).

Update EXPECTED when unirex closes a gap (remove the entry) or a new
failure appears (add the entry with its bucket).
"""

import sys
import types
from pathlib import Path

import pytest

import unirex

# ------------------------------------------------------------------ shim

RE_PUBLIC = {k: v for k, v in vars(unirex).items() if not k.startswith("_")}


def _install_shims():
    re_mod = types.ModuleType("re")
    re_mod.__dict__.update(RE_PUBLIC)
    re_mod.__dict__.update(
        {
            "Pattern": unirex.Pattern,
            "Match": unirex.Match,
            "Scanner": None,
            "_compile": unirex.compile,  # pickle reconstruction helper
            "__name__": "re",
        }
    )

    test_pkg = types.ModuleType("test")
    test_pkg.__path__ = []

    import support_stub

    support_mod = types.ModuleType("test.support")
    support_mod.__dict__.update(vars(support_stub))

    sys.modules["re"] = re_mod
    sys.modules["test"] = test_pkg
    sys.modules["test.support"] = support_mod
    test_pkg.support = support_mod

    # test.re_tests: the external data suite, vendored from the same
    # tag (contains `tests`, `benchmarks`, FAIL, SYNTAX_ERROR).
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "test.re_tests",
        VERSION_DIR / "vendored_re_tests.py",
    )
    re_tests = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(re_tests)
    sys.modules["test.re_tests"] = re_tests
    test_pkg.re_tests = re_tests


# Each supported CPython release has its own vendored copy of the suite and its
# own ledger, because test names and behaviour differ between releases.
VERSION_DIR = Path(__file__).with_name(f"{sys.version_info[0]}.{sys.version_info[1]}")
LEDGER_PATH = VERSION_DIR / "EXPECTED.py"


def pytest_pycollect_makemodule(module_path, parent):
    # Install the shims only when the vendored suite is really being
    # collected (this hook runs after the test-file name filter), so a
    # plain `pytest` run never sees a patched `re`.
    if module_path.name == "vendored_test_re.py":
        _install_shims()
    return None



# ---------------------------------------------------------------- ledger

import os as _os

if LEDGER_PATH.exists() and not _os.environ.get("UNIREX_LEDGER_IGNORE"):
    _ledger = {}
    exec(compile(LEDGER_PATH.read_text(), str(LEDGER_PATH), "exec"), _ledger)
    EXPECTED = _ledger["EXPECTED"]
else:
    EXPECTED = {}

def pytest_collection_modifyitems(config, items):
    for item in items:
        cls = getattr(item, "cls", None)
        key = f"{cls.__name__}.{item.name}" if cls else item.name
        if key in EXPECTED:
            # skip (not xfail): unittest subTest failures inside a
            # method are reported independently and would bypass the
            # marker; the ledger is kept honest by review — closing a
            # gap means removing the entry so the test runs again.
            item.add_marker(pytest.mark.skip(reason=EXPECTED[key]))
