# Testing and Checks

## Current state

Milestones 1, 3, 4, and 5 are implemented. The suite covers the strict Python
subset, the Python 3.7+ empty-match rule, all three UCS kinds, lone
surrogates, the unit-wide first-unit prefilter, the lazy DFA and its PikeVM
fallback, the PyO3 binding, adversarial patterns, and a randomized comparison
against CPython `re`.

## Test layers

1. **Compiler and adapter tests.** Accepted and rejected syntax with error
   positions, `\Z` normalization, Python numeric-escape rules, `$` and `\w`
   rejection, class set operations, nested classes, and Rust-only assertions.
2. **Unit tests.** `Ucs` access, kind preservation, slicing, lone surrogates,
   and the first-unit scan. Scan tests cover all three widths, targets above
   `0xFF`, `0xFFFF`, and `0x10FFFF`, short tails, and unaligned byte hits that
   must be discarded.
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
8. **Python binding smoke test.** `tools/test_python_binding.py` compares
   `findall`, `find`, and `is_match` with CPython `re` on UCS-1, UCS-2, UCS-4,
   and lone-surrogate inputs. It also alternates two compiled patterns,
   exercises the GIL-detach threshold, and scans the same patterns from several
   threads. Capture groups are group 0 only; the smoke test uses group-free
   patterns for `findall`.
9. **End-to-end benchmark.** `tools/bench_python.py` loads the corpus
   untimed, compiles once, and times `findall` per post. It verifies the full
   mention counts. The full corpus is not part of ordinary CI.
10. **Planned for milestone 2.** Python `\w`/`\d`/`\s` tables, `\b`/`\B`, `$`,
    flags, captures, and generated UCS-1/2/4 differential suites.

## Commands

Run from the repository root:

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo test --release
    python3 tools/python_diff.py --patterns 1000
    cargo run --release --example bench -- --self-test
    cargo run --release --example bench -- --mode ucs --limit 100000 --cpu-list 2 --warmup-seconds 1 --repeat 3 --min-efficiency 0.90
    cargo check --features python
    cargo build --release --features extension-module
    PYTHONPATH=var/py python3 tools/test_python_binding.py
    PYTHONPATH=var/py python3 tools/bench_python.py --engine exp_rurex --limit 100000 --cpu-list 2 --warmup-seconds 1 --repeat 2 --min-efficiency 0.90

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
6. **Core lease.** Two benchmark processes must not run concurrently on
   `/tmp/core4.lock`. One acquires; the other waits or fails after
   `--lock-wait`. A killed holder must release the lease. `--no-lock` is only
   for a child whose parent already holds it.
7. **Benchmark portability and load.** The benchmark must not reference an
   absolute host path or another checkout. Run `--self-test` after a fresh
   checkout or a copy outside the original source tree. A benchmark result is
   valid only if its mention count matches the independent naive count; the
   full corpus must report 334,136 mentions in 186,226 posts. For timed runs,
   use `--cpu-list`, `--warmup-seconds`, and `--min-efficiency`; a run that
   fails the efficiency check must be retried, not reported.

## Continuous integration

CI runs the same format, lint, test, Python differential, benchmark self-test,
and Python binding smoke test commands on every push. It does not run the full
1M-post end-to-end benchmark.
Tool versions are pinned in the workflow to the versions in
`rust-toolchain.toml`. A docs check verifies local links and referenced paths
so documentation cannot silently drift.
