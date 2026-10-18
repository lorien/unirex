# Testing and Checks

## Current state

No implementation has landed, so there is no automated test suite yet. The
checks below are the minimum for a documentation-only change. The first
experiment adds its own runnable checks; CI is added by the plan task that
follows it.

## Per-session final checks

Run these before every commit:

1. **Docs consistency.** Every repository path, command, and link mentioned
   in `spec/docs/` exists. Check internal markdown links by hand or with a
   link checker; do not leave dead references.
2. **Marker reconciliation.** Review the still-open report items:

       rg -n '\[(open|needs-decision)\]' spec/report/

   Mark `[acted]` every item this work resolved, with a one-line note, in the
   same commit. The remaining set must contain only items that are genuinely
   still open or undecided.
3. **No secrets.** Inspect the staged diff for credentials, tokens, private
   paths, and local configuration:

       git diff --cached

4. **No stray files.** `git status --short` must show only intended changes;
   local artifacts belong under `var/` or in `.env`, both gitignored.

## When code lands

- Each experiment ships a check that verifies its correctness claims against
  the pinned baseline. A performance claim without a rerunnable benchmark is
  not accepted.
- One command runs the formatter, the linter, and all experiment checks from
  a clean checkout. Document that command here and in the experiment README.
- CI runs the same command on every push, with tool versions pinned to the
  versions recorded in the experiments.
- A docs check in CI verifies local links and referenced paths so the
  documentation cannot silently drift.
