# Spec-Driven Agent Workflow

Date: 2026-10-18
Status: accepted

## Context

The repository starts empty and will be worked on by coding agents in short,
independent sessions. Each session must be able to recover the project's
purpose, conventions, current tasks, and prior decisions without relying on
chat history or on the memory of a previous agent. The project also expects
several agents to work at different times, so the source of truth must be
versioned together with the code and must be cheap to read.

## Decision

Adopt a spec-driven workflow:

- `AGENTS.md` at the repository root is a thin pointer. It states the
  purpose and sends agents to the catalog and workflow files.
- `spec/docs/` holds the current truth: overview, conventions, experiment
  protocol, testing, and the open-task list. `spec/docs/index.md` is the
  catalog and reading order.
- `spec/skills/` holds the day-to-day workflow copied verbatim from the
  bootstrap source: `work.md`, `task_tracking.md`, `report_tracking.md`,
  `adr_tracking.md`.
- Work is tracked in `spec/docs/plan.md`; a finished task is removed from the
  plan and archived in a session report under `spec/report/`.
- Decisions with real alternatives become numbered ADRs in
  `spec/docs/adr/`. Topical documents carry current behavior; ADRs carry the
  rationale and history.
- Documentation is updated in the same commit as the behavior it describes.

## Alternatives rejected

- A single long README: it becomes stale and mixes current truth, history,
  and workflow, so agents cannot tell which part is authoritative.
- Agent-specific prompt files (for example separate `CLAUDE.md`,
  `.cursorrules`, or tool-specific configs): they duplicate content and
  diverge; one tool-neutral `AGENTS.md` plus shared specs keeps every agent
  on the same source.
- An external issue tracker as the only task list: tasks and their context
  would not be versioned with the code, and offline or fresh sessions could
  not see them.
- Relying on commit messages alone for decisions: rationale and rejected
  alternatives are hard to find and easy to lose in history.
