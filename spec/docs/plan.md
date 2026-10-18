# Open Tasks

Open-task list. Format, statuses, and lifecycle:
`spec/skills/task_tracking.md`. Priorities: higher number = more important;
`-1` = deferred.

## Set up continuous integration

Status: new
Priority: 3

Add CI that runs `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings`, `cargo test`, `python3 tools/python_diff.py`, and the benchmark
self-test, plus a docs check for local links and referenced paths. Pin the
toolchain to `rust-toolchain.toml` and keep the CI commands identical to the
local commands in `spec/docs/testing.md`.

## Milestone 2: Python classes, flags, captures

Status: new
Priority: 2

Add Python-equivalent `\w`, `\d`, and `\s` tables generated from CPython data,
Python word boundaries `\b` and `\B`, the Python `$` assertion, capture slots,
lazy repetition, named groups, and flags `i m s x`. Switch the full
differential suite to Python `re` on generated UCS-1/2/4 strings. Acceptance
criteria are in `spec/docs/milestones.md`.

References: ADR-0003, ADR-0007, ADR-0008

## Milestone 5: PyO3 binding and end-to-end benchmark

Status: new
Priority: 4

The DFA benchmark is 0.363 s at one thread and 0.114 s at four threads. Rust
`regex` bytes is 0.152 s at one thread. Add the PyO3 and maturin binding for
the current supported subset with validated PEP 393 layout detection, result
construction from the UCS buffer, and GIL release for large haystacks.
Benchmark end to end against the current `rurex` str path. Support CPython
3.10 through 3.15, GIL and free-threaded 3.14t. Milestone 2 compatibility work
can follow or interleave; the Telegram pattern does not need it. Acceptance
criteria are in `spec/docs/milestones.md`.

References: ADR-0007
