# Development

## Prerequisites

- Rust stable, edition 2024, `rust-version = "1.85"` or newer. The installed
  toolchain is selected by `rust-toolchain.toml`.
- Cargo.
- Network access for the first Telegram benchmark download, unless the corpus
  is supplied with `--corpus` or `UNIREX_TG_DATA`.
- Python 3 is needed for the randomized differential harness and the Python
  binding/benchmark.
- PyO3 and maturin are needed for the Python binding.

## Dependencies

- Core: `regex-syntax` 0.8 for AST parsing, `memchr` and `bytemuck` for the
  first-unit prefilter.
- Optional: `pyo3` behind the `python` and `extension-module` features.
- Dev-only: `regex` for differential and benchmark baselines; `serde`,
  `serde_json`, `flate2`, `ureq`, `sha2`, and `libc` for the benchmark's
  affinity and CPU-time measurements.
- No Python dependency in the core crate without the Python feature.

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
    cargo run --release --example bench -- --mode ucs --limit 100000 --cpu-list 2 --warmup-seconds 1 --repeat 3 --min-efficiency 0.90
    cargo run --release --example bench -- --mode ucs --cpu-list 2 --warmup-seconds 2 --repeat 5 --min-efficiency 0.90
    cargo run --release --example bench -- --mode ucs --threads 4 --cpu-list 0,2,4,6 --warmup-seconds 2 --repeat 5
    cargo run --release --example bench -- --mode bytes --cpu-list 2 --warmup-seconds 2 --repeat 5
    cargo run --release --example bench -- --mode str --cpu-list 2 --warmup-seconds 2 --repeat 5

Python binding:

    python3 -m venv .venv
    . .venv/bin/activate
    pip install maturin
    maturin develop --release
    python3 tools/test_python_binding.py
    python3 tools/bench_python.py --engine unirex --cpu-list 2 --warmup-seconds 2 --repeat 5 --min-efficiency 0.90

Full Telegram matrix and report:

    cargo build --release --example bench
    cargo build --release --features extension-module
    mkdir -p var/py
    cp target/release/libunirex.so var/py/unirex.so
    python3 tools/telegram_benchmark.py --cpu-list 0,2,4,6 --repeat 5 --warmup-seconds 2
    python3 tools/telegram_report.py --results-dir var/bench-results/telegram

The first command writes `docs/telegram-benchmark.md`, the SVG charts under
`docs/img/`, and the gitignored `docs/telegram-benchmark.html`. Raw JSON stays
under `var/bench-results/telegram/`.

For a Cargo-only build, run `cargo build --release --features extension-module`
and put `target/release/libunirex.so` on `PYTHONPATH` as
`unirex.so`. The `pyproject.toml` uses maturin with the `extension-module`
feature and is non-abi3.

Benchmark runs pin workers, warm up, measure CPU time, take the
`/tmp/core4.lock` lease, and discard runs whose per-worker CPU/wall ratio falls
below `--min-efficiency`. If external load
prevents clean runs, the runner exits instead of reporting a contaminated
number. It never changes the CPU governor.

The benchmark downloads and SHA-256 verifies the corpus into
`var/bench-data/tg-1m.json.gz` on first use. Use `--corpus` or
`UNIREX_TG_DATA` for an offline copy. It does not need another checkout.

The Python harness builds and runs `examples/oracle.rs`, then compares
`find_iter` spans with CPython `re`. It accepts `--patterns N` and `--seed S`.

## Remote benchmarks on regexus

Use the `web` account and `/web/benchmark/unirex`. Sync with `target`,
`var`, `.git`, virtualenvs, `__pycache__`, and built modules excluded. Build
with `~/.cargo/bin/cargo`. The corpus must download and verify on `regexus`;
do not copy the local corpus as a fallback.

Use the first four physical P-cores, logical CPUs `0,2,4,6`; SMT siblings
`1,3,5,7` stay idle. Take the exclusive `/tmp/core4.lock` lease for the whole
matrix. The Rust and Python runners do this by default; a shell-level matrix
holder passes `--no-lock` to its children.

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
