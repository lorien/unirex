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
   supported grammar and compares every capture span, on UCS-1, UCS-2, and
   UCS-4, over all strings up to `--max-length` built from an alphabet that
   includes whitespace, a non-ASCII letter, an Arabic-Indic digit, and an
   emoji. It covers classes, boundaries, `$`, flags, lazy repetition, named
   groups, and nested groups. Run it after any prefilter, search-loop, or
   capture change: a prefilter false negative and a wrong group span both show
   up as a mismatch.
5. **DFA unit tests.** The DFA result must equal the PikeVM result on patterns
   with alternation priority, greedy repetition, optional prefixes,
   `^`, `\A`, `\Z`, impossible `Start`/`End` placements, UCS-2, and UCS-4.
   Class maps must partition the alphabet, and cache overflow must report a
   fallback rather than a wrong result.
6. **Rust `regex` differential tests.** A smaller common subset where Python
   and Rust agree; this is not the semantic authority.
7. **Linearity test.** `search_time_is_linear_in_haystack_length` runs only in
   release builds and asserts that four times the haystack costs less than eight
   times the time for `a+b`, `a+b$`, and `(a|aa)+b`, guarding the bound in
   ADR-0018.
8. **Adversarial-time tests.** `(a+)+b` and `(x+x+)+y` over 500 units must
   finish within a fixed budget.
9. **Python binding smoke test.** `tools/test_python_binding.py` compares
   `findall`, `find`, and `is_match` with CPython `re` on UCS-1, UCS-2, UCS-4,
   and lone-surrogate inputs. It also alternates two compiled patterns,
   exercises the GIL-detach threshold, and scans the same patterns from several
   threads, using eight workers when the interpreter reports the GIL as
   disabled. The smoke test compares `findall` on capture patterns too, so group
   tuples and unmatched-group empty strings are checked against `re`, and it
   prints the interpreter version and GIL state.
10. **Capture cost.** `capture_costs` is an ignored test that prints per-match
   cost for the backtracker and the PikeVM fallback across span lengths. It is
   the only way to see the fallback path, which is silent at runtime.
11. **Capture-cost diagnostic.** `tools/bench_captures.py` compares `findall`
   with and without capture groups on the same posts. It does not pin CPUs and
   is not a published number; it exists to show that extraction is charged per
   matched span rather than per haystack.
12. **End-to-end benchmark.** `tools/bench_python.py` loads the corpus
   untimed, compiles once, and times `findall` per post. It verifies the full
   mention counts. The full corpus is not part of ordinary CI.
13. **Report rendering.** `tools/telegram_report.py` reads a completed matrix
    from `var/bench-results/`, checks that every expected point and full-corpus
    count is present, and writes the tracked Markdown report, the SVG charts,
    and the gitignored HTML twin. It is not part of ordinary CI because the
    full matrix is too large; it is run after a matrix completes.
14. **CPython conformance suite.** `tests/cpython/vendored_test_re.py` is
   CPython 3.13.5's own `test_re.py`, vendored verbatim, run against `unirex`
   through a `re` shim. Every test that does not pass is listed in the
   `EXPECTED` ledger with a bucket and a reason, so the suite is a compatibility
   ledger: closing a gap means deleting its entry in the same commit. The shim
   is process-wide, so the file runs in its own pytest invocation, and
   `tests/re_guard.py` fails if a parity suite ever runs with a replaced `re`.
   Each supported release has its own vendored copy of the suite under
   `tests/cpython/<version>/` and its own ledger, because test names and
   behaviour differ between releases. CI runs the conformance suite on every
   matrix leg, free-threaded included; a leg whose ledger is missing generates
   one and reports it as a warning instead of failing silently.
   `tools/gen_conformance_ledger.py` regenerates a ledger for the running
   interpreter; it ignores the existing ledger while doing so, otherwise the
   skipped tests would hide their own failures.
15. **Planned follow-ups.** Python `\w`/`\d`/`\s` tables, `\b`/`\B`, `$`,
    flags, captures, and generated UCS-1/2/4 differential suites.

## Commands

Run from the repository root:

    cargo fmt --check
    cargo clippy --all-targets -- -D warnings
    cargo test
    cargo test --release
    python3 tools/python_diff.py --patterns 300 --max-length 3
    cargo run --release --example bench -- --self-test
    cargo run --release --example bench -- --mode ucs --limit 100000 --cpu-list 2 --warmup-seconds 1 --repeat 3 --min-efficiency 0.90
    cargo check --features python
    cargo build --release --features extension-module
    python3 tools/check_docs.py
    python3 tools/gen_unicode_tables.py --check
    cargo publish --dry-run --manifest-path packaging/crates-stub/Cargo.toml
    PYTHONPATH=var/py python3 tools/test_python_binding.py
    PYTHONPATH=var/py python3 tools/bench_python.py --engine unirex --limit 100000 --cpu-list 2 --warmup-seconds 1 --repeat 2 --min-efficiency 0.90
    PYTHONPATH=var/py python3 tools/bench_captures.py --limit 200000 --repeat 3
    cargo test --release --lib -- --ignored --nocapture capture_costs
    PYTHONPATH=var/py python3 -m pytest tests/re_guard.py -q
    PYTHONPATH=var/py python3 -m pytest tests/cpython/vendored_test_re.py -q
    python3 tools/telegram_report.py --results-dir var/bench-results/telegram

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

`.github/workflows/ci.yml` runs on every push to `main`, on pull requests, and
on demand. Five jobs, Ubuntu only, using the `rust-toolchain.toml` channel
`stable`:

1. **Rust.** `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
   `cargo test`, `cargo test --release`, `cargo check --features python`, and a
   `cargo publish --dry-run` of the crates.io placeholder manifest.
2. **Binding.** A matrix over CPython 3.10 through 3.15 plus the free-threaded
   3.14t and 3.15t builds: `maturin build
   --release`, install the wheel, run `tools/test_python_binding.py`.
3. **Python differential.** `tools/python_diff.py --patterns 300
   --max-length 3` on CPython 3.13, about 128,000 cases across all three
   storage kinds. It builds `examples/oracle.rs` itself.
4. **Benchmark self-test.** `cargo run --release --example bench --
   --self-test`, offline.
5. **Docs.** `python3 tools/check_docs.py`.

CI deliberately does not run the full 1M-post end-to-end benchmark, any timed
benchmark, or the report renderer: those need a quiet machine and take far
longer than a push does.

The toolchain is not pinned to a version. `rust-toolchain.toml` selects
`stable`, so a compiler release can change CI results without a repository
change; pin it when reproducibility starts to matter more than currency.

`tools/check_docs.py` is standard-library only and checks seven things: local
Markdown links resolve, every `spec/docs/*.md` file is catalogued in
`spec/docs/index.md`, the retired identifiers listed in the `Naming` rule of
`spec/docs/conventions.md` do not reappear outside reports that record their
removal, every report has a valid
`[open]`/`[acted]`/`[needs-decision]` marker and a `## Report on task:` first
line, every plan record has a `Status:` and `Priority:`, and the versions in
`Cargo.toml` and `pyproject.toml` agree.
