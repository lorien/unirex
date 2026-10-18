#!/usr/bin/env python3
"""End-to-end Telegram benchmark for the exp_rurex Python binding.

The corpus is resolved from --corpus, RUREX_TG_DATA, the verified cache under
var/bench-data, or the public release URL. Loading and compilation are outside
the timed window. Each worker scans its slice once after a barrier.

Usage:
    PYTHONPATH=var/py python3 tools/bench_python.py --engine exp_rurex
    PYTHONPATH=var/py python3 tools/bench_python.py --engine re
"""

import argparse
import gzip
import hashlib
import json
import os
import platform
import statistics
import sys
import threading
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA_URL = (
    "https://github.com/lorien/rurex/releases/download/"
    "bench-data-telegram-v1/tg-1m.json.gz"
)
DATA_SHA256 = (
    "eb91db735fcf1a9e6096a59e959471cb9a3c4f41438f864e800cdb66f994023b"
)
DATA_CACHE = ROOT / "var" / "bench-data" / "tg-1m.json.gz"
PATTERN = r"@[a-zA-Z][a-zA-Z0-9_]{4,31}"
FULL_MENTIONS = 334_136
FULL_POSTS_WITH = 186_226


def resolve_corpus(path: Path | None) -> Path:
    if path is not None:
        if not path.exists():
            raise SystemExit(f"corpus not found: {path}")
        return path
    env = os.environ.get("RUREX_TG_DATA")
    if env:
        path = Path(env)
        if not path.exists():
            raise SystemExit(f"RUREX_TG_DATA points to a missing file: {path}")
        return path
    if DATA_CACHE.exists():
        return DATA_CACHE
    print(f"downloading corpus from {DATA_URL}")
    DATA_CACHE.parent.mkdir(parents=True, exist_ok=True)
    tmp = DATA_CACHE.with_suffix(".part")
    digest = hashlib.sha256()
    with urllib.request.urlopen(DATA_URL) as response, open(tmp, "wb") as out:
        while True:
            chunk = response.read(1 << 20)
            if not chunk:
                break
            digest.update(chunk)
            out.write(chunk)
    got = digest.hexdigest()
    if got != DATA_SHA256:
        tmp.unlink(missing_ok=True)
        raise SystemExit(
            f"corpus sha256 mismatch: {got} != {DATA_SHA256}"
        )
    tmp.replace(DATA_CACHE)
    return DATA_CACHE


def load_texts(path: Path, limit: int | None) -> list[str]:
    texts = []
    opener = gzip.open if path.suffix == ".gz" else open
    with opener(path, "rt", encoding="utf-8") as stream:
        for line in stream:
            post = json.loads(line)
            text = post.get("text")
            if text is not None:
                texts.append(text)
                if limit and len(texts) >= limit:
                    break
    return texts


def import_engine(engine: str):
    if engine == "exp_rurex":
        try:
            import exp_rurex
        except ImportError:
            sys.path.insert(0, str(ROOT / "var" / "py"))
            import exp_rurex
        return exp_rurex
    if engine == "re":
        import re
        return re
    raise SystemExit(f"unknown engine: {engine}")


def compile_pattern(engine: str, pattern: str):
    module = import_engine(engine)
    return module.compile(pattern)


def single_point(rx, texts: list[str], threads: int):
    chunk_size = (len(texts) + threads - 1) // threads
    chunks = [
        texts[index * chunk_size:(index + 1) * chunk_size]
        for index in range(threads)
    ]
    barrier = threading.Barrier(threads)
    walls = [0.0] * threads
    counts = [0] * threads
    posts_with = [0] * threads

    def work(index: int):
        part = chunks[index]
        barrier.wait()
        start = time.perf_counter()
        mentions = 0
        with_matches = 0
        for text in part:
            found = len(rx.findall(text))
            mentions += found
            if found:
                with_matches += 1
        walls[index] = time.perf_counter() - start
        counts[index] = mentions
        posts_with[index] = with_matches

    workers = [threading.Thread(target=work, args=(i,)) for i in range(threads)]
    for worker in workers:
        worker.start()
    for worker in workers:
        worker.join()
    return max(walls), sum(counts), sum(posts_with)


def cpu_name() -> str:
    try:
        with open("/proc/cpuinfo", encoding="utf-8") as stream:
            for line in stream:
                if line.startswith("model name"):
                    return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or platform.machine()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--engine", choices=["exp_rurex", "re"], default="exp_rurex")
    parser.add_argument("--corpus", type=Path)
    parser.add_argument("--limit", type=int)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--pattern", default=PATTERN)
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()
    if args.repeat < 1 or args.threads < 1:
        raise SystemExit("--repeat and --threads must be positive")

    path = resolve_corpus(args.corpus)
    texts = load_texts(path, args.limit)
    rx = compile_pattern(args.engine, args.pattern)
    runs = []
    last = None
    for _ in range(args.repeat):
        last = single_point(rx, texts, args.threads)
        runs.append(last)
    first_wall, mentions, posts_with = runs[0]
    walls = [run[0] for run in runs]
    median = statistics.median(walls)
    if args.limit is None:
        if mentions != FULL_MENTIONS or posts_with != FULL_POSTS_WITH:
            raise SystemExit(
                f"count mismatch: {mentions} mentions in {posts_with} posts, "
                f"expected {FULL_MENTIONS} in {FULL_POSTS_WITH}"
            )
    result = {
        "engine": args.engine,
        "pattern": args.pattern,
        "threads": args.threads,
        "repeat": args.repeat,
        "limit": args.limit,
        "posts": len(texts),
        "mentions": mentions,
        "posts_with_mentions": posts_with,
        "first_s": first_wall,
        "runs_s": walls,
        "median_s": median,
        "min_s": min(walls),
        "ns_per_post": (median * 1e9 / len(texts)) if texts else 0.0,
        "cpu": cpu_name(),
        "python": sys.version.split()[0],
    }
    print(
        f"engine={args.engine} threads={args.threads} posts={len(texts)} "
        f"mentions={mentions} first={first_wall:.6f}s median={median:.6f}s "
        f"min={min(walls):.6f}s"
    )
    if args.json:
        args.json.write_text(json.dumps(result, indent=2))
        print(f"wrote {args.json}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
