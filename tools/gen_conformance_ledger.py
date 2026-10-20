#!/usr/bin/env python3
"""Generate the conformance ledger for the running interpreter.

Runs the vendored CPython suite for `sys.version_info[:2]`, records every
failing test with a bucketed reason, and writes `tests/cpython/<ver>/EXPECTED.py`.
Run it after vendoring a new CPython tag, then review the buckets and move
genuine gaps from `todo` to `unsupported` where the engine rejects by decision.

    PYTHONPATH=var/py python3 tools/gen_conformance_ledger.py
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSION = f"{sys.version_info[0]}.{sys.version_info[1]}"
VERSION_DIR = ROOT / "tests" / "cpython" / VERSION

BUCKETS = (
    # Ordered: the first matching rule wins.
    ("ascii flag", "unsupported: ASCII flag is rejected; classes follow CPython's Unicode tables"),
    ("locale", "unsupported: LOCALE flag is rejected"),
    ("debug flag", "unsupported: DEBUG flag is rejected"),
    ("backreference", "unsupported: pattern backreferences are rejected"),
    ("atomic", "unsupported: atomic groups are rejected"),
    ("possessive", "unsupported: possessive quantifiers are rejected"),
    ("lookaround", "unsupported: lookaround is rejected"),
    ("look-behind", "unsupported: lookbehind is rejected"),
    ("lookbehind", "unsupported: lookbehind is rejected"),
    ("conditional", "unsupported: conditional groups are rejected"),
    ("\\n{", "unsupported: named character escapes are rejected"),
    ("character-class set operations", "unsupported: class set operations are rejected"),
    ("global flags outside the start", "unsupported: global inline flags are only allowed at the pattern start"),
    ("regex parse error", "unsupported: syntax the parser rejects"),
    ("takes 1 positional arguments", "todo: pos/endpos parameters are not accepted yet"),
    ("unexpected keyword", "todo: keyword arguments are not accepted yet"),
    ("not subscriptable", "todo: Match.__getitem__ missing"),
    ("has no attribute", "todo: API surface missing"),
    ("expected str or bytes", "todo: argument type handling differs from re"),
    ("bytes", "todo: bytes behaviour still differs from re"),
    ("error not raised", "todo: validation that re performs is missing"),
    ("not triggered", "todo: warning that re emits is missing"),
    ("!=", "todo: match result differs from re"),
    ("no error", "todo: validation that re performs is missing"),
)


# Failures whose message is not a reliable signal, classified by hand.
OVERRIDES = {
    "ImplementationTest.test_repeat_minmax_overflow_maxrepeat": "by-design: repetition count limit is lower than re's",
    "ReTests.test_repeat_minmax_overflow": "by-design: repetition count limit is lower than re's",
    "ReTests.test_bigcharset": "harness: the test builds a string the extension cannot be handed directly",
    "ReTests.test_bug_1661": "todo: a compiled pattern is not accepted as the first argument",
    "ReTests.test_compile": "todo: a compiled pattern is not accepted as the first argument",
    "ReTests.test_copying": "by-design: patterns are not picklable or copyable yet",
    "ReTests.test_pickling": "by-design: patterns are not picklable or copyable yet",
    "ReTests.test_weakref": "todo: Pattern does not support weak references",
    "ReTests.test_enum": "todo: flag constants are plain ints, not a RegexFlag enum",
    "ReTests.test_large_search": "harness: bigmem test, skipped without UNIREX_BIGMEM",
    "ReTests.test_large_subn": "harness: bigmem test, skipped without UNIREX_BIGMEM",
    "ReTests.test_memory_leaks": "harness: needs a debug build of CPython",
    "ReTests.test_possessive_quantifiers": "unsupported: possessive quantifiers are rejected",
    "ReTests.test_symbolic_refs": "unsupported: named backreferences are rejected",
    "ReTests.test_scanner": "todo: Scanner implements only match and search",
    "ReTests.test_search_anchor_at_beginning": "todo: the test asserts a timing bound this engine does not meet",
    "ReTests.test_re_escape": "todo: escape differs from re for some inputs",
    "ReTests.test_re_escape_bytes": "todo: escape differs from re for some inputs",
    "ReTests.test_re_escape_non_ascii_bytes": "todo: escape differs from re for some inputs",
    "ReTests.test_re_match": "todo: Match.group does not accept the pos/endpos form",
}


def bucket(message: str) -> str:
    lowered = message.lower()
    for needle, reason in BUCKETS:
        if needle in lowered:
            return reason
    return "todo: unclassified failure, triage by hand"


def main() -> int:
    out = ROOT / "var" / "conformance-ledger.tsv"
    out.parent.mkdir(parents=True, exist_ok=True)
    env = {"PYTHONPATH": f"{ROOT / 'var' / 'py'}:{ROOT / 'tests' / 'cpython'}"}
    suite = VERSION_DIR / "vendored_test_re.py"
    if not suite.exists():
        print(f"no vendored suite for CPython {VERSION} at {suite}")
        return 1
    result = subprocess.run(
        [sys.executable, "-m", "pytest", str(suite), "-q", "--tb=no",
         "-p", "ledger_plugin"],
        cwd=ROOT,
        env={**__import__("os").environ, **env, "UNIREX_LEDGER_OUT": str(out), "UNIREX_LEDGER_IGNORE": "1"},
        capture_output=True,
        text=True,
    )
    print(result.stdout.splitlines()[-1] if result.stdout else result.stderr[-500:])
    entries: dict[str, str] = {}
    for line in out.read_text().splitlines():
        if "\t" not in line:
            continue
        test_id, message = line.split("\t", 1)
        key = test_id.replace("::", ".")
        entries.setdefault(key, OVERRIDES.get(key, bucket(message)))
    lines = [
        f'"""Conformance ledger for the CPython {VERSION} suite.',
        "",
        "Generated by `tools/gen_conformance_ledger.py`; review the buckets by hand.",
        'Every test that does not pass is listed here with a bucket and reason.',
        'Closing a gap means deleting its entry."""',
        "",
        "EXPECTED = {",
    ]
    for key in sorted(entries):
        lines.append(f'    "{key}": "{entries[key]}",')
    lines.append("}")
    (VERSION_DIR / "EXPECTED.py").write_text("\n".join(lines) + "\n")
    print(f"wrote {VERSION_DIR / 'EXPECTED.py'} with {len(entries)} entries")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
