# Open Tasks

Open-task list. Format, statuses, and lifecycle:
`spec/skills/task_tracking.md`. Priorities: higher number = more important;
`-1` = deferred.

## Optimize the DFA fast path

Status: new
Priority: 4

Remote `regexus`, one thread on the first four physical P-cores: end-to-end
`exp_rurex` is 0.296 s median versus 0.191 s for CPython `re`. Engine-only, the
DFA is 0.191 s versus 0.071 s for Rust `regex` bytes. The binding overhead is
not the main gap; the per-candidate DFA setup and transition lookup are.
Optimize the DFA fast path and rerun both the engine-only and end-to-end
benchmarks. Acceptance: engine-only moves substantially toward 0.071 s, and
end-to-end moves toward the same-machine CPython `re` time of 0.191 s.

References: ADR-0010, ADR-0011, ADR-0012, ADR-0013, ADR-0014

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
