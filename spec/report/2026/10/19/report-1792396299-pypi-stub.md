## Report on task: Reserve the unirex name on PyPI

### Done

- Published `unirex 0.0.1` to PyPI as a metadata-only placeholder: an sdist and
  a `py3-none-any` wheel containing only the README and the `dist-info`
  metadata, with `packages = []` so the distribution installs no modules.
- Kept the sources in `packaging/pypi-stub/` (`pyproject.toml`, `README.md`)
  so the published artifacts are reproducible from this repository.
- Verified availability first: `https://pypi.org/pypi/unirex/json` returned
  404 before the upload, and `rurex` was already taken (200), so only
  `unirex` was reserved.
- Verified after the upload: the JSON API reports `0.0.1` with both artifacts,
  and an install in a throwaway virtual environment raises
  `ModuleNotFoundError: No module named 'unirex'`, so the placeholder cannot
  shadow the future extension.
- Added `spec/docs/publishing.md` (placeholder sources, build and upload
  commands, version policy, credential rules, post-publish verification) and
  catalogued it in `spec/docs/index.md`.
- Added `.gitignore` entries for `packaging/*/build/` and
  `packaging/*/*.egg-info/`.

### Artifacts

- `unirex-0.0.1-py3-none-any.whl`, 1509 bytes, SHA-256
  `edfb27d7f916fc9fb843b68a627301964b833676e6c051bbbcce83ac98ca7ddc`.
- `unirex-0.0.1.tar.gz`, 1429 bytes, SHA-256
  `9f50a524dd5a4eafc890281db9e52c68f3ff606ef16e2d8802f7cb0914bafd60`.
- Live at `https://pypi.org/project/unirex/0.0.1/`.

### Spec/ADR amendments

- [acted] Added `spec/docs/publishing.md` and catalogued it in
  `spec/docs/index.md`; registry procedure no longer lives only in a report.
- [open] `spec/docs/overview.md` does not mention the reserved registry names.
  Decide whether the project overview should state them.

### Future-task notes

- [open] Reserve `unirex` on crates.io. It needs an account token on the
  development machine; publish a minimal `0.0.1` crate from
  `packaging/` below the real `0.1.0`.
- [open] Yank `unirex 0.0.1` on PyPI after the first functional release ships.
  It is lower than `0.1.0`, so resolvers already prefer the real release, but
  yanking keeps the project page from advertising an empty package.
- [open] The placeholder has no `project.urls` because the repository has no
  git remote. Add `Repository`/`Homepage` once a remote exists; metadata is
  editable without a new release.
- [open] Future functional releases need real wheels. `maturin` is not
  installed on the development machine, and the extension is non-abi3, so a
  release needs one wheel per CPython version and platform.

### Tooling/process

- [acted] `build` and `maturin` are absent, so the placeholder is built by
  calling `setuptools.build_meta.build_sdist` and `build_wheel` directly. This
  avoids installing a build frontend for a metadata-only distribution.
- [acted] The system `twine` at `/usr/local/bin/twine` is broken: its shebang
  points at `/usr/bin/python3`, where the `twine` package is not installed, so
  `python3 -m twine` fails and the script raises `ImportError`. A virtual
  environment under `var/venv-twine` with `twine 7.0.0` worked. Either repair
  the system install or record the virtual environment as the supported path.
- [acted] Build output went to `var/pypi-stub-dist/` rather than a `dist/`
  directory inside the checkout, so nothing generated entered the tree.
