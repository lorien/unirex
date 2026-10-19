## Report on task: Make the crate comparison header symmetric

### Done

- Renamed the section header `Unirex crate vs Rust regex` to `unirex crate vs
  regex crate`. The old header labelled one side by artifact type and the other
  by language, although both sides are Rust crates; the new one names both
  participants the same way.
- Applied the same string to the three other places that carried it: the
  summary-table row, the chart alt text, and the SVG chart title.
- Re-rendered `docs/telegram-benchmark.md`, both SVGs, and the gitignored HTML
  from the saved matrix. No benchmark was re-run.

### Verification

- Every result data row is byte-identical to the published version.
- The engine-only chart `aria-label` reads `unirex crate vs regex crate`.
- `tools/check_docs.py` passes.

### Future-task notes

- [open] The sibling header still reads `Unirex python binding vs stdlib re`,
  which mixes an interface name with a library category in the same way.
  `unirex binding vs re module` would be parallel.
- [open] `README.md` still labels the baseline column `Rust regex crate, bytes`
  while the crate column is `unirex crate, UCS`; that pair is inconsistent with
  the section header now used in the report.

### Tooling/process

- [acted] Replacing a display string in the renderer and re-rendering is safer
  than editing the generated Markdown, which would be overwritten on the next
  run.
