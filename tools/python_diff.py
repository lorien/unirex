#!/usr/bin/env python3
"""Differential check: unirex against CPython's `re`.

Generates random patterns from the supported grammar, runs both engines over
short strings built from an alphabet that exercises `\\w`, `\\d`, `\\s`, and
non-ASCII storage, and compares every capture span on UCS-1, UCS-2, and UCS-4.

Usage:
    python3 tools/python_diff.py [--patterns N] [--seed S] [--max-length N]

Exits nonzero on the first mismatch beyond the report limit.
"""

import argparse
import itertools
import random
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Letters, digits, underscore, whitespace, one non-ASCII letter, one Arabic-Indic
# digit, and one code point that forces UCS-4.
ALPHABET = ["a", "1", "_", " ", "é", "\u0663", "\U0001f600"]


def gen(depth: int) -> str:
    atoms = [
        "a",
        "1",
        " ",
        ".",
        "[abc]",
        "[^ab]",
        "[a-c]",
        r"\w",
        r"\W",
        r"\d",
        r"\D",
        r"\s",
        r"\S",
        "[\\w\\d]",
        r"\b",
        r"\B",
        "é",
    ]
    choices = list(atoms)
    if depth > 0:
        choices += ["ALT", "CONCAT", "REP", "LAZY", "GROUP", "SCOPED"]
    kind = random.choice(choices)
    if kind == "ALT":
        body = f"({gen(depth - 1)}|{gen(depth - 1)})"
    elif kind == "CONCAT":
        body = f"{gen(depth - 1)}{gen(depth - 1)}"
    elif kind == "REP":
        op = random.choice(["*", "+", "?", "{0,2}", "{1,3}", "{2}", "{2,}"])
        body = f"({gen(depth - 1)}){op}"
    elif kind == "LAZY":
        op = random.choice(["*?", "+?", "??", "{1,2}?"])
        body = f"({gen(depth - 1)}){op}"
    elif kind == "GROUP":
        body = f"({gen(depth - 1)})"
    elif kind == "SCOPED":
        flag = random.choice(["i", "s", "m"])
        body = f"(?{flag}:{gen(depth - 1)})"
    else:
        body = kind
    if random.random() < 0.12:
        body = random.choice(["^", r"\A", r"\b"]) + body
    if random.random() < 0.12:
        body = body + r"\Z"
    if random.random() < 0.10:
        body = body + "$"
    if random.random() < 0.15:
        body = random.choice(["(?i)", "(?s)", "(?m)"]) + body
    return body


def haystacks(max_length: int) -> list[str]:
    result = [""]
    for length in range(1, max_length + 1):
        for chars in itertools.product(ALPHABET, repeat=length):
            result.append("".join(chars))
    return result


def kinds_for(hay: str) -> list[str]:
    points = [ord(ch) for ch in hay]
    kinds = ["ucs4"]
    if all(point <= 0xFFFF for point in points):
        kinds.append("ucs2")
    if all(point <= 0xFF for point in points):
        kinds.append("ucs1")
    return kinds


def expected_spans(pattern: re.Pattern, hay: str) -> str:
    matches = []
    for match in pattern.finditer(hay):
        parts = [str(match.start()), str(match.end())]
        for index in range(1, pattern.groups + 1):
            start, end = match.span(index)
            parts.extend([str(start), str(end)])
        matches.append(",".join(parts))
    return ";".join(matches)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--patterns", type=int, default=300)
    parser.add_argument("--seed", type=int, default=12345)
    parser.add_argument("--max-length", type=int, default=3)
    parser.add_argument("--max-mismatches", type=int, default=20)
    args = parser.parse_args()

    random.seed(args.seed)
    hays = haystacks(args.max_length)
    cases = []
    seen = set()
    for _ in range(args.patterns):
        pattern = gen(2)
        if pattern in seen:
            continue
        seen.add(pattern)
        try:
            compiled = re.compile(pattern)
        except re.error:
            continue
        for hay in hays:
            expected = expected_spans(compiled, hay)
            points = ",".join(str(ord(ch)) for ch in hay)
            for kind in kinds_for(hay):
                cases.append((pattern, kind, points, hay, expected))

    payload = "".join(f"{p}\t{k}\t{pts}\n" for p, k, pts, _, _ in cases)
    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--release", "--example", "oracle"],
        input=payload,
        text=True,
        capture_output=True,
        cwd=ROOT,
    )
    if proc.returncode != 0:
        sys.stderr.write(proc.stderr[-4000:])
        return 1

    lines = proc.stdout.splitlines()
    if len(lines) != len(cases):
        print(
            f"oracle produced {len(lines)} lines for {len(cases)} cases",
            file=sys.stderr,
        )
        return 1

    mismatches = 0
    for (pattern, kind, points, hay, expected), got in zip(cases, lines):
        if got != expected:
            print(
                f"MISMATCH pattern={pattern!r} kind={kind} hay={hay!r} "
                f"python={expected!r} ours={got!r}"
            )
            mismatches += 1
            if mismatches >= args.max_mismatches:
                break

    print(f"cases={len(cases)} mismatches={mismatches}")
    return 1 if mismatches else 0


if __name__ == "__main__":
    raise SystemExit(main())
