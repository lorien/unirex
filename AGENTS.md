# exp_rurex — UCS-native linear-time regex engine

`exp_rurex` is an experimental Rust engine that matches regular expressions
directly against CPython's PEP 393 UCS-1, UCS-2, and UCS-4 storage, with no
UTF-8 conversion in the matching path. The long-term goal is an alternative
`str` path for `rurex`; the first milestones are a standalone crate with no
Python dependency.

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

Never write the IPv4 address of a remote benchmark server in documentation,
reports, README files, or commit messages. Refer to the server only by its
hostname or alias, for example `regexus`.

Benchmarks on `regexus` use the first four physical P-cores, CPUs `0,2,4,6`.
Take the exclusive `/tmp/core4.lock` lease for the whole matrix; see
`spec/docs/benchmarks.md` and ADR-0014.
