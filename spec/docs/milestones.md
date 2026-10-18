# Milestones

Each milestone is a plan task in `spec/docs/plan.md`. A milestone is complete
only when its acceptance criteria pass and its report is committed. The order
is deliberate: prove the matching model, add Python compatibility, optimize,
then bind to Python.

## Milestone 1: UCS haystack, Python subset, group-0 PikeVM

Status: implemented.

- `Ucs` haystack enum with `len`, `get`, `kind`, and character slicing.
- Python-dialect adapter over the `regex-syntax` AST: `\Z` normalization,
  Python numeric-escape rules, strict subset validation, positioned errors.
- Lowering to a Thompson program for literals, explicit classes,
  concatenation, alternation, and greedy repetition.
- PikeVM over code points, group 0 only: `is_match`, `find`, `find_iter`.
- Python 3.7+ empty-match iteration.
- Compile limits for pattern length, program size, repetition count, class
  count, and class ranges.

Acceptance: all tests pass; the randomized Python differential harness reports
no mismatches; adversarial patterns terminate within a fixed budget; no UTF-8
conversion exists in the matching path.

## Milestone 2: Python classes, flags, captures

Status: planned.

- Python-equivalent `\w`, `\d`, and `\s` tables generated from CPython data for
  a documented Unicode version; use the word table for `\b` and `\B`.
- A Python `$` assertion: end of haystack or before a final newline.
- Capture slots, leftmost-first priority, lazy repetition, and named groups.
- Flags `i`, `m`, `s`, and `x`, including scoped flags. Python `x` stripping
  is ASCII-only.
- Differential tests against Python `re` on generated UCS-1, UCS-2, and UCS-4
  strings, including capture values in unbounded loops.
- Decide whether `a{,2}`, `(?#...)`, and `\N{...}` belong in the dialect.

Acceptance: Python differential suite passes on all three kinds, including
empty matches, anchors, multiline, dot, alternation priority, captures,
unmatched groups, and zero-width stepping. Every remaining divergence is
documented in `spec/docs/semantics.md`.

## Milestone 3: Prefilter and benchmark

Status: implemented.

- Conservative first-unit extraction from the lowered program.
- False-negative-free candidate skipping for UCS-1, UCS-2, and UCS-4.
- Full-corpus benchmark using the actual CPython kind of each post, plus
  same-machine Rust `regex` bytes and str baselines.

Results: 5.424 s before the prefilter, 1.266 s after at one thread, 0.413 s at
four threads. The prefilter is necessary but not sufficient.

Acceptance: differential suites pass with no prefilter false negatives;
engine-only and end-to-end numbers are reported separately; ADR-0010 records
the DFA decision.

## Milestone 4: Lazy DFA

Status: implemented.

- Lazy determinization over code-point equivalence classes with per-kind class
  maps.
- Leftmost-first match tracking by pruning states after the first `Match`.
- `Start` resolved at position zero; `End` kept pending until the final
  position.
- Bounded state cache in `Scratch` with PikeVM fallback on overflow.
- DFA used when a safe first-unit set exists; the prefilter selects candidates.
  Other patterns remain correct through the PikeVM.

Results: 1.266 s to 0.360 s at one thread, and 0.413 s to 0.110 s at four
threads. The Rust `regex` bytes baseline is 0.169 s at one thread and 0.061 s
at four threads. The DFA does not beat the same-thread baseline.

Acceptance: cache cap and fallback are tested; DFA results match the PikeVM on
the differential and assertion suites; the DFA is faster on the full corpus.

## Milestone 5: PyO3 binding and end-to-end benchmark

Status: next.

- PyO3 and maturin module for the current supported subset.
- Validated PEP 393 layout detection; safe slices for the core.
- Result strings built from the UCS buffer.
- GIL release for large haystacks.
- End-to-end benchmark against the current `rurex` str path.

Acceptance: works on CPython 3.10 through 3.15, GIL and free-threaded 3.14t;
no unvalidated field is read; end-to-end time beats the 0.75 s baseline and
ideally approaches 0.45 s. Milestone 2 compatibility work can follow or
interleave; the Telegram pattern does not need it.
