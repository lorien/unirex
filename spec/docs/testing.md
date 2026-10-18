# Testing and Checks

## Current state

Milestones 1, 3, and 4 are implemented. The suite covers the strict Python
subset, the Python 3.7+ empty-match rule, all three UCS kinds, lone
surrogates, the first-unit prefilter, the lazy DFA and its PikeVM fallback,
adversarial patterns, and a randomized comparison against CPython `re`.

## Test layers

1. **Compiler and adapter tests.** Accepted and rejected syntax with error
   positions, `\Z` normalization, Python numeric-escape rules, `$` and `\w`
   rejection, class set operations, nested classes, and Rust-only assertions.
2. **Unit tests.** `Ucs` access, kind preservation, slicing, and lone
   surrogates.
3. **Matching tests.** Literals, `.`, explicit classes, alternation, greedy
   repetition, anchors, grouping, class edge cases, and first-unit prefilter
   cases: optional prefixes, alternation, narrow classes, anchors, and
   nullable patterns that must fall back to the unfiltered path.
4. **Python differential tests.** `tests/m1.rs` pins spans generated with
   CPython `re`. `tools/python_diff.py` generates random patterns from the
   supported grammar and compares `find_iter` spans on all strings of length
   0 through 4 over `{a,b,c}`. Run it after any prefilter or search-loop
   change: a prefilter false negative shows up as a missing match.
5. **DFA unit tests.** The DFA result must equal the PikeVM result on patterns
   with alternation priority, greedy repetition, optional prefixes,
   `^`, `\A`, `\Z`, impossible `Start`/`End` placements, UCS-2, and UCS-4.
   Class maps must partition the alphabet, and cache overflow must report a
   fallback rather than a wrong result.
6. **Rust `regex` differential tests.** A smaller common subset where Python
   and Rust agree; this is not the semantic authority.
7. **Adversarial-time tests.** `(a+)+b` and `(x+x+)+y` over 500 units must
   finish within a fixed budget.
8. **Planned for milestone 2.** Python `\w`/`\d`/`\s` tables, `\b`/`\B`, `$`,
   flags, captures, and generated UCS-1/2/4 differential suites.

## Commands

Run from the repository root:

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo test --release
    python3 tools/python_diff.py --patterns 1000
    cargo run --release --example bench -- --self-test

Timing assertions use release builds. A debug-build timing test is not
meaningful and must not gate CI. The Python harness needs a `python3` with
`re`; it does not link Python into the crate.

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
   Corpus data, build output, local artifacts, and temporary examples stay
   under `var/` or in `.env`, both gitignored.
6. **Benchmark portability.** The benchmark must not reference a path outside
   this repository. Check with:

       rg -n "/web/|/home/" --glob '!target/**' .

   Run `--self-test` after a fresh checkout or a copy outside `/web`. A
   benchmark result is valid only if its mention count matches the independent
   naive count; the full corpus must report 334,136 mentions in 186,226 posts.

## Continuous integration

CI runs the same format, lint, test, Python differential, and benchmark
self-test commands on every push. It does not run the full 1M-post benchmark.
Tool versions are pinned in the workflow to the versions in
`rust-toolchain.toml`. A docs check verifies local links and referenced paths
so documentation cannot silently drift.
