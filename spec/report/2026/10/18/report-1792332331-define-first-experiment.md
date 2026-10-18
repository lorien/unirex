## Report on task: Define the first experiment

### Task (archived from plan.md)

## Define the first experiment

Status: done

Draft a proposal for the first experiment: the question about `rurex`
behavior or performance, the hypothesis, the baseline revision, the metric,
and a pass/fail threshold. Present it to the owner for approval before
creating `experiments/0001-*` or writing code. The proposal format is in
`spec/docs/experiments.md`.

### Done

- Read `var/handoff.txt` and treated it as the owner's approved proposal.
- Rewrote `overview.md` around the UCS-native engine: motivation, scope,
  success criteria, and the planned crate layout.
- Added `architecture.md` (pipeline, haystack, instruction set, classes,
  PikeVM, prefilter, DFA, Python binding), `semantics.md` (supported syntax,
  matching rules, flags, surrogates, divergences), `milestones.md` (five
  milestones with acceptance criteria), `benchmarks.md` (corpus provenance,
  metrics, baselines), and `dev.md`.
- Rewrote `testing.md` for the real test layers and commands, and
  `conventions.md` for Rust rather than the empty-project placeholder.
- Replaced the generic experiment plan with milestone tasks and a CI task in
  `plan.md`; updated `index.md`.
- Added ADR-0002 through ADR-0007: UCS-native engine, leftmost-first
  semantics, haystack enum, class representation, lazy DFA after benchmark,
  and the CPython UCS contract.
- Removed the obsolete `experiments.md`.
- Resolved the purpose and layout items in the bootstrap report.

### Spec/ADR amendments

- [open] Milestone 2 must pin the `\Z`/`\z` alias and the Python capture
  behavior in unbounded loops with differential tests. If Python-compatible
  capture values cannot be produced by the chosen priority scheme, record a
  superseding ADR instead of silently diverging.
- [open] Verify the Telegram corpus release URL and both SHA-256 values before
  milestone 3. The values came from the `rurex` provenance file and were not
  re-checked here.
- [open] Decide whether ASCII/UNICODE flags are needed for the `str` path
  after milestone 2. They are currently out of scope, and `semantics.md` says
  so.

### Future-task notes

- [open] Milestone 1 must not claim linear-time behavior in the README until
  the adversarial tests for `(a+)+b` and `(x+x+)+y` pass.
- [open] The milestone 1 differential corpus must avoid `$` and the
  capture-in-loop divergence, because the oracle is the Rust `regex` crate
  while the final target is Python `re`.
- [open] Milestone 5 must validate PEP 393 layout detection on CPython 3.10
  through 3.15, including free-threaded 3.14t, before trusting the fast path.

### Tooling/process

- [open] The CI task in `plan.md` should also run the docs link check and the
  marker reconciliation from `testing.md`.
- [open] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue noted in the bootstrap report; use the `rg`
  form from `testing.md` until the upstream workflow file is fixed.
