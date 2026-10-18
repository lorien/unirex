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

## Rust

- Edition 2024, `rust-version = "1.85"`. Build with stable Rust selected by
  `rust-toolchain.toml`.
- `cargo fmt` formatting is mandatory; `cargo clippy --all-targets -- -D
  warnings` must pass.
- Python `re` is the semantic authority for the supported dialect. Reject
  syntax that cannot be made Python-exact; never silently accept Rust `regex`
  semantics.
- Parse with the `regex-syntax` AST and lower it in this crate. Do not lower
  `regex-syntax` HIR: it applies Rust class, assertion, and flag semantics and
  loses the distinction between `$` and `\Z`.
- Numeric escapes follow Python's rules: backreferences outside a class,
  octal inside a class, and octal values at most `0o377`.
- Public items get `///` doc comments. Errors use a typed `Error` enum; do
  not `unwrap()` or `expect()` on user-controlled input.
- The core engine is safe Rust. `unsafe` appears only in the Python binding,
  where validated CPython pointers are converted to slices, and every block
  carries a safety comment naming the invariant.
- The hot search loop is monomorphized over the UCS unit type. Do not
  dispatch on the unit type per character and do not allocate per match.
- Unit tests live in `#[cfg(test)]` modules next to the code, or in `tests/`
  for differential and adversarial suites. Python-generated span
  expectations belong in the test file with a note naming the CPython version
  when the behavior differs across Python versions.
- Do not use the byte-oriented `regex-automata` DFA for matching; its
  alphabet and spans are UTF-8 bytes. `regex-syntax` is the parser.

## Benchmarks

- A performance claim needs a reproducible benchmark and a recorded baseline.
- Correctness checks run before timing.
- Report the machine, tool versions, command, repetition count, and raw
  numbers. Label synthetic corpora as synthetic.

## Commits

- One logical change per commit; the subject is short and imperative, for
  example `Add UCS haystack enum`.
- The body, when needed, explains what changed and why, not how.
- Documentation updates for a behavior change belong in the same commit.
  Do not leave docs behind for a follow-up.
- Never commit secrets, corpus data, or local configuration. `/var/` and
  `.env` are gitignored.
- No `sudo` for project commands. If a system tool is missing, ask the owner
  to install it.
