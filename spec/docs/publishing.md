# Publishing

Registry presence and release procedure for the `unirex` distribution and
crate. The Python distribution is `unirex` on PyPI; the Rust crate is
`unirex` on crates.io.

## Current state

- PyPI: name reserved by `unirex 0.0.1`, a metadata-only placeholder that
  installs no modules. Uploaded 2026-10-19.
- crates.io: name reserved by `unirex 0.0.1`, an empty placeholder crate that
  exposes no items. Published 2026-10-19.
- Both placeholders sit below the first functional release, `0.1.0` or later.

## Toolchain

Release tooling is not a project dependency. Keep it in one disposable
virtual environment, because the system `twine` install is broken on the
development machine:

    python3 -m venv var/venv-packaging
    var/venv-packaging/bin/pip install build maturin twine

Validated versions: `build` 1.6.1, `maturin` 1.15.0, `twine` 7.0.0.

## Placeholder sources

The placeholder lives in `packaging/pypi-stub/` and is built from this
repository so the published artifacts are reproducible. It declares
`packages = []`, so `import unirex` fails with `ModuleNotFoundError` and the
placeholder can never shadow the real extension module.

Build and upload with the `build` frontend, never by calling
`setuptools.build_meta` directly. The frontend creates an isolated environment
from `[build-system] requires`, which is what validates the pin; a direct call
silently uses whatever setuptools is installed and produces different
artifacts. The recorded `0.0.1` upload was built both ways to check this: the
wheel contents were identical except `WHEEL` and `RECORD`, because the
frontend resolved setuptools 84.0.0 while the system install was 78.1.1.

    var/venv-packaging/bin/python -m build --outdir var/pypi-stub-dist packaging/pypi-stub
    var/venv-packaging/bin/twine check var/pypi-stub-dist/*
    var/venv-packaging/bin/twine upload --non-interactive --repository pypi var/pypi-stub-dist/*

Build output stays under `var/`, which is gitignored.

## Functional wheels

`maturin` builds the real extension from the repository root. The extension is
non-abi3, so every CPython minor version and platform needs its own wheel:

    var/venv-packaging/bin/maturin build --release --out var/maturin-dist

A release wheel was verified this way on CPython 3.13: it installs into a clean
virtual environment and answers `findall` correctly. Upload a real release with
`maturin publish` or by passing the built wheels to `twine upload`.

## Placeholder crate

The crate placeholder lives in `packaging/crates-stub/`. It declares
`unirex 0.0.1`, edition 2021 (so it does not demand Rust 1.85), no
dependencies, and no items in `src/lib.rs`. It must never gain a dependency or
an API: dependents would inherit both.

Commit the sources before publishing. `cargo publish` refuses a dirty tree and
`--allow-dirty` hides that the published archive then omits version-control
information:

    git commit -am "..."
    cd packaging/crates-stub
    cargo package --list
    cargo publish --dry-run
    cargo publish

The dry run needs no token and no upload, so it validates packaging and
verification before the irreversible step. Check the name is still free
immediately beforehand:

    curl -s -H 'User-Agent: unirex-check' https://crates.io/api/v1/crates/unirex

After the first functional release, yank the placeholder:

    cargo yank --version 0.0.1

## Version policy

- A placeholder must always be lower than the first real release. `0.0.1` is
  the placeholder; the first functional release is `0.1.0` or later.
- Registry versions are immutable. A published version can be yanked but never
  replaced or deleted, so never publish a version that a later release must
  reuse.
- Yank the placeholder on both registries once the first real release is
  published. Yanking hides it from resolution without breaking the name.

## Credentials

- Tokens live in `~/.pypirc` and in `~/.cargo/credentials.toml`, never in the
  repository and never in a command line that a report records.
- A PyPI upload of a **new** project needs an account-scoped token. A
  project-scoped token can only publish later versions of a project it already
  owns.
- crates.io rejects every publish, including a first one, until the account
  email address is verified. The rejection is `400 Bad Request` with a
  verification message and arrives after authentication, so it is an account
  state problem, not a token problem.
- crates.io has no API endpoint to validate a token ahead of time;
  `GET /api/v1/me` is web-only and answers 403 for any token. The first real
  signal is `cargo publish --dry-run` followed by the upload itself.
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
4. `https://docs.rs/unirex` builds automatically after a publish. It is not
   available immediately; check it a few minutes later rather than treating an
   early 404 as a failure.
