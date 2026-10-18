## Report on task: Milestone 3: Prefilter and prefixed benchmark

### Task (archived from plan.md)

## Milestone 3: Prefilter and prefixed benchmark

Status: done

Add a false-negative-free required-first-unit prefilter to the engine and
rerun the existing `examples/bench.rs` harness. The milestone 1 engine-only
baseline is 5.424 s for 1M posts at 1 thread, 1.796 s at 4 threads, versus
0.151 s for Rust `regex` bytes. About 77 percent of posts contain no `@`, so
the prefilter can skip most of the corpus. Measure the prefixed engine and
decide from the numbers whether the prefilter is sufficient or the lazy DFA
is required. Keep engine-only and eventual end-to-end numbers separate.
Protocol and corpus provenance: `spec/docs/benchmarks.md`.

References: ADR-0005, ADR-0009

### Done

- Added a conservative first-unit set to the compiled program. It follows
  epsilon transitions from the NFA start and is disabled when a match is
  reachable, a first class is negated, the set covers more than 1,024 code
  points, or it has more than 16 ranges.
- Added a `Unit` trait for UCS-1/2/4. A single candidate is searched with
  `memchr` over raw unit bytes; UCS-2/UCS-4 hits are checked against the full
  unit. Small range sets use a scalar scan.
- Changed the PikeVM loop: when no threads are active and no match is
  pending, it jumps to the next candidate instead of advancing one unit at a
  time. The unfiltered path remains for nullable, negated, or broad patterns.
- Added prefilter tests for optional prefixes, `a?b`, `a*b`, `(a|)b`,
  alternation, narrow classes, `^`, nullable fallback, UCS-2, and UCS-4.
- Reran the randomized Python differential harness: 40,414 cases, zero
  mismatches. No prefilter false negatives were found.
- Reran the full corpus and updated `benchmarks.md`, `README.md`,
  `architecture.md`, `milestones.md`, `testing.md`, `plan.md`, and `index.md`.
- Added ADR-0010, recording that the lazy DFA is now required and milestone 4
  is prioritized to 4.

### Results: full corpus, 1M posts, 334,136 mentions

Machine: Intel Core i5-8350U, 8 logical cores, Linux, Rust 1.97.1. Median of
three engine-only runs.

- Before prefilter, 1 thread: 5.424 s.
- Before prefilter, 4 threads: 1.796 s.
- With prefilter, 1 thread: 1.266 s.
- With prefilter, 4 threads: 0.413 s.
- Rust `regex` bytes, 1 thread: 0.179 s (0.15 s to 0.18 s across runs).
- Rust `regex` str, 1 thread: 0.215 s.

The prefilter is about 4.3 times faster than the unfiltered PikeVM. It is
still about 7 times slower than the one-thread bytes baseline. No same-thread
four-thread baseline was measured in that session; that was corrected in the
milestone 4 benchmark. The candidate scan is no longer the dominant cost; the
remaining time is the PikeVM work at each `@` candidate and its per-call
overhead. The lazy DFA is required.

The `memchr` `@` diagnostic was later removed from the benchmark: it was a
byte-search diagnostic, not a regex engine or baseline, and it counted `@`
marks rather than mentions.

### Spec/ADR amendments

- [acted] Milestone 4 must implement the lazy DFA with a bounded state cache,
  PikeVM fallback, and anchored PikeVM for captures. The DFA is implemented in
  `src/dfa.rs`; the full corpus improved to 0.363 s at one thread.
- [open] The first-unit set is disabled for negated and broad classes. A later
  optimization can represent complements and narrow broad classes; it is not
  needed for the Telegram pattern.
- [open] Range first sets use a scalar scan. If a real pattern has a narrow
  multi-unit first set, add a faster range scan.
- [open] The benchmark still reports one total time. Add per-kind timing to
  show whether UCS-4 posts dominate.
- [open] The full 1M-post benchmark is not suitable for ordinary CI. CI should
  run the self-test and a small corpus only.

### Future-task notes

- [acted] The DFA must preserve the existing Python 3.7+ empty-match iteration
  and leftmost-first behavior, and must fall back to the PikeVM on cache
  overflow. DFA and PikeVM agree on the targeted assertion/priority tests, and
  overflow falls back.
- [acted] Compare DFA results against both the PikeVM and CPython on the
  differential suites before trusting the benchmark. The randomized Python
  differential ran 40,414 cases with zero mismatches.
- [acted] Keep the prefilter and DFA candidate selection consistent; the DFA
  is anchored at each prefilter candidate.
- [open] Revisit the end-to-end target only after the DFA and the Python
  binding exist.

### Tooling/process

- [open] The `bytes` baseline varied from 0.15 s to 0.18 s across runs. Use
  more repeats or CPU pinning when comparing small differences.
- [open] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue; use the `rg` form from `testing.md`.
