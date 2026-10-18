# Benchmarks

## Harness

`examples/bench.rs` is self-contained. It never depends on another checkout.

Corpus resolution:

1. `--corpus PATH` or `RUREX_TG_DATA`, for an offline or pre-downloaded copy.
2. `var/bench-data/tg-1m.json.gz`, if present and the SHA-256 matches.
3. Download from the public release URL, verify SHA-256, then cache it.
4. `--no-download` or a failed download prints the URL and checksum and exits.

The corpus URL is a public release asset. The URL lives under the `rurex`
project only as data provenance; this benchmark does not import or run the
`rurex` engine.

## Modes

- `ucs`: this engine over the actual CPython kind of each post (UCS-1, UCS-2,
  or UCS-4), using a reused `Scratch`.
- `bytes`: Rust `regex::bytes` over UTF-8, a same-machine engine baseline.
- `str`: Rust `regex` over UTF-8.

## Protocol

- Load, pack, and compile outside the timed window, using a cold one-pass
  protocol.
- Each post is scanned exactly once per run, counting matches with
  `find_iter_with` and `black_box`.
- The first run is cold; `--repeat N` adds warm runs. Report first, median,
  minimum, nanoseconds per post, and MB/s.
- `--threads N` splits posts into contiguous ranges. Each worker has its own
  `Scratch`; wall time is the slowest worker. Default is 1 thread.
- An independent naive counter runs during loading, untimed. Every engine
  mode must match it. For the full corpus, 334,136 mentions in 186,226 posts
  is also asserted.
- `--self-test` runs an embedded UCS-1/2/4 corpus offline.

## Corpus

- 1,000,000 posts, 526,880,801 uncompressed bytes.
- 334,136 mentions in 186,226 posts.
- Pattern: `@[a-zA-Z][a-zA-Z0-9_]{4,31}`.
- URL:
  `https://github.com/lorien/rurex/releases/download/bench-data-telegram-v1/tg-1m.json.gz`
- gzip SHA-256:
  `eb91db735fcf1a9e6096a59e959471cb9a3c4f41438f864e800cdb66f994023b`
- Uncompressed SHA-256:
  `02c51223e24db5b05acedd88ca6123167addeebc9328e450f7d0afd4bdec6c3d`
- Both checksums were verified against the cached corpus on 2026-10-18.

If the corpus is unavailable, a synthetic corpus may be used, but every
result must be labeled synthetic.

## Results

Machine: Intel Core i5-8350U, 8 logical cores, Linux, Rust 1.97.1. Full
corpus, engine-only Rust harness, `--repeat 3`, median reported.

Evolution, one thread:

- Milestone 1 PikeVM: 5.424 s.
- Milestone 3 prefilter: 1.266 s.
- Milestone 4 DFA: 0.360 s.

Same-thread baselines and engine results:

- One thread: DFA 0.360 s, Rust `regex` bytes 0.169 s, Rust `regex` str
  0.185 s.
- Four threads: DFA 0.110 s, Rust `regex` bytes 0.061 s, Rust `regex` str
  0.061 s.

The DFA is about 3.5 times faster than the prefiltered PikeVM. It is about
2.1 times slower than the Rust `regex` bytes engine at one thread and 1.8
times slower at four threads. It does not beat the same-thread baseline.

Kind distribution: 210,313 UCS-1 posts, 331,272 UCS-2 posts, and 458,415
UCS-4 posts; 19.1M, 95.2M, and 176.0M code units respectively. Forcing every
post to UCS-2 would misrepresent about 46 percent of the corpus.

The single-thread engine-only result is 0.360 s. The end-to-end benchmark now
exists; the remaining gap is engine optimization, not binding overhead.
Engine-only numbers must not be compared directly with another engine's
end-to-end time.

Patterns without a safe first-unit set still use the PikeVM. The DFA is an
optimization, not a change in the supported language or correctness.

Numbers from other engines or machines are external context and are not part
of this benchmark.

## End-to-end Python benchmark

`tools/bench_python.py` measures the same corpus through the Python binding.
It loads and compiles outside the timed window, then each worker scans its
slice once after a barrier. It resolves the corpus the same way as the Rust
harness and verifies 334,136 mentions in 186,226 posts.

Use a fresh process per point for the cold protocol. The following are five
fresh-process runs from this repository's own corpus cache, pinned to one CPU,
one thread, GIL build, full corpus:

- `exp_rurex`: 0.596 s, 0.597 s, 0.598 s, 0.599 s, 0.604 s. Median 0.598 s,
  minimum 0.596 s.
- CPython `re`: 0.490 s, 0.491 s, 0.501 s, 0.510 s, 0.648 s. Median 0.501 s,
  minimum 0.490 s.

The binding works and produces the correct mention counts, but it is slower
than CPython `re`: about 1.19 times slower at the median and 1.22 times slower
at the minimum.

The engine-only gap explains this: the DFA is 0.360 s versus 0.169 s for Rust
`regex` bytes at one thread. The binding overhead is not the main problem.
Closing the end-to-end gap requires engine optimization.

GIL 4-thread runs were not meaningfully faster; free-threaded CPython has not
been tested.

## Commands

    cargo run --release --example bench -- --self-test
    cargo run --release --example bench -- --mode ucs --limit 100000 --repeat 3
    cargo run --release --example bench -- --mode ucs --repeat 3 --threads 4
    cargo run --release --example bench -- --mode bytes --repeat 3
    cargo run --release --example bench -- --mode bytes --repeat 3 --threads 4
    cargo run --release --example bench -- --mode str --repeat 3 --threads 4
    PYTHONPATH=var/py python3 tools/bench_python.py --engine exp_rurex --repeat 1
    PYTHONPATH=var/py python3 tools/bench_python.py --engine re --repeat 1

Add `--json var/bench-results/<name>.json` to keep raw results. Never commit
the corpus or raw result files.
