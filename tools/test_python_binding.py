#!/usr/bin/env python3
"""Smoke-test the unirex Python binding against CPython `re`.

Run with the built module on PYTHONPATH, for example:
    PYTHONPATH=var/py python3 tools/test_python_binding.py
"""

import re
import sys
import threading
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
try:
    import unirex
except ImportError:
    sys.path.insert(0, str(ROOT / "var" / "py"))
    import unirex

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
    r"(a)(b)",
    r"(a+)(b+)",
    r"(a)+",
    r"(a)?(b)",
    r"(a)|(b)",
    r"([a-c]+)(x)",
    r"((a)*)",
    r"()(a)",
    r"\w+",
    r"\W+",
    r"\d+",
    r"\s+",
    r"[\w.]+",
    r"[^\w]+",
    r"\bfoo\b",
    r"\Bfoo",
    r"foo$",
    r"\b",
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
        ours = unirex.compile(pattern)
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
    first = unirex.compile("a+")
    second = unirex.compile("[0-9]+")
    for _ in range(1000):
        if first.findall("baaab") != ["aaa"]:
            print("SCRATCH MISMATCH first pattern")
            return 1
        if second.findall("x123y") != ["123"]:
            print("SCRATCH MISMATCH second pattern")
            return 1

    long_text = "x" * 5000 + "bbb" + "y" * 5000
    long_re = unirex.compile("b+")
    if long_re.findall(long_text) != ["bbb"]:
        print("DETACH MISMATCH")
        return 1

    failures = []

    def worker(regex_a, regex_b):
        for _ in range(500):
            if regex_a.findall("baaab") != ["aaa"]:
                failures.append("thread regex_a")
                return
            if regex_b.findall("x123y") != ["123"]:
                failures.append("thread regex_b")
                return

    threads = [
        threading.Thread(target=worker, args=(first, second)) for _ in range(4)
    ]
    for thread in threads:
        thread.start()
    for thread in threads:
        thread.join()
    if failures:
        print(f"THREAD MISMATCH: {failures[0]}")
        return 1

    if unirex.compile("(a)(b)").groups != 2:
        print("GROUPS ATTRIBUTE MISMATCH")
        return 1
    if unirex.compile("ab").groups != 0:
        print("GROUPS ATTRIBUTE MISMATCH for group-free pattern")
        return 1

    long_grouped = unirex.compile(r"([a-c]+)(x)")
    long_text = "q" * 5000 + "abcx" + "q" * 5000
    if long_grouped.findall(long_text) != [("abc", "x")]:
        print("DETACH GROUP MISMATCH")
        return 1

    for pattern, text in [
        (r"(\w+)@(\w+)\.(\w+)", "mail me@example.com and you@test.org"),
        (r"(?P<user>[a-z]+)", "abc def"),
        (r"(a)?(b)", "b"),
        (r"(?i)([a-z]+)", "ABC xyz"),
    ]:
        ours = unirex.compile(pattern)
        reference = re.compile(pattern)
        got = [(m.span(), m.groups(), m.group(0), tuple(m.groupdict().items())) for m in ours.finditer(text)]
        want = [(m.span(), m.groups(), m.group(0), tuple(m.groupdict().items())) for m in reference.finditer(text)]
        if got != want:
            print(f"FINDITER MISMATCH pattern={pattern!r} text={text!r} ours={got!r} re={want!r}")
            return 1
    indexed = unirex.compile(r"(?P<user>[a-z]+)")
    if dict(indexed.groupindex) != {"user": 1}:
        print("GROUPINDEX MISMATCH")
        return 1

    api_pattern, api_text = r"(\w+)@(\w+)", "mail me@host now"
    ours, reference = unirex.compile(api_pattern), re.compile(api_pattern)
    checks = [
        ("search", lambda rx: [(m.span(), m.groups()) for m in [rx.search(api_text)] if m]),
        ("match", lambda rx: [(m.span(), m.groups()) for m in [rx.match(api_text)] if m]),
        ("fullmatch", lambda rx: [(m.span()) for m in [rx.fullmatch("ab@cd")] if m]),
        ("sub", lambda rx: [rx.sub(r"\2:\1", api_text)]),
        ("subn", lambda rx: [rx.subn("X", api_text, 1)]),
        ("split", lambda rx: [rx.split("a b,c")]),
        ("flags", lambda rx: [rx.findall("a b,c")]),
    ]
    for name, run in checks:
        got, want = run(ours), run(reference)
        if got != want:
            print(f"API MISMATCH {name}: ours={got!r} re={want!r}")
            return 1
    verbose = unirex.compile(r"(?x) a b  # comment")
    if verbose.findall("ab") != re.findall(r"(?x) a b  # comment", "ab"):
        print("VERBOSE MISMATCH")
        return 1
    try:
        unirex.compile("(?P<1bad>a)")
    except unirex.error as err:
        if err.pattern != "(?P<1bad>a)" or err.pos is None:
            print("ERROR ATTRIBUTES MISMATCH")
            return 1
    else:
        print("ERROR TYPE MISMATCH: expected unirex.error")
        return 1

    print(f"python binding smoke test ok: {len(PATTERNS)} patterns")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
