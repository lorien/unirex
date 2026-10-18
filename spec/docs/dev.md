# Development

## Prerequisites

- Rust stable, edition 2024, `rust-version = "1.85"` or newer. The installed
  toolchain is selected by `rust-toolchain.toml`.
- Cargo.
- Network access for the first Telegram benchmark download, unless the corpus
  is supplied with `--corpus` or `RUREX_TG_DATA`.
- Python 3 is needed only for the randomized differential harness, not for
  building, using, or benchmarking the crate.
- Python, PyO3, and maturin are needed only for milestone 5.

## Dependencies

- Core: `regex-syntax` 0.8 for AST parsing, `memchr` and `bytemuck` for the
  first-unit prefilter.
- Dev-only: `regex` for differential and benchmark baselines; `serde`,
  `serde_json`, `flate2`, `ureq`, and `sha2` for the benchmark.
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

Benchmark commands:

    cargo run --release --example bench -- --self-test
    cargo run --release --example bench -- --mode ucs --limit 100000 --repeat 3
    cargo run --release --example bench -- --mode ucs --repeat 3
    cargo run --release --example bench -- --mode bytes --repeat 3
    cargo run --release --example bench -- --mode bytes --repeat 3 --threads 4
    cargo run --release --example bench -- --mode str --repeat 3 --threads 4

The benchmark downloads and SHA-256 verifies the corpus into
`var/bench-data/tg-1m.json.gz` on first use. Use `--corpus` or
`RUREX_TG_DATA` for an offline copy. It does not need another checkout.

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
