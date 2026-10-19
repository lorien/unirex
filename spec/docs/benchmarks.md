# Benchmarks

## Harness

`examples/bench.rs` is self-contained. It never depends on another checkout.

Corpus resolution:

1. `--corpus PATH` or `UNIREX_TG_DATA`, for an offline or pre-downloaded copy.
2. `var/bench-data/tg-1m.json.gz`, if present and the SHA-256 matches.
3. Download from the public release URL, verify SHA-256, then cache it.
4. `--no-download` or a failed download prints the URL and checksum and exits.

The corpus URL is a public release asset. The URL lives under the external
`rurex` project only as data provenance; this benchmark does not import or run
the `rurex` engine.

## Per-interpreter binding benchmark

The binding is built and smoke-tested on CPython 3.10 through 3.15 and on the
free-threaded 3.14t and 3.15t builds. CI runs a 100,000-post end-to-end
benchmark for each of those interpreters with `--limit 100000 --repeat 2`, so a
per-interpreter regression shows up in the job log rather than in prose that
goes stale.

Reference point, local laptop, CPython 3.13.5 with the GIL, 100,000 posts, one
thread: median 0.0541 s, minimum 0.0536 s, 43,456 mentions.

Free-threaded builds are non-abi3 as well, so they need their own wheels, and
their GIL-detach path is exercised by the eight-worker phase of
`tools/test_python_binding.py`.

## Published report

`tools/telegram_benchmark.py` is the full matrix entry point. It holds
`/tmp/core4.lock`, runs the engine-only modes and the Python binding at 1 to
4 workers, writes JSON and a manifest under `var/bench-results/telegram/`, and
then calls `tools/telegram_report.py`. The renderer writes the tracked
[Telegram report](../../docs/telegram-benchmark.md) plus SVG chart assets; the
self-contained `docs/telegram-benchmark.html` twin is generated and
gitignored. Raw JSON is never committed.

Use `--from-results` to rerender without rerunning, or
`--results-dir var/bench-results/<name>` to render an existing matrix. The
report validates that every expected point is present and that all full-corpus
counts match.

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
- `--warmup N` and `--warmup-seconds F` run untimed passes before the measured
  runs. Warmup ramps CPU frequency and builds DFA caches.
- `--cpu-list` pins each worker to a CPU, for example `2` or `0,2,4,6`.
  `--threads N` splits posts into contiguous ranges. Each worker has its own
  `Scratch`; wall time is the slowest worker.
- The runner records per-worker CPU time, core ID, CPU efficiency, load and
  frequency. Any worker below `--min-efficiency` (default 0.90) causes the run
  to be discarded and retried up to `--retries` times.
- `--max-load F` aborts before warmup when the initial load average is above
  `F`.
- `--lock PATH` takes an exclusive advisory lease, default
  `/tmp/core4.lock`. `--lock-wait F` sets the timeout. `--no-lock` is for a
  matrix child whose parent already holds the lease.
- The runner does not change the CPU governor. It fails if it cannot obtain
  the requested number of clean runs.
- Report first, median, minimum, nanoseconds per post, MB/s, CPU time,
  efficiency, load, and frequency.
- An independent naive counter runs during loading, untimed. Every engine
  mode must match it. For the full corpus, 334,136 mentions in 186,226 posts
  is also asserted.
- `--self-test` runs an embedded UCS-1/2/4 corpus offline.

## Core lease

On `regexus`, benchmarks use the first four physical P-cores, logical CPUs
`0,2,4,6`; SMT siblings `1,3,5,7` stay idle. The lease is an exclusive
`flock` on `/tmp/core4.lock`, held for the whole matrix. It is advisory: all
benchmark agents must take it. It does not stop non-cooperating processes;
per-worker efficiency detects that contamination. The format and alternatives
are in ADR-0014.

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

## Remote results on regexus

Machine: Intel Core i5-13500, 14 physical cores, 20 logical threads, Debian,
Rust 1.99.1, Python 3.13.5. The benchmark used the first four physical P-cores:
CPUs `0,2,4,6`, one thread per core; SMT siblings `1,3,5,7` stayed idle.
Governor stayed `powersave`, turbo enabled. Engine frequency was 4.7 GHz to
4.8 GHz. Load was 0.35 to 1.27 from other work, but per-worker efficiency
stayed at 1.00. `/tmp/core4.lock` was held for the whole matrix.

Engine-only medians, full corpus, measured by `examples/bench.rs` calling the
Rust crate directly with no Python involved:

- 1 thread: UCS 0.0690 s, Rust `regex` bytes 0.0712 s, str 0.0711 s.
- 2 threads: UCS 0.0386 s, bytes 0.0377 s, str 0.0378 s.
- 3 threads: UCS 0.0294 s, bytes 0.0266 s, str 0.0266 s.
- 4 threads: UCS 0.0258 s, bytes 0.0209 s, str 0.0209 s.

UCS scales 2.67 times from 1 to 4 threads; bytes scales 3.41 times. UCS is
about 3.1 percent faster than bytes at one thread and 1.24 times slower at
four. The one-thread result is the parity target; four-thread UCS scaling is
limited by each worker scanning wider UCS storage and keeping its own DFA
cache.

End-to-end Python, same CPU lists, GIL, measured by `tools/bench_python.py`
importing the installed python binding into a Python process:

- 1 thread: `unirex` 0.1851 s median, 0.1830 s minimum; CPython `re`
  0.1923 s median, 0.1899 s minimum.
- 2 threads: `unirex` 0.1861 s median, 0.1776 s minimum; `re` 0.1890 s
  median, 0.1881 s minimum.
- 3 threads: `unirex` 0.1924 s median, 0.1703 s minimum; `re` 0.1923 s
  median, 0.1670 s minimum.
- 4 threads: `unirex` 0.1801 s median, 0.1617 s minimum; `re` 0.1829 s
  median, 0.1775 s minimum.

Naming rule for result tables: never write a bare `unirex` in a column header
or chart legend. Name the interface, `unirex binding` for the python binding or
`unirex crate` for the Rust API, and name the baselines the same way (`re`,
`regex bytes`, `regex str`). The two measurements use the same pattern and
corpus but their numbers are not comparable.

The GIL prevents meaningful scaling beyond one thread, so the 1-thread result
is the end-to-end target. `unirex` is 3.8 percent faster than CPython `re`
at the median and 3.6 percent faster at the minimum. The 2-4 thread medians
are noisy because the Python runs do not keep the cores at a stable frequency;
they are reported for completeness, not as scaling evidence. This CPU is much
faster than the original laptop baseline, and no `rurex` engine was run.

## Historical laptop results

The numbers below were collected before the load-aware runner and before the
remote lease. They are wall-clock only and did not record CPU time, frequency,
or contamination. Treat them as provisional history, not current results.

Load-aware attempt on 2026-10-18: an external four-thread benchmark kept the
machine at load near 3. The runner rejected all four-thread attempts and most
one-thread bytes/str attempts. One UCS one-thread set produced five clean
samples at a 0.429 s median with observed frequency 2.8 GHz to 3.0 GHz, which
is not comparable to the earlier 0.360 s at higher frequency. No new official
numbers are claimed until the machine is idle.

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

## Historical end-to-end laptop benchmark

`tools/bench_python.py` measures the same corpus through the Python binding.
It loads and compiles outside the timed window, then each worker scans its
slice once after a barrier. It resolves the corpus the same way as the Rust
harness and verifies 334,136 mentions in 186,226 posts.

These end-to-end numbers are also provisional: they were collected before
CPU-time accounting and efficiency rejection were added.

Use a fresh process per point. The following are five fresh-process runs from
this repository's own corpus cache, pinned to one CPU, one thread, GIL build,
full corpus:

- `unirex`: 0.596 s, 0.597 s, 0.598 s, 0.599 s, 0.604 s. Median 0.598 s,
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
    cargo run --release --example bench -- --mode ucs --limit 100000 --cpu-list 2 --warmup-seconds 1 --repeat 3 --min-efficiency 0.90
    cargo run --release --example bench -- --mode ucs --cpu-list 2 --warmup-seconds 2 --repeat 5 --min-efficiency 0.90
    cargo run --release --example bench -- --mode ucs --threads 4 --cpu-list 0,1,2,3 --warmup-seconds 2 --repeat 5
    cargo run --release --example bench -- --mode bytes --cpu-list 2 --warmup-seconds 2 --repeat 5
    cargo run --release --example bench -- --mode str --cpu-list 2 --warmup-seconds 2 --repeat 5
    PYTHONPATH=var/py python3 tools/bench_python.py --engine unirex --cpu-list 2 --warmup-seconds 2 --repeat 5 --min-efficiency 0.90
    PYTHONPATH=var/py python3 tools/bench_python.py --engine re --cpu-list 2 --warmup-seconds 2 --repeat 5 --min-efficiency 0.90
    python3 tools/telegram_benchmark.py --cpu-list 0,2,4,6 --repeat 5 --warmup-seconds 2
    python3 tools/telegram_report.py --results-dir var/bench-results/telegram

Add `--json var/bench-results/<name>.json` to keep raw results. Never commit
the corpus or raw result files. The full matrix runner writes to
`var/bench-results/telegram/` and then renders the tracked report; use
`--from-results` to rerender without rerunning.
