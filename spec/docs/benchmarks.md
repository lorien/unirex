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

The single-thread engine-only result is 0.360 s. The end-to-end target is
0.75 s, so the remaining question is Python binding and result construction
overhead. That is milestone 5. Engine-only numbers must not be compared
directly with `rurex` end-to-end time.

Patterns without a safe first-unit set still use the PikeVM. The DFA is an
optimization, not a change in the supported language or correctness.

Historical `rurex` and CPython numbers remain external context; they were not
produced by this harness on this machine and are not used as a pass/fail
gate.

## Commands

    cargo run --release --example bench -- --self-test
    cargo run --release --example bench -- --mode ucs --limit 100000 --repeat 3
    cargo run --release --example bench -- --mode ucs --repeat 3 --threads 4
    cargo run --release --example bench -- --mode bytes --repeat 3
    cargo run --release --example bench -- --mode bytes --repeat 3 --threads 4
    cargo run --release --example bench -- --mode str --repeat 3 --threads 4

Add `--json var/bench-results/<name>.json` to keep raw results. Never commit
the corpus or raw result files.
