# Testing and Checks

## Test layers

1. **Compiler unit tests.** Every supported HIR variant, repetition
   expansion, class compilation, flag handling, and error case. Errors carry
   the pattern position when available.
2. **PikeVM unit tests.** Empty pattern, empty matches, anchors, multiline,
   dot, greedy and lazy repetition, alternation priority, captures later,
   unmatched groups, and zero-width stepping.
3. **Differential tests, milestone 1.** Compare against the Rust `regex`
   crate on UTF-8 text decoded to UCS-1, UCS-2, and UCS-4 code units, using
   patterns inside the supported subset and avoiding the divergences in
   `spec/docs/semantics.md`.
4. **Differential tests, milestone 2.** Compare against Python `re` on
   generated UCS-1, UCS-2, and UCS-4 strings and a curated pattern corpus.
   Draw from CPython's `test_re` and the rust-lang/regex `testdata` cases
   where the syntax is supported.
5. **Fuzz tests.** Random patterns and haystacks. The engine must never report
   a match where the reference has none, or the reverse, for the supported
   subset. Fuzz targets live with the crate and have a time budget.
6. **Adversarial-time tests.** `(a+)+b`, `(x+x+)+y`, large bounded
   repetitions, and huge character classes must finish within a fixed budget.
7. **Surrogate tests.** Lone UCS-2 surrogates are matchable as units, are not
   paired, and do not raise an encoding error.
8. **Memory tests.** DFA cache cap, fallback on overflow, and no unbounded
   per-call allocation.

## Commands

Run from the repository root:

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo test --release

Timing assertions use release builds. A debug-build timing test is not
meaningful and must not gate CI.

## Per-session final checks

Before every commit:

1. **Tests and checks.** Run the commands above and fix anything they
   surface.
2. **No hard-coded credentials.** Inspect the staged diff; tokens and paths
   belong in the environment or `.env`, never in source.
3. **Docs consistency.** Every path, command, and link in `spec/docs/` exists.
   Update stale documents in the same commit as the behavior change.
4. **Marker reconciliation.** Review the still-open report items:

       rg -n '\[(open|needs-decision)\]' spec/report/

   Mark `[acted]` every item this work resolved, with a one-line note, in the
   same commit. The remaining set must contain only items that are genuinely
   still open or undecided.
5. **No stray files.** `git status --short` shows only intended changes.
   Corpus data, build output, and local artifacts stay under `var/` or in
   `.env`, both gitignored.

## Continuous integration

CI runs the same format, lint, and test commands on every push. Tool versions
are pinned in the workflow to the versions in `rust-toolchain.toml`. A docs
check verifies local links and referenced paths so documentation cannot
silently drift.
