# Conventions

## Documents

- One focused topic per document under `spec/docs/`; `index.md` catalogs
  them; `plan.md` is the open-task list.
- Documents describe the intended design, never a list of known breakage.
  Fix the code or the experiment, not the description of the goal.
- Keep documents in sync with the implementation in the same commit that
  changes behavior. When it is unclear whether the code or the document is
  right, ask the owner in plain text instead of guessing.
- Prefer short paragraphs and flat lists over tables. Use ASCII punctuation;
  do not use arrow symbols or enclosed alphanumerics.
- Reference other documents by repository-relative path, for example
  `spec/docs/testing.md`.

## ADRs

Decisions with real alternatives get a numbered record in `spec/docs/adr/`
at decision time. The format is defined in
`spec/docs/adr/0000-adr-format.md`; the workflow is
`spec/skills/adr_tracking.md`.

- Filename `NNNN-short-title.md`, numbered sequentially from `0001`.
- Records are short: context, decision, alternatives rejected.
- Records are never deleted or renumbered. A later change supersedes or
  revises a record by annotating its `Status:` line.
- Topical documents carry the current truth; ADRs carry the history and the
  rationale. Do not duplicate details between them.
- Plan records cite relevant ADRs on their `References:` line.

## Experiments

- Each experiment lives in `experiments/NNNN-slug/`, numbered sequentially.
  The full lifecycle and layout are in `spec/docs/experiments.md`.
- The README states the question, hypothesis, baseline, metric, procedure,
  and verdict before results are interpreted.
- Pin tool and dependency versions; the experiment must run from a clean
  checkout with one documented command.
- Keep raw measurements in the experiment directory. Summaries belong in the
  session report.
- Do not tune a benchmark after seeing its result without recording that the
  procedure changed and rerunning the baseline.

## Code

Language and toolchain are chosen by the first experiment and recorded in an
ADR. Until then, these rules apply to any code that lands:

- Formatting and linting are mandatory and run by the project's single check
  command.
- Public interfaces get a short doc comment; scripts get a usage line.
- No hard-coded credentials. Read secrets from the environment or `.env`.
- No `sudo` for project commands. If a system tool is missing, ask the
  owner to install it.
- Local-only paths and generated files stay out of git: `/var/`, `.env`,
  build directories, virtualenvs, and caches.

## Commits

- One logical change per commit; the subject is short and imperative, for
  example `Add experiment harness skeleton`.
- The body, when needed, explains what changed and why, not how.
- Documentation updates for a behavior change belong in the same commit.
  Do not leave docs behind for a follow-up.
- Never commit secrets or local configuration.
