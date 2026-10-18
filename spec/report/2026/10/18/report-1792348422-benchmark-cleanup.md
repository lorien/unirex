## Report on task: Benchmark comparison cleanup

### Done

- Removed the `at`/`memchr` diagnostic mode from `examples/bench.rs`. It was
  not a regex engine or a baseline: it counted `@` bytes, not mentions, and it
  had no meaningful same-thread comparison.
- Removed the `at_marks` report field, the `expected=0` output, the `--mode
  at` command, and every `memchr` result line from the current docs.
- Reran the benchmark at the same thread counts:
  - One thread: DFA 0.360 s, Rust `regex` bytes 0.169 s, Rust `regex` str
    0.185 s.
  - Four threads: DFA 0.110 s, Rust `regex` bytes 0.061 s, Rust `regex` str
    0.061 s.
- Removed the invalid cross-thread comparison from the README, benchmark
  docs, and DFA report. The DFA does not beat the same-thread baseline at one
  or four threads.
- Corrected stale status:
  - README now says milestones 1, 3, and 4 are implemented.
  - `architecture.md`, `testing.md`, and `overview.md` no longer describe the
    prefilter, DFA, or benchmark as future work.
  - `dev.md` lists `memchr` and `bytemuck` as core dependencies, not dev-only.
  - `semantics.md` describes the current subset instead of calling it the
    milestone 1 subset.
- Updated the DFA, prefilter, and self-contained-benchmark reports to remove
  the `at` diagnostic and the cross-thread claims. ADR-0009 notes that the
  diagnostic was removed.

### Results: same-thread comparison

Machine: Intel Core i5-8350U, 8 logical cores, Linux, Rust 1.97.1. Full
corpus, median of three engine-only runs.

- One thread: DFA 0.360 s, bytes 0.169 s, str 0.185 s.
- Four threads: DFA 0.110 s, bytes 0.061 s, str 0.061 s.

The DFA is about 2.1 times slower than bytes at one thread and 1.8 times
slower at four threads. It is about 3.5 times faster than the prefiltered
PikeVM. The remaining gap is in per-candidate DFA setup and transition lookup.

### Spec/ADR amendments

- [open] The DFA still does not beat the same-thread bytes baseline. Decide
  whether to optimize transitions further or proceed to the Python binding
  and judge the end-to-end result first.
- [open] The benchmark still reports one total time. Add per-kind timing to
  show whether UCS-4 posts dominate.
- [open] The full 1M-post benchmark remains unsuitable for ordinary CI.
- [open] The docs still mention a synthetic-corpus fallback that the harness
  does not implement.

### Future-task notes

- [open] Milestone 5: PyO3 binding and the end-to-end comparison against
  `rurex` str.
- [open] Keep engine-only and end-to-end numbers separate. The 0.75 s target
  includes Python overhead and result construction.
- [open] Revisit the transition cache only if the binding shows the engine is
  the limiting factor.

### Tooling/process

- [acted] Remove the `at`/`memchr` diagnostic. It confused the results: it was
  a single-thread byte-search diagnostic, not a thread or a regex baseline.
- [acted] Compare only same-thread results. Cross-thread claims were removed
  from all current docs and reports.
- [open] `bytes` timing still varies between 0.12 s and 0.18 s at one thread.
  Use more repeats or CPU pinning for small comparisons.
- [open] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue; use the `rg` form from `testing.md`.
