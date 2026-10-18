# Open Tasks

Open-task list. Format, statuses, and lifecycle:
`spec/skills/task_tracking.md`. Priorities: higher number = more important;
`-1` = deferred.

## Define the first experiment

Status: new
Priority: 2

Draft a proposal for the first experiment: the question about `rurex`
behavior or performance, the hypothesis, the baseline revision, the metric,
and a pass/fail threshold. Present it to the owner for approval before
creating `experiments/0001-*` or writing code. The proposal format is in
`spec/docs/experiments.md`.

## Scaffold the toolchain and repository hygiene

Status: new
Priority: 1

Once the first experiment is approved, choose the implementation toolchain
and record the choice in an ADR. Extend `.gitignore` for the chosen build
artifacts, add the formatter and linter, and provide a single command that
reproduces the experiment from a clean checkout. Update
`spec/docs/overview.md` with the directories that appear.

## Set up continuous integration

Status: new
Priority: 0

Add CI that runs the formatter, linter, experiment checks, and a docs check
on every push. Pin tool versions in the workflow and keep the CI command
identical to the local check command documented in `spec/docs/testing.md`.
