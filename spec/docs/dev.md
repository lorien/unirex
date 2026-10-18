# Development

## Prerequisites

- Rust stable, edition 2024, `rust-version = "1.85"` or newer. The installed
  toolchain is selected by `rust-toolchain.toml`.
- Cargo.
- No Python is required for milestones 1 through 4. Python, PyO3, and maturin
  are needed only for milestone 5.

## Dependencies

- `regex-syntax` 0.8 for parsing and HIR.
- `memchr` 2.8 for the prefilter.
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

Run the benchmark with:

    cargo run --release --bin bench -- --corpus var/bench-data/tg-1m.json.gz

The final benchmark entry point is defined by milestone 3. Until then the
command above is the intended shape, not a shipped interface.

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
