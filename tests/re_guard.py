"""Guard against a replaced stdlib ``re``.

``tests/cpython/conftest.py`` swaps ``sys.modules["re"]`` for unirex so
CPython's own ``test_re.py`` runs against unirex. The swap is
process-wide and never undone, so a differential suite sharing that
process would bind unirex as its oracle and pass vacuously (unirex
compared with itself).

The guard does not look for unirex specifically: it requires the module
to be the one CPython itself ships, so any replacement is rejected.
"""

from __future__ import annotations

import sys
import sysconfig
from pathlib import Path

__all__ = ["shadow_reasons", "require_stdlib_re"]


def _origin_problem(re_mod, stdlib: Path) -> str | None:
    """Why ``re_mod``'s provenance is not CPython's stdlib, or None."""
    spec = getattr(re_mod, "__spec__", None)
    if spec is None:
        return "no __spec__ (synthetic module, not imported)"
    origin = getattr(spec, "origin", None)
    if not origin:
        return "__spec__ has no origin"
    path = Path(origin).resolve()
    # Python 3.10 ships re.py; 3.11+ ships re/__init__.py.
    if path != stdlib / "re.py" and path != stdlib / "re" / "__init__.py":
        return f"loaded from {str(path)!r}, not from the stdlib"
    file = getattr(re_mod, "__file__", None)
    if file is None:
        return "no __file__"
    if Path(file).resolve() != path:
        return f"__file__ {file!r} disagrees with __spec__.origin {origin!r}"
    return None


def shadow_reasons(re_mod) -> list[str]:
    """All reasons ``re_mod`` is not the genuine stdlib ``re``."""
    bad = []
    name = getattr(re_mod, "__name__", None)
    if name != "re":
        bad.append(f"__name__ is {name!r}, not 're'")

    stdlib = Path(sysconfig.get_path("stdlib")).resolve()
    problem = _origin_problem(re_mod, stdlib)
    if problem is not None:
        bad.append(problem)

    # CPython implementation markers (present on 3.10 through 3.15);
    # a replacement that forged __spec__ would have to fake these too.
    if not isinstance(getattr(re_mod, "_MAXCACHE", None), int):
        bad.append("no _MAXCACHE")
    if not isinstance(getattr(re_mod, "_cache", None), dict):
        bad.append("no _cache")
    if not callable(getattr(re_mod, "purge", None)):
        bad.append("no purge()")
    return bad


def require_stdlib_re(re_mod=None) -> None:
    """Raise RuntimeError unless ``re_mod`` (default: sys.modules["re"])
    is CPython's genuine stdlib ``re``."""
    if re_mod is None:
        re_mod = sys.modules.get("re")
    bad = shadow_reasons(re_mod)
    if bad:
        raise RuntimeError(
            "stdlib re has been replaced (%s): oracle-based suites must "
            "not share a process with the tests/cpython shim; run them "
            "in a separate pytest process (see `make test-conformance`)."
            % "; ".join(bad)
        )
