## Report on task: Bootstrap agent workflow and spec docs

Ad-hoc work: the repository was empty and had no git history.

### Done

- Added `AGENTS.md` as a thin pointer: purpose, catalog, workflow files, and
  the instruction to present a plan and keep docs in sync.
- Copied the four workflow files verbatim from the bootstrap source into
  `spec/skills/`: `work.md`, `task_tracking.md`, `report_tracking.md`,
  `adr_tracking.md`.
- Wrote the initial spec: `overview.md`, `conventions.md`, `experiments.md`,
  `testing.md`, `plan.md`, and `index.md`.
- Added ADR-0000 (the ADR format) and ADR-0001 (the spec-driven workflow
  decision).
- Added `.gitignore` for `/.env`, `/var/`, and common Python and build
  artifacts.
- Initialized git and committed the scaffolding.

### Spec/ADR amendments

- [acted] Confirm the purpose statement in `overview.md`. Resolved by
  `var/handoff.txt`: the project is a UCS-native linear-time regex engine;
  `overview.md` was rewritten and ADR-0002 records the choice.
- [acted] Confirm that `experiments/NNNN-slug/` is the right layout. The
  handoff replaced it with the concrete crate layout and milestones;
  `experiments.md` was removed and `milestones.md` added.

### Future-task notes

- [open] The first benchmark must pin the baseline and record the machine,
  operating system, and tool versions, per `spec/docs/benchmarks.md`.
- [acted] Extend `.gitignore` when the toolchain is chosen. The toolchain is
  Rust; `target/`, Python artifacts, `/var/`, and `/.env` are already
  covered.
- [acted] Add implementation directories to the layout in `overview.md`.
  The planned crate layout is now documented there.

### Tooling/process

- [open] Automate the checks in `testing.md` (local link check and marker
  reconciliation) in CI once CI exists.
- [open] The marker command in `spec/skills/work.md` uses
  `grep -rn "\[open\]|\[needs-decision\]"`, where the unescaped `|` is
  literal in basic regular expressions, so it matches nothing. Use
  `rg -n '\[(open|needs-decision)\]' spec/report/` as documented in
  `testing.md`; `work.md` is copied verbatim and must not be edited in this
  repository.
