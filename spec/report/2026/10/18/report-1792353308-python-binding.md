## Report on task: Milestone 5: PyO3 binding and end-to-end benchmark

### Task (archived from plan.md)

## Milestone 5: PyO3 binding and end-to-end benchmark

Status: done

The DFA benchmark is 0.360 s at one thread and 0.110 s at four threads. Rust
`regex` bytes is 0.169 s at one thread and 0.061 s at four threads, so the DFA
does not beat the same-thread baseline yet. Add the PyO3 and maturin binding for
the current supported subset with validated PEP 393 layout detection, result
construction from the UCS buffer, and GIL release for large haystacks.
Benchmark end to end against CPython `re`. The original task named an external
`rurex` baseline; it was removed from the reproducible benchmark. Support
CPython 3.10 through 3.15, GIL and free-threaded 3.14t. Milestone 2 compatibility work
can follow or interleave; the Telegram pattern does not need it. Acceptance
criteria are in `spec/docs/milestones.md`.

References: ADR-0007

### Done

- Added `src/python.rs` with `unirex.compile` and `unirex.Regex`
  (`findall`, `find`, `is_match`).
- UCS access uses CPython's `PyUnicode_KIND`, `PyUnicode_DATA`, and
  `PyUnicode_GET_LENGTH`; no private PEP 393 offsets. `PyUnicode_READY` is
  called first for pre-3.12 interpreters.
- Result strings come from `PyUnicode_Substring`; no UTF-8 conversion.
- Added a per-`Regex` scratch pool. Calls take a scratch, scan without the
  pool lock, and return it.
- Short strings match under the GIL. Haystacks of at least 4,096 code units
  detach the GIL for the scan and build strings afterward.
- Added `pyproject.toml` with maturin and the `extension-module` feature, plus
  Cargo-only build instructions.
- Added `tools/bench_python.py` (end-to-end Telegram benchmark) and
  `tools/test_python_binding.py` (smoke test against CPython `re`).
- Added ADR-0012 and updated `architecture.md`, `benchmarks.md`,
  `milestones.md`, `testing.md`, `conventions.md`, `dev.md`, `overview.md`,
  `README.md`, `index.md`, and `plan.md`.
- Built a maturin wheel for CPython 3.13 and verified it in a virtualenv.
  Built the Cargo extension and verified the same module through `PYTHONPATH`.

### Results: end-to-end, full corpus, 1 thread, GIL

Cold fresh process per point, pinned to one CPU, five runs each. Loading and
pattern compilation are outside the timed window.

- `unirex`: 0.596 s, 0.597 s, 0.598 s, 0.599 s, 0.604 s. Median 0.598 s,
  minimum 0.596 s.
- CPython `re`: 0.490 s, 0.491 s, 0.501 s, 0.510 s, 0.648 s. Median 0.501 s,
  minimum 0.490 s.

The binding is correct and counts 334,136 mentions in 186,226 posts, but it is
about 1.2 times slower than CPython `re`. The 0.75 s baseline is met, but the
ideal 0.45 s target is not.

Engine-only, the DFA is 0.360 s versus 0.169 s for Rust `regex` bytes. The
binding overhead is not the main gap; per-candidate DFA work is. GIL 4-thread
runs were not meaningfully faster. Free-threaded CPython was not tested.

### Spec/ADR amendments

- [acted] Optimize the DFA fast path. The unit-wide prefilter and DFA
  hot-loop changes closed the engine-only gap and made the one-thread
  end-to-end result slightly faster than CPython `re`.
- [open] Verify CPython 3.10 through 3.15 and free-threaded 3.14t. Only 3.13
  GIL was tested. This is a separate plan task.
- [open] The binding exposes group 0 only. Patterns with capture groups return
  the whole match, not `re.findall` group tuples. Document or fix when
  milestone 2 lands.
- [open] The extension is non-abi3 because the UCS accessors are outside the
  limited API. Add a limited-API fallback only if one wheel per version becomes
  a problem.
- [acted] GIL-detach behavior for long haystacks is implemented but not
  covered by the smoke test beyond short strings; add a long-string test.
  The binding smoke test now crosses the detach threshold.

### Future-task notes

- [open] Milestone 2: Python `\w`/`\d`/`\s`, `\b`, `$`, flags, captures, lazy
  repetition, named groups.
- [open] Report per-kind end-to-end time and per-kind engine time.
- [open] Keep the Python binding smoke test in CI; do not run the full 1M-post
  end-to-end benchmark in ordinary CI.

### Tooling/process

- [acted] Remove `rurex` from the reproducible benchmark. The harness now
  compares only `unirex` and CPython `re`; an external comparison, if
  wanted, must be produced separately.
- [acted] Build and test through maturin. A CPython 3.13 wheel built and loaded
  successfully in a fresh virtualenv.
- [acted] Keep a Cargo-only path for environments without maturin:
  `cargo build --release --features extension-module`, then place
  `libunirex.so` on `PYTHONPATH` as `unirex.so`.
- [acted] End-to-end timings varied with system load. The Python runner now
  supports CPU pinning, warmups, thread CPU time, load/frequency reporting,
  and efficiency-based retries.
- [open] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue; use the `rg` form from `testing.md`.
