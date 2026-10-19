#!/usr/bin/env python3
"""End-to-end Telegram benchmark for the unirex Python binding.

The corpus is resolved from --corpus, RUREX_TG_DATA, the verified cache under
var/bench-data, or the public release URL. Loading and compilation are outside
the timed window. Each worker scans its slice once after a barrier.

Usage:
    PYTHONPATH=var/py python3 tools/bench_python.py --engine unirex
    PYTHONPATH=var/py python3 tools/bench_python.py --engine re
"""

import argparse
import fcntl
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
    if engine == "unirex":
        try:
            import unirex
        except ImportError:
            sys.path.insert(0, str(ROOT / "var" / "py"))
            import unirex
        return unirex
    if engine == "re":
        import re
        return re
    raise SystemExit(f"unknown engine: {engine}")


class CoreLease:
    """Advisory exclusive lease for the first four physical P-cores."""

    def __init__(self, path: Path, wait: float):
        self.path = path
        self.wait = wait
        self.file = None

    def __enter__(self):
        self.file = open(self.path, "a+", encoding="utf-8")
        try:
            os.chmod(self.path, 0o666)
        except OSError:
            pass
        deadline = time.monotonic() + self.wait
        while True:
            try:
                fcntl.flock(self.file.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except BlockingIOError:
                owner = self._owner()
                if time.monotonic() >= deadline:
                    raise SystemExit(
                        f"core lease {self.path} is busy after {self.wait:.0f}s; "
                        f"owner: {owner}"
                    )
                print(f"waiting for core lease {self.path}; owner: {owner}", file=sys.stderr)
                time.sleep(1)
        self.file.seek(0)
        self.file.truncate()
        json.dump(
            {
                "user": os.environ.get("USER", ""),
                "pid": os.getpid(),
                "host": platform.node(),
                "cpus": [0, 2, 4, 6],
                "start": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
                "command": " ".join(sys.argv),
            },
            self.file,
        )
        self.file.flush()
        return self

    def _owner(self) -> str:
        try:
            self.file.seek(0)
            value = self.file.read().strip()
            self.file.seek(0, os.SEEK_END)
            return value or "<unknown>"
        except OSError:
            return "<unknown>"

    def __exit__(self, _exc_type, _exc, _tb):
        if self.file is not None:
            try:
                fcntl.flock(self.file.fileno(), fcntl.LOCK_UN)
            finally:
                self.file.close()
        return False


def compile_pattern(engine: str, pattern: str):
    module = import_engine(engine)
    return module.compile(pattern)


def read_load() -> float:
    try:
        with open("/proc/loadavg", encoding="utf-8") as stream:
            return float(stream.read().split()[0])
    except OSError:
        return 0.0


def read_freq(cpu: int) -> int | None:
    try:
        with open(
            f"/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_cur_freq",
            encoding="utf-8",
        ) as stream:
            return int(stream.read().strip()) // 1000
    except (OSError, ValueError):
        return None


def parse_cpu_list(value: str) -> list[int]:
    result = []
    for part in value.split(","):
        part = part.strip()
        if "-" in part:
            start, end = (int(item) for item in part.split("-", 1))
            if start > end:
                raise ValueError(f"invalid CPU range {part!r}")
            result.extend(range(start, end + 1))
        else:
            result.append(int(part))
    return sorted(set(result))


def pin_current(cpu: int) -> None:
    try:
        os.sched_setaffinity(0, {cpu})
    except (AttributeError, OSError):
        pass


def current_cpu() -> int:
    try:
        path = f"/proc/self/task/{threading.get_native_id()}/stat"
        with open(path, encoding="utf-8") as stream:
            fields = stream.read().split()
        return int(fields[38])
    except (OSError, ValueError, AttributeError):
        return -1


def single_point(rx, texts: list[str], threads: int, cpus: list[int]):
    if cpus:
        threads = max(1, min(threads, len(cpus)))
    chunk_size = (len(texts) + threads - 1) // threads
    chunks = [
        texts[index * chunk_size:(index + 1) * chunk_size]
        for index in range(threads)
    ]
    barrier = threading.Barrier(threads)
    walls = [0.0] * threads
    counts = [0] * threads
    posts_with = [0] * threads
    thread_cpu = [0.0] * threads
    cores = [-1] * threads

    def work(index: int):
        part = chunks[index]
        if cpus:
            pin_current(cpus[index])
        cores[index] = current_cpu()
        barrier.wait()
        cpu_start = time.thread_time()
        start = time.perf_counter()
        mentions = 0
        with_matches = 0
        for text in part:
            found = len(rx.findall(text))
            mentions += found
            if found:
                with_matches += 1
        walls[index] = time.perf_counter() - start
        thread_cpu[index] = time.thread_time() - cpu_start
        counts[index] = mentions
        posts_with[index] = with_matches

    workers = [threading.Thread(target=work, args=(i,)) for i in range(threads)]
    for worker in workers:
        worker.start()
    for worker in workers:
        worker.join()
    return (
        max(walls),
        sum(counts),
        sum(posts_with),
        sum(thread_cpu),
        cores,
        threads,
    )


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
    parser.add_argument("--engine", choices=["unirex", "re"], default="unirex")
    parser.add_argument("--corpus", type=Path)
    parser.add_argument("--limit", type=int)
    parser.add_argument("--repeat", type=int, default=3)
    parser.add_argument("--warmup", type=int, default=1)
    parser.add_argument("--warmup-seconds", type=float, default=1.0)
    parser.add_argument("--min-efficiency", type=float, default=0.90)
    parser.add_argument("--retries", type=int, default=2)
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--cpu-list")
    parser.add_argument("--max-load", type=float)
    parser.add_argument("--lock", type=Path, default=Path("/tmp/core4.lock"))
    parser.add_argument("--lock-wait", type=float, default=600.0)
    parser.add_argument("--no-lock", action="store_true")
    parser.add_argument("--pattern", default=PATTERN)
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()
    if args.repeat < 1 or args.warmup < 0 or args.warmup_seconds < 0 or args.threads < 1:
        raise SystemExit("--repeat and --threads must be positive; warmup values must be non-negative")
    cpus = parse_cpu_list(args.cpu_list) if args.cpu_list else []
    load_before = read_load()
    if args.max_load is not None and load_before > args.max_load:
        raise SystemExit(
            f"load {load_before:.2f} exceeds --max-load {args.max_load:.2f}; retry when idle"
        )

    lease = None
    if not args.no_lock:
        lease = CoreLease(args.lock, args.lock_wait)
        lease.__enter__()

    path = resolve_corpus(args.corpus)
    texts = load_texts(path, args.limit)
    rx = compile_pattern(args.engine, args.pattern)
    sampled_cpus = cpus or [0]
    warmup_deadline = time.perf_counter() + args.warmup_seconds
    warmup_passes = 0
    while warmup_passes < args.warmup or time.perf_counter() < warmup_deadline:
        single_point(rx, texts, args.threads, cpus)
        warmup_passes += 1
        if warmup_passes > 100_000:
            break
    freq_before = [value for cpu in sampled_cpus if (value := read_freq(cpu)) is not None]
    runs = []
    discarded = 0
    attempts = 0
    process_cpu_start = time.process_time()
    while len(runs) < args.repeat and attempts < args.repeat + args.retries:
        attempts += 1
        point = single_point(rx, texts, args.threads, cpus)
        wall = point[0]
        cpu = point[3]
        efficiency = cpu / wall if wall > 0 else 0.0
        if efficiency >= args.min_efficiency:
            runs.append(point)
        else:
            discarded += 1
            print(
                f"discarding run: CPU efficiency {efficiency:.2} "
                f"below {args.min_efficiency:.2}; system load?",
                file=sys.stderr,
            )
    process_cpu = time.process_time() - process_cpu_start
    if not runs:
        raise SystemExit(
            f"no clean runs: CPU efficiency stayed below {args.min_efficiency:.2}; retry when idle"
        )
    if len(runs) < args.repeat:
        print(
            f"warning: only {len(runs)} clean runs out of requested {args.repeat}",
            file=sys.stderr,
        )
    freq_after = [value for cpu in sampled_cpus if (value := read_freq(cpu)) is not None]
    first_wall, mentions, posts_with, thread_cpu, cores, threads = runs[0]
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
        "threads": threads,
        "repeat": args.repeat,
        "warmup": args.warmup,
        "warmup_passes": warmup_passes,
        "warmup_seconds": args.warmup_seconds,
        "min_efficiency": args.min_efficiency,
        "retries": args.retries,
        "attempts": attempts,
        "discarded_runs": discarded,
        "cpu_list": cpus,
        "worker_cores": cores,
        "limit": args.limit,
        "posts": len(texts),
        "mentions": mentions,
        "posts_with_mentions": posts_with,
        "first_s": first_wall,
        "runs_s": walls,
        "first_cpu_s": thread_cpu,
        "process_cpu_s": process_cpu,
        "cpu_efficiency": (thread_cpu / first_wall) if first_wall > 0 else 0.0,
        "median_s": median,
        "min_s": min(walls),
        "load_before": load_before,
        "load_after": read_load(),
        "freq_before_mhz": freq_before,
        "freq_after_mhz": freq_after,
        "ns_per_post": (median * 1e9 / len(texts)) if texts else 0.0,
        "cpu": cpu_name(),
        "python": sys.version.split()[0],
        "gil_enabled": bool(getattr(sys, "_is_gil_enabled", lambda: True)()),
        "lock": None if args.no_lock else str(args.lock),
    }
    print(
        f"engine={args.engine} threads={threads} posts={len(texts)} "
        f"mentions={mentions} first={first_wall:.6f}s median={median:.6f}s "
        f"min={min(walls):.6f}s cpu={thread_cpu:.6f}s "
        f"efficiency={thread_cpu / first_wall if first_wall else 0:.2f} "
        f"load={load_before:.2f}->{read_load():.2f} "
        f"freq={freq_before}->{freq_after} cores={cores}"
    )
    if args.json:
        args.json.write_text(json.dumps(result, indent=2))
        print(f"wrote {args.json}")
    if lease is not None:
        lease.__exit__(None, None, None)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
