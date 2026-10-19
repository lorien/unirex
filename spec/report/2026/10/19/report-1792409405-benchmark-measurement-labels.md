## Report on task: Disambiguate the two benchmark measurements

### Done

- Diagnosed the ambiguity: the word `unirex` named three different things on one
  page. In the first table the header said `unirex median (s)`, meaning the
  PyO3 extension inside a Python process; in the second it said `unirex UCS
  (s)`, meaning the Rust crate with no Python at all, right next to columns
  labelled "Rust regex". Both SVG legends used the same `unirex` label, and the
  page title promised a comparison with `re` while the second table compared
  against the `regex` crate.
- Added a `Two measurements, two processes` table at the top of the generated
  page: measurement, process, what is called, and what the time includes.
- Renamed the two result sections to `End-to-end: PyO3 extension called from
  Python` and `Engine-only: Rust crate, no Python`, and added a lead sentence
  above each table stating which interface is measured and what is excluded.
- Renamed the result columns: `unirex (PyO3)` and `CPython re` in the first
  table, `unirex crate, UCS` and `regex crate, bytes/str` in the second.
- Renamed the chart legends and titles the same way, and changed the page title
  to name both baselines.
- Added a sentence to `Reading the numbers`: in the end-to-end table `unirex`
  is the PyO3 extension, in the engine-only table it is the Rust crate.
- Added left alignment support to the Markdown table renderer, so the new
  prose table is not rendered right-aligned.
- Updated `README.md` and `spec/docs/benchmarks.md` with the same naming, and
  added a naming rule to `spec/docs/benchmarks.md` so future tables name the
  interface rather than the project.
- Re-rendered `docs/telegram-benchmark.md` and both SVGs from the saved matrix
  under `var/bench-results/telegram`; no benchmark was re-run.

### Verification

- Every result data row is byte-identical before and after: the diff of
  `docs/telegram-benchmark.md` contains no changed `| 1 |` through `| 4 |` rows.
- Both SVG files differ only in text nodes and the `aria-label` that encodes the
  chart title; all coordinates and `points` attributes are unchanged.
- `tools/check_docs.py` passes, and no bare `unirex` remains in a result header
  or chart legend anywhere in the repository.
- No document referenced the old section headings.

### Spec/ADR amendments

- [acted] `spec/docs/benchmarks.md` now states the naming rule for result
  tables: name the interface (`unirex (PyO3)`, `unirex crate`) and the
  baselines, never a bare project name.
- [open] `spec/docs/benchmarks.md` still lists the raw engine-only numbers as
  prose bullets while the published page uses tables. Decide whether the spec
  should stop duplicating the published numbers entirely and point at
  `docs/telegram-benchmark.md` instead.

### Future-task notes

- [open] `tools/telegram_report.py` has no test. A synthetic matrix fixture in
  CI would catch a label regression like this one, and would also exercise the
  renderer and the HTML twin, which are currently only run by hand.
- [open] The HTML twin is regenerated but gitignored, so the disambiguation is
  not visible in the published `docs/telegram-benchmark.html` until someone
  runs the renderer locally.

### Tooling/process

- [acted] Comparing numeric tokens and table rows before and after a re-render
  is the cheap way to prove that a presentation change did not disturb the
  data; a diff eyeball on a 70-line Markdown file is not.
- [acted] Stripping text nodes from an SVG before diffing isolates geometry from
  labels, which showed that only `aria-label` and two legend texts moved.
