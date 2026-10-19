## Report on task: Wipe the leftover RUREX_TG_DATA environment variable

### Done

- Renamed the corpus-path override from `RUREX_TG_DATA` to
  `UNIREX_TG_DATA` in both benchmark runners: `examples/bench.rs` (usage text,
  `var_os` lookup, error message) and `tools/bench_python.py` (docstring,
  `os.environ` lookup, error message). No fallback alias was added.
- Updated every document that names the variable: `README.md`,
  `spec/docs/dev.md`, `spec/docs/benchmarks.md`, `spec/docs/conventions.md`,
  `spec/docs/adr/0009-benchmark-protocol.md`, and the archived benchmark
  report that recorded the original design.
- Added a `Naming` section to `spec/docs/conventions.md`: project-owned
  identifiers use the `unirex` name, the former name `exp_rurex` must not
  appear anywhere, and `rurex` names only the external project at
  `github.com/lorien/rurex`, whose corpus release URL stays unchanged as data
  provenance.
- Labelled that URL as external in `spec/docs/benchmarks.md`.
- Replaced the literal-pipe `grep` marker command in `spec/skills/work.md`
  with the `rg` form from `testing.md`, and marked the 13 reports that carried
  that item as `[acted]`.
- Marked the rename report's local-directory and compatibility-alias items
  `[acted]`.
- Verified: no occurrence of the old variable remains outside this report;
  `cargo fmt --check`, `cargo clippy
  --all-targets -- -D warnings`, `cargo test --release`, the Python
  differential (16,093 cases, 0 mismatches), the benchmark self-test, and the
  binding smoke test all pass. Ran the Rust and Python benchmarks with
  `UNIREX_TG_DATA` set to the cached corpus, with a bogus path (correct new
  error text), and with the variable unset (cache path still works).

### Spec/ADR amendments

- [acted] Added the `Naming` rule to `spec/docs/conventions.md` so a future
  rename cannot leave an identifier behind, which is exactly how this variable
  survived the `exp_rurex` to `unirex` rename.
- [acted] Fixed the marker command in `spec/skills/work.md`.
- [acted] `spec/docs/testing.md` states in the present tense that CI runs these
  checks on every push, but there is no `.github/` directory and no git
  remote. Reword it or do the CI task.
  Resolved: `testing.md` now describes the real workflow, and
  `tools/check_docs.py` fails on the retired identifier in code or docs.
- [acted] `RUREX` survived the rename because no check greps for the old name.
  A docs/naming check should reject `exp_rurex`, `RUREX_`, and any
  project-owned identifier that does not use the `unirex` prefix; fold it into
  the CI task.
  Resolved: the docs check rejects the retired identifiers outside reports
  that record their removal.

### Future-task notes

- [open] Any shell profile or matrix script on `regexus` that exports
  `RUREX_TG_DATA` now falls through to the download path. The remote
  environment was not inspected; check it before the next published matrix.
- [open] The `rurex` corpus URL and baseline mentions stay. If the corpus is
  ever re-hosted under a `unirex` release, update the URL, the pinned
  SHA-256 in both runners, `spec/docs/benchmarks.md`, and
  `spec/docs/adr/0009-benchmark-protocol.md` together.
- [acted] The registry stub publish is half done: PyPI now owns `unirex`
  through the `0.0.1` placeholder. The crates.io side is still pending because
  no account token exists on the development machine.
  Resolved: crates.io also owns `unirex` through the `0.0.1` placeholder, so
  both registries are reserved below the real `0.1.0`.

### Tooling/process

- [acted] A single `sed` token replacement covered all eight files; the
  remaining risk is prose that paraphrases an identifier, so the follow-up was
  a manual `Naming` rule rather than another search.
- [acted] Verifying the renamed variable by running with it set, bogus, and
  unset caught what a grep cannot: that the code path is still reachable.
