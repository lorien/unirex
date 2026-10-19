## Report on task: Rename the library to unirex

### Done

- Renamed the Rust package and library to `unirex`.
- Renamed the Python distribution and module to `unirex`; the extension is now
  `unirex.so` and exposes `import unirex`.
- Updated the PyO3 module name, class module string, and `Regex` repr.
- Updated every Rust import, example, test, Python tool, benchmark engine key,
  extension path, report renderer label, and documentation reference.
- Removed the old name from all tracked files, including historical reports,
  as requested.
- Moved the remote benchmark checkout to `/web/benchmark/unirex` and updated
  the development documentation.
- Rebuilt the release extension as `libunirex.so`, installed it as
  `var/py/unirex.so`, and re-ran the full Telegram matrix on `regexus` under
  `/tmp/core4.lock`.
- Regenerated `docs/telegram-benchmark.md`, the SVG charts, and the ignored
  HTML twin with the `unirex` name.
- Removed old-name build artifacts and scratch results from local and remote
  `target/` and `var/` trees.
- Verified formatting, clippy for the default and Python features, debug and
  release tests, the Python differential, the benchmark self-test, the binding
  smoke test, local markdown links, and SVG parsing.

### Results

The renamed matrix is effectively unchanged:

- Engine-only 1 thread: UCS 0.0690 s, Rust `regex` bytes 0.0712 s, str
  0.0711 s.
- Engine-only 4 threads: UCS 0.0258 s, bytes 0.0209 s, str 0.0209 s.
- End-to-end Python 1 thread: `unirex` 0.1851 s median and 0.1830 s minimum;
  CPython `re` 0.1923 s median and 0.1899 s minimum.

### Spec/ADR amendments

- [acted] Updated `AGENTS.md`, `README.md`, and all current `spec/docs/`
  references to `unirex`.
- [acted] Updated the remote checkout path in `spec/docs/dev.md`.
- [open] The local checkout directory keeps its previous directory name until
  the owner renames it after this commit. All repository content uses
  `unirex`.
- [open] No previous-name compatibility alias was added. Old installed
  extensions and wheels must be removed or replaced.
- [open] The generated report records `source workspace snapshot` because the
  remote checkout has no `.git`; future published runs can pass `--commit`.

### Future-task notes

- [open] Keep the local and remote checkout paths aligned after the owner
  renames the directory.
- [open] Re-run `tools/telegram_benchmark.py` rather than the individual
  runners when publishing a new page.
- [open] The next planned tasks are CI and Milestone 2; the rename does not
  change their scope.

### Tooling/process

- [acted] A tracked-file replacement covered package names, imports, extension
  paths, documentation, and historical reports in one pass.
- [acted] Cargo rebuilt `Cargo.lock` with the new package name.
- [acted] The remote checkout was moved with `mv` before syncing, preserving
  the verified corpus cache and avoiding a second download.
- [acted] The remote and local scratch trees were cleaned of old-name build
  products and result files.
