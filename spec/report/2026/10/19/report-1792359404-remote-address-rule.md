## Report on task: Forbid remote server IPv4 in documentation

### Done

- Searched the repository for IPv4 literals. None were present in
  `AGENTS.md`, `README.md`, `spec/docs/`, or the reports.
- Added a rule to `AGENTS.md`: never write the IPv4 address of a remote
  benchmark server in documentation, reports, README files, or commit
  messages; use the hostname or alias, for example `regexus`.
- Added the same rule to `spec/docs/conventions.md` so it is part of the
  project's document conventions, not only the agent pointer.

### Spec/ADR amendments

- [open] Add a documentation check that rejects IPv4 literals in `AGENTS.md`,
  `README.md`, and `spec/docs/`. Such a check would be heuristic because
  version numbers and checksums can resemble addresses.
- [open] Extend the check to `spec/report/` and commit messages if the owner
  wants enforcement beyond documentation.

### Tooling/process

- [acted] Used an IPv4 literal search over the documentation and reports; it
  found no addresses.
- [open] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue; use the `rg` form from `testing.md`.
