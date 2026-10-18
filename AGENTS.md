# exp_rurex — experimental workspace for rurex

`exp_rurex` is the experimental sibling of `rurex`: a place to prototype
regex-engine ideas, measure them against `rurex` and CPython's `re`, and only
then propose them upstream. The repository currently contains the spec and
workflow scaffolding, not an implementation.

Read these before doing any work:

- `spec/docs/index.md` — the catalog and recommended reading order of all
  design documents.
- `spec/skills/work.md` — the working loop.
- `spec/skills/task_tracking.md` — plan records and priorities.
- `spec/skills/report_tracking.md` — session reports and markers.
- `spec/skills/adr_tracking.md` — architecture decision records.

`spec/docs/index.md` and the files in `spec/skills/` are the authoritative
starting points. Pick the highest-priority `new` task from
`spec/docs/plan.md`, present your implementation plan, and keep the docs in
sync with the work.

Do not run project commands with `sudo`; ask the owner to install system
tools. Never commit credentials or machine-specific paths.
