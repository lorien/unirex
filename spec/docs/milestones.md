# Milestones

Each milestone is a plan task in `spec/docs/plan.md`. A milestone is complete
only when its acceptance criteria pass and its report is committed. The
order is deliberate: prove the matching model first, then add Python classes
and captures, then optimize, then bind to Python.

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
- Differential tests against CPython `re` for the accepted subset, UCS-1/2/4
  tests, lone-surrogate tests, and adversarial-time tests.

Acceptance: all tests pass; the randomized Python differential harness
reports no mismatches; `(a+)+b` and `(x+x+)+y` terminate within a fixed
budget; no UTF-8 conversion exists in the matching path; the README states
O(n * m) and does not claim more.

## Milestone 2: Python classes, flags, captures

- Generate Python-equivalent `\w`, `\d`, and `\s` ranges from CPython data for
  a documented Unicode version; use the word table for `\b` and `\B`.
- Add a Python `$` assertion: end of haystack or before a final newline.
- Add capture slots, leftmost-first priority, lazy repetition, and named
  groups.
- Add flags `i`, `m`, `s`, and `x`, including scoped flags. Implement Python
  `x` stripping as ASCII-only.
- Add differential tests against Python `re` on generated UCS-1, UCS-2, and
  UCS-4 strings, including Python capture values in unbounded loops.
- Decide whether `a{,2}`, `(?#...)`, and `\N{...}` belong in the supported
  dialect.

Acceptance: Python differential suite passes on all three kinds, including
empty matches, anchors, multiline, dot, alternation priority, captures,
unmatched groups, and zero-width stepping. Every remaining divergence is
documented in `spec/docs/semantics.md`.

## Milestone 3: Prefilter and benchmark

Status: implemented.

- Conservative first-unit extraction from the lowered program.
- False-negative-free candidate skipping for UCS-1, UCS-2, and UCS-4.
- Full-corpus benchmark using the actual CPython kind of each post, plus
  same-machine Rust `regex` bytes and str baselines and a `memchr` lower
  bound.

Results: 5.424 s before the prefilter, 1.266 s after at one thread, 0.413 s at
four threads. Rust `regex` bytes is 0.179 s. The prefilter is necessary but
not sufficient.

Acceptance: differential suites pass with no prefilter false negatives;
engine-only and end-to-end numbers are reported separately; ADR-0010 records
the DFA decision.

## Milestone 4: Lazy DFA

Triggered by the milestone 3 benchmark and ADR-0010:

- Lazy determinization over code-point equivalence classes.
- Bounded DFA state cache with PikeVM fallback on overflow.
- DFA for match start and end; anchored PikeVM over the matched window for
  captures.
- Keep the first-unit prefilter as the candidate selector.

Acceptance: cache cap and fallback are tested; memory stays bounded; matching
results are identical to the PikeVM on the differential suites; the DFA is
faster on the benchmark corpus.

## Milestone 5: PyO3 binding and end-to-end benchmark

- PyO3 and maturin module.
- Validated PEP 393 layout detection; safe slices for the core.
- Result strings built from the UCS buffer.
- GIL release for large haystacks.
- End-to-end benchmark against the current `rurex` `str` path.

Acceptance: works on CPython 3.10 through 3.15, GIL and free-threaded 3.14t;
no unvalidated field is read; end-to-end time beats the 0.75 s baseline and
ideally approaches 0.45 s; ASCII and bytes paths do not regress in the
integration prototype.
