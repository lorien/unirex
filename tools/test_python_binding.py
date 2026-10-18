#!/usr/bin/env python3
"""Smoke-test the exp_rurex Python binding against CPython `re`.

Run with the built module on PYTHONPATH, for example:
    PYTHONPATH=var/py python3 tools/test_python_binding.py
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
try:
    import exp_rurex
except ImportError:
    sys.path.insert(0, str(ROOT / "var" / "py"))
    import exp_rurex

PATTERNS = [
    "a",
    "abc",
    "a|ab",
    "ab|a",
    "a*",
    "a+b",
    "a?b",
    "[a-c]+",
    "[^a]+",
    ".",
    "^a",
    r"\Aa",
    r"a\Z",
    r"a\Z|b",
    "[a-c]+x",
]
HAYSTACKS = [
    "",
    "a",
    "b",
    "ab",
    "abc",
    "xabc",
    "abcx",
    "aaab",
    "baa",
    "zzb",
    "\n",
    "مرحبا @user_name",
    "😀",
    "\ud800",
    "a\ud800b",
]


def main() -> int:
    for pattern in PATTERNS:
        ours = exp_rurex.compile(pattern)
        reference = re.compile(pattern)
        for text in HAYSTACKS:
            got = ours.findall(text)
            want = reference.findall(text)
            if got != want:
                print(f"MISMATCH pattern={pattern!r} text={text!r} ours={got!r} re={want!r}")
                return 1
            one = ours.find(text)
            one_ref = reference.search(text)
            got_one = one
            want_one = one_ref.group(0) if one_ref else None
            if got_one != want_one:
                print(
                    f"FIND MISMATCH pattern={pattern!r} text={text!r} "
                    f"ours={got_one!r} re={want_one!r}"
                )
                return 1
            if ours.is_match(text) != bool(one_ref):
                print(f"IS_MATCH MISMATCH pattern={pattern!r} text={text!r}")
                return 1
    print(f"python binding smoke test ok: {len(PATTERNS)} patterns")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
