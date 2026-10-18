## Report on task: Milestone 1: UCS haystack and group-0 PikeVM

### Task (archived from plan.md)

## Milestone 1: UCS haystack and group-0 PikeVM

Status: done

Build the standalone crate: `Ucs` haystack enum, `regex-syntax` parsing, HIR
lowering to the Thompson instruction set, and a PikeVM for `is_match`, `find`,
and `find_iter` with group 0 only. Add differential tests against the Rust
`regex` crate on decoded UCS-1/2/4 text and adversarial-time tests. Acceptance
criteria and the full scope are in `spec/docs/milestones.md`.

References: ADR-0002, ADR-0003, ADR-0004, ADR-0005, ADR-0007

### Done

- Created the crate with `Cargo.toml`, `Cargo.lock`, and
  `rust-toolchain.toml`; dependency `regex-syntax` 0.8, dev-dependency
  `regex` 1.13.
- Implemented `Ucs` for UCS-1/2/4 with `len`, `get`, `kind`, and slicing, plus
  unit tests for each width and for lone surrogates.
- Added the Python-dialect adapter over the `regex-syntax` AST: `\Z` to `\z`
  normalization, the Python backreference-versus-octal rule, the `0o377`
  octal cap, and positioned rejection of Rust-only or unimplemented syntax.
- Lowered the AST to a Thompson program with compile limits for pattern
  length, program size, repetition count, class count, and class ranges.
- Implemented the group-0 PikeVM with monomorphized UCS dispatch, a sparse
  visited set, leftmost-first thread priority, and the Python 3.7+ empty-match
  iteration rule.
- Added 4 unit tests and 12 integration tests: accepted and rejected syntax,
  Python-pinned spans, UCS-1/2/4 equivalence, lone surrogates, class edges,
  Rust `regex` common-subset differential, and adversarial timing.
- Ran a randomized differential against CPython `re` over 3,000 generated
  patterns and 121 haystacks (363,000 cases) with zero mismatches. Committed
  the reproducible harness as `tools/python_diff.py` with
  `examples/oracle.rs`.
- Wrote `README.md`, updated the spec and ADRs, and added ADR-0008 revising
  ADR-0002's parser choice from HIR to AST plus a Python adapter.
- Checks passed: `cargo fmt --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test`, and `cargo test --release`.

### Spec/ADR amendments

- [open] Milestone 2 must add Python `\w`, `\d`, and `\s` tables generated
  from CPython data for a documented Unicode version, plus Python `\b`/`\B`,
  `$`, flags, captures, lazy repetition, and named groups.
- [open] Decide whether `a{,2}` (Python 3.11 and later) and `(?#...)` belong
  in the supported dialect. They are currently rejected.
- [open] Pin Python capture values in unbounded loops during milestone 2. If
  the chosen priority scheme cannot reproduce Python, record a superseding
  ADR rather than silently diverging.
- [acted] Verify the Telegram corpus release URL and both SHA-256 values
  before milestone 3. Both checksums were verified; the benchmark enforces the
  gzip checksum on download and cache.
- [open] Decide whether ASCII/UNICODE flags are needed for the `str` path
  after milestone 2.

### Future-task notes

- [open] Extend `tools/python_diff.py` to generated UCS-2 and UCS-4
  haystacks, including lone surrogates, once milestone 2 adds the remaining
  Python classes.
- [open] Add a regression corpus from CPython's `test_re` and the
  rust-lang/regex `testdata` for the supported subset.
- [open] Milestone 5 must validate PEP 393 layout detection on CPython 3.10
  through 3.15, including free-threaded 3.14t, before trusting the fast path.

### Tooling/process

- [open] Add the CI task that runs format, clippy, tests, the Python
  differential harness, the benchmark self-test, and the docs link check.
- [open] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue noted in the bootstrap report; use the `rg` form
  from `testing.md` until the upstream workflow file is fixed.
