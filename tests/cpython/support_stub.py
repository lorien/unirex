"""Minimal faithful stand-in for the pieces of CPython's `test.support`
used by the vendored `test_re.py` (see tests/cpython/vendored_test_re.py
provenance header). Installed as the `test.support` module by conftest.
"""

import gc
import io
import os
import time
import warnings
from contextlib import contextmanager, redirect_stdout

_2G = 2 ** 31
SHORT_TIMEOUT = 30.0

is_emscripten = False
is_wasi = False


def gc_collect():
    gc.collect()


def bigmemtest(size, memuse, dry_run=True):
    """Decorator marking a test that wants `size` bytes of memory.

    The test receives the size as its second argument, like CPython's.
    Skipped unless UNIREX_BIGMEM=1 (2 GB haystacks by default).
    """

    def decorator(f):
        def wrapper(self):
            if not os.environ.get("UNIREX_BIGMEM"):
                import unittest

                raise unittest.SkipTest(
                    "bigmem test (set UNIREX_BIGMEM=1 to run)"
                )
            return f(self, size)

        wrapper.__name__ = f.__name__
        wrapper._unirex_bigmem = True
        return wrapper

    return decorator


def cpython_only(f):
    """Run the test; unirex targets the CPython-compatible surface."""
    return f


def requires_resource(name):
    def decorator(f):
        return f

    return decorator


@contextmanager
def captured_stdout():
    buf = io.StringIO()
    with redirect_stdout(buf):
        yield buf


def check_disallow_instantiation(testcase, tp, *args):
    with testcase.assertRaises(TypeError):
        tp(*args)


class _WarningsHelperModule:
    @staticmethod
    @contextmanager
    def _catch(category):
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", category)
            yield

    @classmethod
    def ignore_warnings(cls, category=Warning):
        def decorator(f):
            def wrapper(*args, **kw):
                with cls._catch(category):
                    return f(*args, **kw)

            wrapper.__name__ = f.__name__
            return wrapper

        return decorator


warnings_helper = _WarningsHelperModule()


class CPUStopwatch:
    """Wall-clock stopwatch with a `.seconds` attribute."""

    def __enter__(self):
        self._t0 = time.monotonic()
        self.seconds = 0.0
        return self

    def __exit__(self, *exc):
        self.seconds = time.monotonic() - self._t0
        return False
