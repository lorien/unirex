# Open Tasks

Open-task list. Format, statuses, and lifecycle:
`spec/skills/task_tracking.md`. Priorities: higher number = more important;
`-1` = deferred.

## Milestone 1: UCS haystack and group-0 PikeVM

Status: new
Priority: 3

Build the standalone crate: `Ucs` haystack enum, `regex-syntax` parsing, HIR
lowering to the Thompson instruction set, and a PikeVM for `is_match`, `find`,
and `find_iter` with group 0 only. Add differential tests against the Rust
`regex` crate on decoded UCS-1/2/4 text and adversarial-time tests. Acceptance
criteria and the full scope are in `spec/docs/milestones.md`.

References: ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0007

## Set up continuous integration

Status: new
Priority: 3

Add CI that runs `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings`, and `cargo test`, plus a docs check for local links and referenced
paths. Pin the toolchain to `rust-toolchain.toml` and keep the CI command
identical to the local command in `spec/docs/testing.md`.

## Milestone 2: Captures, anchors, and Python semantics

Status: new
Priority: 2

Add capture slots, leftmost-first priority, lazy repetition, Python-compatible
zero-width iteration, anchors, word boundaries, and flags `i m s x`.
Differential tests move to Python `re` on generated UCS-1/2/4 strings.
Acceptance criteria are in `spec/docs/milestones.md`.

References: ADR-0003, ADR-0007

## Milestone 3: Prefilter and first benchmark

Status: new
Priority: 2

Implement a false-negative-free required-first-unit prefilter and benchmark
the Telegram corpus in UCS-2. Report engine-only and end-to-end numbers
separately. Decide from the numbers whether milestone 4 is needed. Corpus
provenance and success criteria are in `spec/docs/benchmarks.md`.

References: ADR-0005

## Milestone 4: Optional lazy DFA

Status: new
Priority: 1

Only if milestone 3 justifies it: add lazy determinization over code-point
equivalence classes, a bounded state cache, PikeVM fallback on overflow, and
anchored PikeVM for captures. Acceptance criteria are in
`spec/docs/milestones.md`.

References: ADR-0002, ADR-0005, ADR-0006

## Milestone 5: PyO3 binding and end-to-end benchmark

Status: new
Priority: 1

Add the PyO3 and maturin binding with validated PEP 393 layout detection,
result construction from the UCS buffer, and GIL release for large haystacks.
Benchmark end to end against the current `rurex` str path. Support CPython
3.10 through 3.15, GIL and free-threaded 3.14t.

References: ADR-0007
