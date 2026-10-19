## Report on task: Set up continuous integration

### Task (archived from plan.md)

## Set up continuous integration

Status: done

Add CI that runs `cargo fmt --check`, `cargo clippy --all-targets -- -D
warnings`, `cargo test`, `python3 tools/python_diff.py`, the benchmark
self-test, and the Python binding smoke test, plus a docs check for local links
and referenced paths. Pin the toolchain to `rust-toolchain.toml` and keep the
CI commands identical to the local commands in `spec/docs/testing.md`.

### Done

- Added `.github/workflows/ci.yml` with five Ubuntu jobs on `push` to `main`,
  `pull_request`, and `workflow_dispatch`, with superseded runs cancelled.
- `rust`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`, `cargo test --release`, `cargo check --features python`, and a
  `cargo publish --dry-run` of the crates.io placeholder manifest.
- `binding`: a matrix over CPython 3.10, 3.11, 3.12, 3.13, 3.14, and 3.15 that
  builds a wheel with maturin, installs it, and runs
  `tools/test_python_binding.py`.
- `python-diff`: `tools/python_diff.py --patterns 1000` on CPython 3.13.
- `bench`: the offline `--self-test` only.
- `docs`: `tools/check_docs.py`.
- Added `tools/check_docs.py`, standard library only, covering local Markdown
  links, catalogue coverage of `spec/docs/index.md`, the retired identifiers,
  report markers and filenames, plan record fields, and `Cargo.toml` versus
  `pyproject.toml` version agreement.
- Verified the tool actually fails: a missing link, an uncatalogued doc, a
  forbidden identifier, and an unknown marker each produced a specific error,
  and the tree returned to green after removing the probes.
- Ran every job's commands locally: format, clippy, debug and release tests, the
  Python feature check, the placeholder packaging dry run, the differential
  (40,414 cases, 0 mismatches), the benchmark self-test, the maturin wheel build
  with an install and a passing smoke test, and the docs check.
- Added the CI badge to `README.md`. Version-tag badges were deliberately not
  added: the published `0.0.1` packages are empty placeholders.
- Rewrote the `Continuous integration` section of `spec/docs/testing.md`, added
  the CI commands to `spec/docs/dev.md`, and removed this task from
  `spec/docs/plan.md`.

### First CI run

Run 39242540033 on `b2b1acd`: ten jobs, all successful, in about 1m15s.

- Rust: 11:09:08 to 11:10:23 locally-instrumented steps, with the Python
  feature check the slowest at 60 s.
- Binding legs: 26 s to 39 s each, six of six green, including CPython 3.10
  and 3.15, so the `pyo3 0.30` support range held in practice.
- Python differential, benchmark self-test, and docs: green.

This is the first evidence that the engine builds and passes its smoke test on
interpreters other than 3.13.

### Decisions taken by the owner

- Toolchain channel stays `stable`, so the workflow does not pin a Rust
  version. The task text asked for pinning; `testing.md` no longer claims it.
- Ubuntu only, no macOS or Windows jobs.
- Python 3.10 through 3.15, matching `requires-python`.
- Direct pushes to `main`, no branch protection, so CI is advisory.
- One combined docs tool rather than a separate task.

### Spec/ADR amendments

- [acted] `spec/docs/testing.md` no longer asserts that CI runs checks in the
  present tense; it describes the five real jobs, what CI deliberately omits,
  and the fact that the toolchain floats.
- [acted] `spec/docs/plan.md`: the "Verify Python versions and free-threaded
  builds" record now states that CI covers 3.10 through 3.15 GIL and that the
  free-threaded build, the per-interpreter 100k benchmark, and the ABI notes
  remain.
- [open] No ADR records the CI design: Ubuntu only, floating `stable`, advisory
  status. If those choices are meant to outlast the next few sessions, ADR-0017
  should record them and the alternatives.

### Future-task notes

- [open] Add the lock-contention test for `/tmp/core4.lock` to the `bench` job;
  it is still a manual check.
- [open] Add the synthetic report fixture for `tools/telegram_report.py` to CI
  so the renderer and the HTML twin are exercised.
- [open] The `binding` job builds `manylinux_2_34` wheels on the runner, which
  does not match a release build in the manylinux container; the release
  pipeline still needs its own wheel job.
- [open] `tools/test_python_binding.py` documents only the `PYTHONPATH=var/py`
  invocation; it also works from an installed wheel, which is how CI runs it.
  Mention both.

### Tooling/process

- [acted] `actions/cache` keyed on `Cargo.lock` and `rust-toolchain.toml`
  replaced a third-party Rust cache action, keeping the workflow dependency-free
  beyond the official actions.
- [acted] `cargo publish --dry-run` needs no token, so the placeholder manifest
  is verified on every push without giving CI any credential.
- [acted] Proving the docs checker fails on injected faults mattered: a checker
  that only ever passes is indistinguishable from one that checks nothing.
