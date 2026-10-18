# Telegram @mention extraction: exp_rurex vs `re`

Real-world task, measured as one `findall(text)` pass over every post in the corpus. `exp_rurex` matches CPython's UCS-1, UCS-2, or UCS-4 buffer directly; there is no UTF-8 conversion in the matching path. This page compares the UCS-native binding with CPython `re` on the same parsed strings, and separately records the engine-only Rust comparison.

## Methodology

- **Pattern** (Telegram username rules, 5 to 32 characters, `[a-zA-Z0-9_]`, starting with a letter): `@[a-zA-Z][a-zA-Z0-9_]{4,31}`
- **Corpus**: 1,000,000 posts, 470 MB of UTF-8 text (mostly Arabic and Persian), 334,136 mentions in 186,226 posts (18.6 percent of posts). The benchmark resolves the corpus from the cache or the [bench-data-telegram-v1 release](https://github.com/lorien/rurex/releases/tag/bench-data-telegram-v1) and verifies its SHA-256 before timing.
- **UCS storage**: the same posts occupy about 913 MB when packed as CPython UCS storage: 210,313 UCS-1, 331,272 UCS-2, and 458,415 UCS-4 posts. The engine scans these units directly.
- **End-to-end protocol**: each engine and thread point runs in a fresh process. The process loads the corpus, compiles the pattern once, runs untimed warm-up passes over the corpus, and then performs one timed `findall` per post over its slice. Workers are pinned to distinct CPUs and released by a barrier. The reported wall time is the maximum across workers.
- **Engine-only protocol**: each post is scanned exactly once per run with a reused `Scratch`. `ucs` uses the three CPython storage kinds, `bytes` uses Rust `regex::bytes` over UTF-8, and `str` uses Rust `regex` over UTF-8. An independent naive counter runs during loading and every mode must match it.
- **Core lease and contamination guard**: the whole matrix holds `/tmp/core4.lock`. Each point records CPU time, wall time, load, and per-CPU frequency. Runs below the efficiency floor or above the load limit are rejected and retried; a contaminated run is not published.
- **Timing window**: load, corpus resolution, pattern compilation, and warm-up are outside the window for both engines. The measured window starts when the barrier releases and stops when the worker's slice is finished.
- **Repeats**: published numbers are medians of 5 timed passes per point; the raw JSON records every run. Load was 0.09 to 1.04; engine-only frequency was 4.5 to 4.8 GHz; engine worker efficiency was 1.00. Python frequency samples are kept in the raw JSON as diagnostics.
- **Interpreter**: CPython 3.13.5 with the GIL. Free-threaded CPython and other interpreter versions are not measured on this page.

## End-to-end Python under the GIL

![End-to-end Python wall time by worker count](img/telegram_end_to_end.svg)

| Workers | exp_rurex median (s) | exp_rurex min (s) | re median (s) | re min (s) | exp_rurex/re |
|---:|---:|---:|---:|---:|---:|
| 1 | 0.1887 | 0.1838 | 0.1912 | 0.1905 | 0.99 |
| 2 | 0.1799 | 0.1443 | 0.1874 | 0.1862 | 0.96 |
| 3 | 0.1897 | 0.1874 | 0.1889 | 0.1854 | 1.00 |
| 4 | 0.1660 | 0.1599 | 0.1888 | 0.1782 | 0.88 |

A ratio below 1 means `exp_rurex` is faster. At one thread, `exp_rurex` is 0.1887 s median and 0.1838 s minimum; CPython `re` is 0.1912 s median and 0.1905 s minimum. That is 1.3 percent faster at the median and 3.5 percent faster at the minimum.

The GIL prevents meaningful scaling beyond one thread. The two- to four-worker medians stay close to the one-thread time, and the minimums are noisy because each scan is short and Python result construction is serialized. The one-thread result is the meaningful end-to-end comparison on this page.

## Engine-only comparison

![Engine-only wall time by worker count](img/telegram_engine_only.svg)

| Workers | exp_rurex UCS (s) | Rust regex bytes (s) | Rust regex str (s) | UCS/bytes |
|---:|---:|---:|---:|---:|
| 1 | 0.0689 | 0.0715 | 0.0715 | 0.96 |
| 2 | 0.0388 | 0.0380 | 0.0376 | 1.02 |
| 3 | 0.0296 | 0.0270 | 0.0271 | 1.09 |
| 4 | 0.0257 | 0.0209 | 0.0212 | 1.23 |

Engine-only and end-to-end numbers answer different questions. At one thread, UCS is 0.0689 s median versus 0.0715 s for Rust `regex` bytes and 0.0715 s for Rust `regex` str. The UCS path is 3.6 percent faster than the bytes baseline at one thread.

At four threads the picture changes: UCS is 0.0257 s versus 0.0209 s for bytes and 0.0212 s for str, or 1.23 times the bytes time. The UCS path streams about 913 MB of storage against 470 MB of UTF-8, and each worker keeps its own DFA cache. This is the remaining four-thread scaling gap. It does not change the one-thread end-to-end result.

## Reading the numbers

Parity here means the one-thread end-to-end Telegram result, not general `re` compatibility. The supported dialect is still a strict subset: `\w`, `\d`, `\s`, `\b`, `$`, flags, lazy repetition, and capture values are not implemented yet, and only group 0 is exposed. A user who needs those constructs must continue to use CPython `re`.

The engine-only numbers exclude Python call overhead, list construction, and result strings. They are useful for understanding the matcher, but they are not directly comparable with the end-to-end table.

## Free-threaded Python

Not measured yet. The binding is verified on CPython 3.13 with the GIL; free-threaded CPython 3.14t and CPython 3.10 through 3.15 are open verification tasks. The report generator rejects free-threaded points until this section is implemented, so a mixed matrix cannot silently publish GIL prose.

## Reproducing

```sh
cargo build --release --example bench
cargo build --release --features extension-module
mkdir -p var/py
cp target/release/libexp_rurex.so var/py/exp_rurex.so
python3 tools/telegram_benchmark.py --cpu-list 0,2,4,6 --repeat 5 --warmup-seconds 2

# Re-render the report from saved JSON without rerunning:
python3 tools/telegram_report.py --results-dir var/bench-results/telegram
```

---

*Measured 2026-10-18T23:15:10+00:00 on 13th Gen Intel(R) Core(TM) i5-13500; Python 3.13.5; compiler rustc 1.99.1 (58cf8f9c3 2026-10-13); source e63f0ee2bc3424a8d81abcc6cbd9a15996bccb37. Raw JSON lives under `var/bench-results/telegram` and is not committed. The standalone HTML twin is generated at `docs/telegram-benchmark.html` and is gitignored.*
