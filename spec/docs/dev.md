# Development

## Prerequisites

- Rust stable, edition 2024, `rust-version = "1.85"` or newer. The installed
  toolchain is selected by `rust-toolchain.toml`.
- Cargo.
- Python 3 is needed only for the randomized differential harness, not for
  building or using the crate.
- Python, PyO3, and maturin are needed only for milestone 5.

## Dependencies

- `regex-syntax` 0.8 for AST parsing.
- `regex` 1.13 as a dev-dependency for the common-subset differential tests.
- No Python dependency in the core crate.

Add a dependency only with a reason recorded in the commit message or an ADR.
Prefer the standard library for anything small.

## Commands

    cargo build
    cargo test
    cargo test --release
    cargo clippy --all-targets -- -D warnings
    cargo fmt
    cargo fmt --check
    python3 tools/python_diff.py --patterns 1000

The Python harness builds and runs `examples/oracle.rs`, then compares
`find_iter` spans with CPython `re`. It accepts `--patterns N` and `--seed S`.

## Repository layout

The crate layout is described in `spec/docs/overview.md`; the component design
is in `spec/docs/architecture.md`. Keep files focused: one module per concern,
unit tests next to the code or under `tests/`.

## Working loop

The workflow is `spec/skills/work.md`. Pick the highest-priority `new` task
from `spec/docs/plan.md`, present the implementation plan to the owner, then
implement. At the end, run the checks in `spec/docs/testing.md`, write the
session report, remove the finished task from the plan, and commit.

## Local data

The Telegram corpus and other downloads live under `var/`, which is
gitignored. Secrets live in `.env`, also gitignored. Nothing machine-specific
is committed.

## No sudo

Do not run project commands with `sudo`. If a system tool is missing, ask the
owner to install it.
