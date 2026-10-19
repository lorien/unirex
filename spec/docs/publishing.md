# Publishing

Registry presence and release procedure for the `unirex` distribution and
crate. The Python distribution is `unirex` on PyPI; the Rust crate will be
`unirex` on crates.io.

## Current state

- PyPI: name reserved by `unirex 0.0.1`, a metadata-only placeholder that
  installs no modules. Uploaded 2026-10-19.
- crates.io: name not reserved yet. Publishing needs an account token that is
  not present on the development machine.

## Placeholder sources

The placeholder lives in `packaging/pypi-stub/` and is built from this
repository so the published artifacts are reproducible. It declares
`packages = []`, so `import unirex` fails with `ModuleNotFoundError` and the
placeholder can never shadow the real extension module.

Build and upload without touching the checkout:

    mkdir -p var/pypi-stub-dist
    cd packaging/pypi-stub
    python3 -c 'from setuptools import build_meta; print(build_meta.build_sdist("../../var/pypi-stub-dist")); print(build_meta.build_wheel("../../var/pypi-stub-dist"))'
    cd -
    python3 -m twine check var/pypi-stub-dist/*
    python3 -m twine upload --non-interactive --repository pypi var/pypi-stub-dist/*

`twine` is not a project dependency; use any environment that has it. Build
output stays under `var/`, which is gitignored.

## Version policy

- A placeholder must always be lower than the first real release. `0.0.1` is
  the placeholder; the first functional release is `0.1.0` or later.
- Registry versions are immutable. A published version can be yanked but never
  replaced or deleted, so never publish a version that a later release must
  reuse.
- Yank the placeholder on both registries once the first real release is
  published. Yanking hides it from resolution without breaking the name.

## Credentials

- Tokens live in `~/.pypirc` and in the crates.io credentials file, never in
  the repository and never in a command line that a report records.
- A PyPI upload of a **new** project needs an account-scoped token. A
  project-scoped token can only publish later versions of a project it already
  owns.
- `.env` stays gitignored. No token, hash, or machine-specific path is
  committed.

## Verification after a publish

1. `curl -s https://pypi.org/pypi/unirex/json` must report the new version and
   both artifacts.
2. Install the placeholder in a throwaway virtual environment and confirm that
   `import unirex` fails with `ModuleNotFoundError`; a placeholder that
   imports would shadow the future extension.
3. For the crate, `cargo info unirex@<version>` must show the published
   manifest and `cargo add unirex` must resolve to the same version.
