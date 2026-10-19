## Report on task: Name the benchmark sections by what is compared

### Done

- Renamed the generated report's headings to the plain comparison names:
  `Unirex python binding vs stdlib re` and `Unirex crate vs Rust regex`, for
  the page title, both sections, both table headers, both chart titles, and
  both chart legends.
- Column headers are now `unirex binding median (s)` / `unirex binding min (s)`
  / `re median (s)` / `re min (s)` and `unirex crate, UCS (s)` / `regex bytes
  (s)` / `regex str (s)`.
- Removed the internal jargon from the page: `PyO3 extension`, `end-to-end`,
  and `engine-only` no longer appear. The protocols are now `Python binding
  protocol` and `Rust crate protocol`, and the prose talks about "the binding
  and crate numbers" instead of measurement names a reader has to decode.
- Rewrote the summary table as `Measurement | Runs in | Compared with | What
  the time includes`, with the same two names as the sections.
- Applied the same wording to `README.md` and `spec/docs/benchmarks.md`, and
  updated the naming rule there to require `unirex binding` and `unirex crate`
  rather than `unirex (PyO3)`.
- Re-rendered `docs/telegram-benchmark.md`, both SVGs, and the gitignored HTML
  from the saved matrix; no benchmark was re-run.

### Verification

- Every result data row is byte-identical to before the rename.
- SVG legends read `unirex binding` and `re`, and `unirex crate (UCS)`,
  `regex bytes`, `regex str`; the `aria-label` values carry the new titles.
- `rg 'end-to-end|engine-only|PyO3'` returns nothing in the rendered page.
- `tools/check_docs.py` passes.

### Spec/ADR amendments

- [acted] `spec/docs/benchmarks.md` naming rule now requires interface names
  that a reader outside the project understands, not build-system terms.

### Future-task notes

- [open] `tools/telegram_report.py` is still untested; a synthetic matrix
  fixture in CI would have caught the earlier cryptic-label regression and the
  broken string literal I introduced while patching it.
- [open] `spec/docs/benchmarks.md` duplicates the published numbers as prose
  bullets. Pointing at the published page instead would remove a second place
  to keep in sync.

### Tooling/process

- [acted] A one-line string replacement in a Python source file needs a
  `py_compile` immediately afterwards; an unterminated literal only surfaced
  when the renderer ran.
- [acted] Comparing data rows before and after a label change is the cheap proof
  that presentation work did not touch results.
