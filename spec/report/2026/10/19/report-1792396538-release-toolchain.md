## Report on task: Build release artifacts with the proper toolchain

### Done

- Replaced ad-hoc backend invocation with a disposable release environment:
  `python3 -m venv var/venv-packaging` and `pip install build maturin twine`,
  which resolved `build` 1.6.1, `maturin` 1.15.0, and `twine` 7.0.0.
- Rebuilt the placeholder with the frontend,
  `python3 -m build --outdir var/pypi-stub-dist2 packaging/pypi-stub`, and
  compared it against the uploaded `0.0.1` artifacts.
- Confirmed the isolated build resolves its own setuptools: the frontend used
  84.0.0, the system install is 78.1.1. Every wheel entry is byte-identical
  except `WHEEL` and `RECORD`, which only record the generator version. The
  published `0.0.1` artifacts are therefore correct; nothing needs
  re-uploading, and PyPI would reject a duplicate filename anyway.
- Built a real extension wheel for the first time:
  `maturin build --release --out var/maturin-dist` produced
  `unirex-0.1.0-cp313-cp313-manylinux_2_34_x86_64.whl`, 372,052 bytes,
  SHA-256 `dd4c4bf6093f6b5a80c89db6e94a993b1f38562ee7ed9514353dd7dcf9b6d916`.
- Installed that wheel into a clean virtual environment and exercised it:
  `unirex.__version__` is `0.1.0`, `Regex("[a-z]+").findall("abc 12 def")`
  returns `['abc', 'def']`.
- Rewrote `spec/docs/publishing.md`: a `Toolchain` section with the validated
  versions, the frontend-only build path with the reason, and a `Functional
  wheels` section recording the non-abi3 wheel-per-CPython-version rule and
  the verified `maturin` command.
- Marked the corresponding items in the PyPI placeholder report `[acted]`.

### Spec/ADR amendments

- [acted] `spec/docs/publishing.md` now mandates the `build` frontend and
  explains why a direct `setuptools.build_meta` call is wrong: it bypasses
  build isolation and silently uses the host setuptools.
- [open] `spec/docs/testing.md` and `spec/docs/dev.md` still describe the
  Python binding as a `maturin develop` workflow only. Add the release-wheel
  check once CI exists.

### Future-task notes

- [open] Nothing verifies a built wheel automatically. The clean-install check
  in `spec/docs/publishing.md` is manual; it belongs in CI next to the binding
  smoke test.
- [open] The wheel is `manylinux_2_34`, which excludes older glibc hosts. A
  published release needs a manylinux policy decision, for example building in
  the `manylinux` container rather than on this machine.
- [open] Only CPython 3.13 was exercised. The non-abi3 rule means each
  supported version needs its own wheel, which is the same gap the existing
  Python-version verification task covers.

### Tooling/process

- [acted] One disposable environment under `var/` holds the whole release
  toolchain; keeping build, wheel, and upload tools together removed the
  earlier split between a broken system `twine` and a hand-called backend.
- [acted] Comparing two builds of the same sources was the only way to see the
  setuptools version difference. Artifact hashes alone would have looked like
  a reproducibility failure rather than a toolchain difference.
- [acted] `var/venv-twine` and `var/venv-stub` were left in place; they are
  gitignored scratch and can be removed at any time.
