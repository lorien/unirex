"""Tests for the stdlib-``re`` tripwire (``tests/re_guard.py``)."""

import re
import types
from importlib.machinery import ModuleSpec

import pytest

import re_guard


def test_real_stdlib_re_is_accepted():
    assert re_guard.require_stdlib_re(re) is None


def test_default_reads_the_registered_module():
    assert re_guard.require_stdlib_re() is None


def test_bare_module_is_rejected():
    with pytest.raises(RuntimeError, match="no __spec__"):
        re_guard.require_stdlib_re(types.ModuleType("re"))


def test_unirex_module_is_rejected():
    unirex = pytest.importorskip("unirex")
    with pytest.raises(RuntimeError, match="not from the stdlib"):
        re_guard.require_stdlib_re(unirex)


def test_wrong_name_is_rejected():
    # A genuine-looking module under another name is still not `re`.
    fake = types.ModuleType("sre_compile")
    with pytest.raises(RuntimeError, match="__name__ is 'sre_compile'"):
        re_guard.require_stdlib_re(fake)


def test_forged_file_without_spec_is_rejected(tmp_path):
    fake = types.ModuleType("re")
    fake.__file__ = str(tmp_path / "re" / "__init__.py")
    with pytest.raises(RuntimeError, match="no __spec__"):
        re_guard.require_stdlib_re(fake)


def test_spec_outside_stdlib_is_rejected(tmp_path):
    origin = tmp_path / "re" / "__init__.py"
    origin.parent.mkdir()
    origin.write_text("")
    fake = types.ModuleType("re")
    fake.__spec__ = ModuleSpec("re", loader=None, origin=str(origin))
    fake.__file__ = str(origin)
    with pytest.raises(RuntimeError, match="not from the stdlib"):
        re_guard.require_stdlib_re(fake)


def test_forged_provenance_needs_cpython_markers(tmp_path):
    # Forges everything provenance-related, but is still not CPython's re.
    origin = tmp_path / "re" / "__init__.py"
    origin.parent.mkdir()
    origin.write_text("")
    fake = types.ModuleType("re")
    fake.__spec__ = ModuleSpec("re", loader=None, origin=str(origin))
    fake.__file__ = str(origin)
    fake._MAXCACHE = 512
    with pytest.raises(RuntimeError, match="no _cache"):
        re_guard.require_stdlib_re(fake)


def test_message_names_the_fix():
    with pytest.raises(RuntimeError) as excinfo:
        re_guard.require_stdlib_re(types.ModuleType("re"))
    message = str(excinfo.value)
    assert "separate pytest process" in message
    assert "make test-conformance" in message
