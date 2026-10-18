# Overview

## What this repository is

`exp_rurex` is an experimental sibling of `rurex`, the Python binding to the
Rust `regex` crate. It is a scratch workspace: an idea is prototyped here,
measured against `rurex` and CPython's `re`, and only then proposed for
`rurex` itself. Nothing in this repository is a released package, and no
prototype is expected to be production quality.

The repository currently contains only the spec and workflow scaffolding.
These documents describe the intended design; `spec/docs/plan.md` lists the
work needed to reach it.

## Goals

- Answer one concrete question per experiment, with a baseline, a metric,
  and a pass/fail threshold stated before the experiment runs.
- Keep every result reproducible from a clean checkout: pinned tool versions,
  a single entry command, and raw measurements kept with the experiment.
- Feed conclusions back into `rurex` as proposals, patches, or ADRs. A
  negative result is a result and is recorded as such.

## Scope and content policy

- Experiments live in this repository only until their verdict is recorded.
  Code that is not needed to reproduce a result may be removed afterwards;
  the experiment README and its measurements stay.
- Compare against a pinned `rurex` revision and, where semantics matter,
  against the corresponding CPython `re` behavior. Record both revisions.
- Do not copy code from `rurex` or other projects without recording the
  source and revision in the experiment README.
- No credentials, private paths, or machine-specific configuration in the
  repository. Local secrets belong in `.env`, local data under `var/`; both
  are gitignored.
- Correctness comes before speed. A faster prototype that changes matching
  semantics is a divergence to document, not an optimization.

## Repository layout

- `AGENTS.md` — thin pointer for coding agents: purpose and where to read.
- `spec/docs/index.md` — catalog of the design documents and reading order.
- `spec/docs/overview.md` — this document: purpose, scope, layout.
- `spec/docs/conventions.md` — document, experiment, code, and commit rules.
- `spec/docs/experiments.md` — the experiment lifecycle and directory layout.
- `spec/docs/testing.md` — how to verify work, now and once code lands.
- `spec/docs/plan.md` — the open-task list.
- `spec/docs/adr/` — architecture decision records, one file per decision.
- `spec/skills/` — the workflow files: `work.md`, `task_tracking.md`,
  `report_tracking.md`, `adr_tracking.md`.
- `spec/report/<year>/<month>/<day>/` — session reports, including archived
  plan tasks.
- `.gitignore` — ignores `/var/`, `/.env`, and build output.

Implementation directories (`experiments/`, source trees, CI configuration)
are created by the first experiment and are listed here as they appear.
