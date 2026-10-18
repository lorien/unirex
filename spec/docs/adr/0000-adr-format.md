# ADR Format

Date: 2026-10-18
Status: accepted

## Context

The project needs a lightweight, uniform way to record decisions that had
real alternatives, so later agents and owners can learn why the repository
looks the way it does without reverse-engineering it from code or commit
history.

## Decision

Each record lives in `spec/docs/adr/NNNN-short-title.md`, numbered
sequentially from `0001` (`0000` is this format record). A record contains:

- a title,
- a `Date:` line,
- a `Status:` line (`accepted`, plus supersession notes such as
  `accepted (superseded by ADR-0007)`),
- a `Context` section describing the forces at play,
- a `Decision` section describing what was chosen,
- an `Alternatives rejected` section describing what was not chosen and why.

Records are short: the current design lives in the topical documents under
`spec/docs/`, while the ADR carries the history and rationale. Records are
never deleted or renumbered; a later change supersedes or revises a record
by annotating its `Status:` line. The workflow is defined in
`spec/skills/adr_tracking.md`.

## Alternatives rejected

- Free-form notes scattered across topical documents: decisions lose their
  traceability and supersession history.
- A database or tooling-backed decision log: unnecessary overhead for a
  repository read by agents and humans.
- Recording only the final design without alternatives: later readers cannot
  tell whether an option was considered and rejected or never seen.
