## Report on task: Reserve the unirex name on crates.io

### Done

- Added the MIT `LICENSE` for the project, `Copyright (c) 2026 lorien`,
  matching the neighbouring `rurex` project's holder line.
- Added `packaging/crates-stub/`: `unirex 0.0.1`, edition 2021 so the
  placeholder does not demand Rust 1.85, no dependencies, no items in
  `src/lib.rs`, `documentation = "https://docs.rs/unirex"`, and a manifest
  comment forbidding both dependencies and an API.
- Committed the sources first, so `cargo publish` ran on a clean tree and
  `.cargo_vcs_info.json` carries real version-control information. The earlier
  attempt needed `--allow-dirty`, which was the wrong order.
- Published `unirex 0.0.1` to crates.io from `packaging/crates-stub/`. The
  first attempt failed with `400 Bad Request: A verified email address is
  required`, which the owner then fixed; the second attempt succeeded.
- Verified: the registry API reports `0.0.1` as the only version;
  `cargo info unirex@0.0.1` shows the published manifest, MIT license, and the
  docs.rs link; a scratch consumer crate under `var/` depending on
  `unirex = "0.0.1"` built against the registry.
- Updated `spec/docs/publishing.md` with the crate placeholder procedure, the
  clean-tree rule, the `--dry-run` step, the free-name recheck, the yank
  command, the verified-email requirement, and the fact that crates.io has no
  endpoint for validating a token ahead of time.
- Marked the crates.io items in two earlier reports `[acted]`.

### Registry state

- PyPI: `unirex 0.0.1`, metadata only, installs no modules.
- crates.io: `unirex 0.0.1`, library only, exposes no items.
- Both are below the first functional release, `0.1.0`.
- `https://docs.rs/unirex` returned 404 immediately after publishing. docs.rs
  builds are queued, so this is expected and should be rechecked later.

### Spec/ADR amendments

- [acted] `spec/docs/publishing.md` now documents both registries instead of
  only PyPI.
- [open] `spec/docs/overview.md` still does not mention the reserved registry
  names, and the repository now has a `LICENSE` that `README.md` does not
  reference. Decide whether to add a license line there.

### Future-task notes

- [open] Recheck `https://docs.rs/unirex` once docs.rs finishes the queued
  build. A permanently missing docs page would mean the crate metadata is
  wrong.
- [open] Yank `unirex 0.0.1` on both registries after the first functional
  release, with `cargo yank --version 0.0.1` and the PyPI web UI.
- [open] The placeholder manifests have no `repository` or `homepage` because
  the repository has no git remote. Add both once a remote exists.
- [open] `README.md` does not state the license or the reserved names, and the
  root `Cargo.toml` still has no `repository`, `readme`, `keywords`, or
  `categories` for the real release.

### Tooling/process

- [acted] `cargo publish --dry-run` needs no token, so packaging and the
  verification build can be validated before the irreversible upload.
- [acted] crates.io answers `403 this action can only be performed on the
  crates.io website` for `GET /api/v1/me` with any token, so a token cannot be
  validated before publishing. Do not read that response as a bad token.
- [acted] Publishing from a clean committed tree is what makes
  `.cargo_vcs_info.json` meaningful; `--allow-dirty` is a trap to be avoided
  rather than a convenience.
