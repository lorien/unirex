# Benchmarks

## Harness

`examples/bench.rs` is self-contained. It never depends on another checkout.

Corpus resolution:

1. `--corpus PATH` or `RUREX_TG_DATA`, for an offline or pre-downloaded copy.
2. `var/bench-data/tg-1m.json.gz`, if present and the SHA-256 matches.
3. Download from the public release URL, verify SHA-256, then cache it.
4. `--no-download` or a failed download prints the URL and checksum and exits.

The corpus URL is the public `rurex` release asset. That is an external data
source, not a repository dependency.

## Modes

- `ucs`: this engine over the actual CPython kind of each post (UCS-1, UCS-2,
  or UCS-4), using a reused `Scratch`.
- `bytes`: Rust `regex::bytes` over UTF-8, a same-machine engine baseline.
- `str`: Rust `regex` over UTF-8.
- `at`: `memchr` count of `@` over UTF-8. This is a lower bound for a future
  first-unit prefilter, not an engine result.

## Protocol

- Load, pack, and compile outside the timed window, matching the `rurex`
  benchmark protocol.
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

Without a prefilter, milestone 1:

- `ucs`, 1 thread: 5.424 s.
- `ucs`, 4 threads: 1.796 s.

With the first-unit prefilter, milestone 3:

- `ucs`, 1 thread: 1.266 s, first 1.275 s, 334,136 mentions.
- `ucs`, 4 threads: 0.413 s.
- `bytes`, 1 thread: 0.179 s, with 0.15 s to 0.18 s across runs.
- `str`, 1 thread: 0.215 s.
- `at`, 1 thread: 0.077 s, 346,251 marks.

Kind distribution: 210,313 UCS-1 posts, 331,272 UCS-2 posts, and 458,415
UCS-4 posts; 19.1M, 95.2M, and 176.0M code units respectively. Forcing every
post to UCS-2 would misrepresent about 46 percent of the corpus.

Interpretation:

- The prefilter is about 4.3 times faster than the unfiltered PikeVM.
- It is still about 7 times slower than the Rust `regex` bytes engine at one
  thread, and about 2.3 times slower with four threads.
- The candidate scan is no longer the main cost. The remaining time is the
  PikeVM work at each `@` candidate. A lazy DFA is required for a competitive
  engine; ADR-0010 records that decision.
- These are engine-only numbers. They exclude Python call overhead and result
  string construction. The final end-to-end comparison against `rurex` str is
  milestone 5.

Historical `rurex` and CPython numbers remain external context; they were not
produced by this harness on this machine and are not used as a pass/fail
gate.

## Commands

    cargo run --release --example bench -- --self-test
    cargo run --release --example bench -- --mode ucs --limit 100000 --repeat 3
    cargo run --release --example bench -- --mode ucs --repeat 3 --threads 4
    cargo run --release --example bench -- --mode bytes --repeat 3
    cargo run --release --example bench -- --mode at --repeat 3

Add `--json var/bench-results/<name>.json` to keep raw results. Never commit
the corpus or raw result files.
