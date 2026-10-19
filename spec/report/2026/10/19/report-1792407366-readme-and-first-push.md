## Report on task: Rewrite the README and publish the repository

### Done

- Rewrote `README.md` for a public audience: the one-line description, then a
  blunt status line (experimental, strict Python subset, group 0 only, not a
  `re` replacement), an `Install` section, the missing motivation, a milestone
  status table, a single benchmark table, a design-docs pointer, and a license
  section.
- Added the `Install` section because the reserved `0.0.1` packages are empty
  on both registries. Without that warning, a published name actively misleads
  anyone who runs `pip install unirex` or `cargo add unirex`.
- Moved the motivation into the README with its numbers: the UTF-8 transcode
  costs about 38 percent of `rurex`'s `str` time, and 77.2 percent of corpus
  posts contain no `@`, so the first-unit prefilter skips most of the work.
- Verified both code examples rather than trusting them: the Rust example was
  compiled against the crate in a scratch project outside the tree, and the
  Python example was executed against the built extension, returning
  `['@alice123']`, `@alice123`, and `True`.
- Removed a duplicated `{n,}` entry and checked every local link in the README
  resolves.
- Added the `origin` remote `git@github.com:lorien/unirex.git` and pushed all
  21 commits. Verified through the GitHub API that `7c020e6` is the remote tip
  and that `raw.githubusercontent.com/lorien/unirex/main/README.md` serves the
  new text.
- Scanned before pushing: no `var/`, `target/`, `.so`, `__pycache__`, or `.env`
  paths are tracked, and no commit in history contains a token-shaped string.

### Registry and repository state

- `github.com/lorien/unirex` is public, default branch `main`, 21 commits, with
  the one-line description already set on the repository page.
- PyPI and crates.io still hold the empty `unirex 0.0.1` placeholders.

### Spec/ADR amendments

- [acted] The README now points at `spec/docs/index.md` as the source of truth
  instead of duplicating it, and states the license.
- [open] The two placeholder READMEs in `packaging/` still say "The project is
  not yet public", which is now false. They cannot be changed on the registries
  without a new version, so decide whether to fix the source and accept the
  divergence or leave both alone until `0.1.0`.

### Future-task notes

- [open] Add `repository`/`homepage` to both placeholder manifests and to the
  root `Cargo.toml`, and `readme`, `keywords`, and `categories` to the root
  `Cargo.toml`. The remote now exists, so nothing blocks this; the published
  `0.0.1` artifacts cannot be retrofitted.
- [open] Every commit is authored `User <guest@example.com>`, the machine-wide
  default in this environment, and that identity is now public in the pushed
  history. Setting a real identity matters before more history accumulates.

### Tooling/process

- [acted] Pushing only after the README, the license, and the stubs were
  committed meant the first thing a visitor sees is consistent with the
  registry state.
- [acted] The scratch compile and the extension run caught nothing this time,
  but they are the reason the examples can be trusted; a README example that
  does not run is worse than no example.
