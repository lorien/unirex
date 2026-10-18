# Open Tasks

Open-task list. Format, statuses, and lifecycle:
`spec/skills/task_tracking.md`. Priorities: higher number = more important;
`-1` = deferred.

## Collect a load-aware benchmark baseline

Status: new
Priority: 4

Run the engine-only and end-to-end benchmarks on an idle laptop with
`--cpu-list`, `--warmup-seconds`, CPU-time accounting, and
`--min-efficiency`. The current numbers predate this protocol and are
provisional. The last attempt was rejected because an external benchmark kept
the machine loaded. Record CPU frequency, load, per-worker efficiency, and the
exact CPU list. Acceptance: the requested number of clean runs for UCS, bytes,
and str at 1 and 4 threads, and for `exp_rurex` and `re` end to end at 1
thread, with all conditions recorded.

References: ADR-0013

## Optimize the DFA fast path

Status: new
Priority: 3

The provisional end-to-end benchmark is 0.598 s median at one thread, versus
0.501 s for CPython `re`. Engine-only, the DFA is 0.360 s versus 0.169 s for
Rust `regex` bytes. These numbers predate the load-aware protocol; wait for
the baseline task before optimizing. The binding overhead is not the main gap;
the per-candidate DFA setup and transition lookup are. Optimize the DFA fast path and rerun both the
engine-only and end-to-end benchmarks. Acceptance: engine-only moves
substantially toward 0.169 s, end-to-end is reliably below 0.75 s, and ideally
approaches 0.55 s.

References: ADR-0010, ADR-0011, ADR-0012

## Set up continuous integration

Status: new
Priority: 3

Add CI that runs `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings`, `cargo test`, `python3 tools/python_diff.py`, the benchmark
self-test, and the Python binding smoke test, plus a docs check for local links
and referenced paths. Pin the toolchain to `rust-toolchain.toml` and keep the
CI commands identical to the local commands in `spec/docs/testing.md`.

## Milestone 2: Python classes, flags, captures

Status: new
Priority: 2

Add Python-equivalent `\w`, `\d`, and `\s` tables generated from CPython data,
Python word boundaries `\b` and `\B`, the Python `$` assertion, capture slots,
lazy repetition, named groups, and flags `i m s x`. Switch the full
differential suite to Python `re` on generated UCS-1/2/4 strings. Acceptance
criteria are in `spec/docs/milestones.md`.

References: ADR-0003, ADR-0007, ADR-0008

## Verify Python versions and free-threaded builds

Status: new
Priority: 2

The binding is verified only on CPython 3.13 GIL. Build and smoke-test on
CPython 3.10 through 3.15, including free-threaded 3.14t. Run the binding smoke
test and a 100k end-to-end benchmark on each. Record any ABI or GIL-detach
differences. Acceptance: a wheel and a passing smoke test for each supported
interpreter.

References: ADR-0007, ADR-0012
