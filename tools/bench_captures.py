#!/usr/bin/env python3
"""Compare `findall` cost with and without capture groups.

A quick diagnostic, not a published benchmark: it does not pin CPUs, does not
touch the core lease, and does not take medians over a matrix. Use it to see
whether capture extraction costs work proportional to matched text.

    PYTHONPATH=var/py python3 tools/bench_captures.py --limit 200000
"""

from __future__ import annotations

import argparse
import re as stdlib_re
import time

import unirex

from bench_python import load_texts, resolve_corpus

PATTERNS = (
    r"@[a-zA-Z][a-zA-Z0-9_]{4,31}",
    r"@([a-zA-Z])([a-zA-Z0-9_]{4,31})",
    r"@([a-zA-Z])([a-zA-Z0-9_]{4,31})_(x)?",
)


def measure(compile_fn, pattern: str, texts: list[str], repeat: int) -> float:
    rx = compile_fn(pattern)
    best = float("inf")
    for _ in range(repeat):
        start = time.perf_counter()
        for text in texts:
            rx.findall(text)
        best = min(best, time.perf_counter() - start)
    return best


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", type=str, default=None)
    parser.add_argument("--limit", type=int, default=200_000)
    parser.add_argument("--repeat", type=int, default=5)
    args = parser.parse_args()

    texts = load_texts(resolve_corpus(args.corpus), args.limit)
    print(f"posts={len(texts)} repeat={args.repeat}")

    for pattern in PATTERNS:
        ours = measure(unirex.compile, pattern, texts, args.repeat)
        reference = measure(stdlib_re.compile, pattern, texts, args.repeat)
        groups = unirex.compile(pattern).groups
        print(
            f"{pattern:45} groups={groups} "
            f"unirex={ours:7.4f}s ({ours / len(texts) * 1e9:6.0f} ns/post) "
            f"re={reference:7.4f}s ({reference / len(texts) * 1e9:6.0f} ns/post) "
            f"unirex/re={ours / reference:.2f}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
