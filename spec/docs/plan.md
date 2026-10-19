# Open Tasks

Open-task list. Format, statuses, and lifecycle:
`spec/skills/task_tracking.md`. Priorities: higher number = more important;
`-1` = deferred.

## Milestone 2: remaining gaps

Status: new
Priority: 2

Classes, word boundaries, `$`, flags `i m s` with scoped forms, lazy
repetition, named groups, capture values, `Match` objects, and `finditer` are
implemented. What remains:

- The `x` flag: ASCII whitespace and `#` comments outside classes. It is still
  rejected with a positioned error.
- `sub`, `subn`, and `split`.
- The full-grammar differential suite over UCS-1, UCS-2, and UCS-4, comparing
  every group span rather than only group 0, plus a corpus derived from
  CPython's `test_re`.
- DFA support for context assertions (`\b`, `$`, multiline `^`), which
  currently force those patterns onto the PikeVM.

Acceptance criteria are in `spec/docs/milestones.md`.

References: ADR-0003, ADR-0007, ADR-0008, ADR-0017

## Verify Python versions and free-threaded builds

Status: new
Priority: 2

The binding is verified only on CPython 3.13 GIL locally; CI now smoke-tests
CPython 3.10 through 3.15 GIL. What remains is free-threaded 3.14t, the 100k
end-to-end benchmark per interpreter, and recording ABI or GIL-detach
differences. Acceptance: a wheel and a passing smoke test for each supported
interpreter, including the free-threaded build.

References: ADR-0007, ADR-0012
