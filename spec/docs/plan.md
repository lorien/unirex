# Open Tasks

Open-task list. Format, statuses, and lifecycle:
`spec/skills/task_tracking.md`. Priorities: higher number = more important;
`-1` = deferred.

## Milestone 2: Python classes, flags, captures

Status: new
Priority: 2

Add Python-equivalent `\w`, `\d`, and `\s` tables generated from CPython data,
Python word boundaries `\b` and `\B`, the Python `$` assertion, capture slots,
lazy repetition, named groups, and flags `i m s x`. Switch the full
differential suite to Python `re` on generated UCS-1/2/4 strings. Acceptance
criteria are in `spec/docs/milestones.md`.

References: ADR-0003, ADR-0007, ADR-0008

## Verify Python versions and free-threaded builds

Status: new
Priority: 2

The binding is verified only on CPython 3.13 GIL locally; CI now smoke-tests
CPython 3.10 through 3.15 GIL. What remains is free-threaded 3.14t, the 100k
end-to-end benchmark per interpreter, and recording ABI or GIL-detach
differences. Acceptance: a wheel and a passing smoke test for each supported
interpreter, including the free-threaded build.

References: ADR-0007, ADR-0012
