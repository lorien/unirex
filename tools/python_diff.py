#!/usr/bin/env python3
"""Differential check: unirex against CPython's `re` on the M1 subset.

Generates random patterns from the supported grammar, runs both engines over
all strings of length 0..4 over {a,b,c}, and compares `finditer` spans.

Usage:
    python3 tools/python_diff.py [--patterns N] [--seed S]

Exits nonzero on the first mismatch.
"""

import argparse
import itertools
import random
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def gen(depth: int) -> str:
    atoms = ["a", "b", "c", ".", "[abc]", "[^ab]", "[a-c]", "[b]"]
    choices = list(atoms)
    if depth > 0:
        choices += ["ALT", "CONCAT", "REP", "GROUP"]
    kind = random.choice(choices)
    if kind == "ALT":
        body = f"({gen(depth - 1)}|{gen(depth - 1)})"
    elif kind == "CONCAT":
        body = f"{gen(depth - 1)}{gen(depth - 1)}"
    elif kind == "REP":
        op = random.choice(["*", "+", "?", "{0,2}", "{1,3}", "{2}", "{2,}"])
        body = f"({gen(depth - 1)}){op}"
    elif kind == "GROUP":
        body = f"(?:{gen(depth - 1)})"
    else:
        body = kind
    if random.random() < 0.15:
        body = random.choice(["^", r"\A"]) + body
    if random.random() < 0.15:
        body = body + r"\Z"
    return body


def haystacks() -> list[str]:
    result = [""]
    for length in range(1, 5):
        for chars in itertools.product("abc", repeat=length):
            result.append("".join(chars))
    return result


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--patterns", type=int, default=1000)
    parser.add_argument("--seed", type=int, default=12345)
    args = parser.parse_args()

    random.seed(args.seed)
    hays = haystacks()
    cases = []
    seen = set()
    for _ in range(args.patterns):
        pattern = gen(2)
        if pattern in seen:
            continue
        seen.add(pattern)
        try:
            compiled = re.compile(pattern)
        except Exception:
            continue
        for hay in hays:
            expected = ";".join(
                f"{m.start()},{m.end()}" for m in compiled.finditer(hay)
            )
            cases.append((pattern, hay, expected))

    payload = "".join(f"{p}\t{h}\n" for p, h, _ in cases)
    proc = subprocess.run(
        ["cargo", "run", "--quiet", "--example", "oracle"],
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
    for (pattern, hay, expected), got in zip(cases, lines):
        if got != expected:
            print(
                f"MISMATCH pattern={pattern!r} hay={hay!r} "
                f"python={expected!r} ours={got!r}"
            )
            mismatches += 1
            if mismatches >= 20:
                break

    print(f"cases={len(cases)} mismatches={mismatches}")
    return 1 if mismatches else 0


if __name__ == "__main__":
    raise SystemExit(main())
