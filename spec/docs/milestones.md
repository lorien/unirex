# Milestones

Each milestone is a plan task in `spec/docs/plan.md`. A milestone is complete
only when its acceptance criteria pass and its report is committed. The
order is deliberate: prove the matching model first, then add captures, then
optimize, then bind to Python.

## Milestone 1: UCS haystack and group-0 PikeVM

Build the standalone crate and the smallest engine that proves the model:

- `Ucs` haystack enum with `len`, `get`, `kind`, and character slicing.
- Pattern parsing with `regex-syntax`.
- HIR lowering to the Thompson instruction set for literals, classes,
  concatenation, alternation, and repetition.
- PikeVM over code points, group 0 only: `is_match`, `find`, `find_iter`.
- Differential tests against the Rust `regex` crate on UTF-8 text decoded to
  UCS-1, UCS-2, and UCS-4 code units, restricted to supported syntax and
  avoiding the documented divergences.

Acceptance: `cargo test` passes on all three kinds; the adversarial patterns
`(a+)+b` and `(x+x+)+y` terminate within a fixed budget; no UTF-8 conversion
exists in the matching path; the README does not claim linear time until the
adversarial tests pass.

## Milestone 2: Captures, anchors, and Python semantics

- Capture slots, leftmost-first priority, greedy and lazy repetition.
- Python-compatible zero-width iteration.
- Anchors, word boundaries, and flags `i m s x`.
- Differential tests against Python `re` on generated UCS-1, UCS-2, and UCS-4
  strings, restricted to supported syntax.
- Surrogate tests: lone surrogates match as units, not as encoded characters.

Acceptance: Python differential suite passes on all three kinds, including
empty matches, anchors, multiline, dot, alternation priority, unmatched
groups, and zero-width stepping. Every remaining divergence is documented in
`spec/docs/semantics.md`.

## Milestone 3: Prefilter and first benchmark

- Required-first-unit extraction from HIR literals and classes.
- A false-negative-free UCS scan before entering the engine.
- Benchmark the Telegram corpus in UCS-2 and a synthetic corpus when the real
  one is unavailable.
- Decide from the numbers whether the PikeVM is fast enough or a lazy DFA is
  needed.

Acceptance: no prefilter false negatives in fuzz tests; engine-only and
end-to-end numbers are reported separately; the decision for milestone 4 is
recorded in an ADR.

## Milestone 4: Optional lazy DFA

Only if milestone 3 says it is needed:

- Lazy determinization over code-point equivalence classes.
- Bounded DFA state cache with PikeVM fallback on overflow.
- DFA for match start and end; anchored PikeVM over the matched window for
  captures.

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
