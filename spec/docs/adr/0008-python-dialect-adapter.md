# Python Dialect Adapter

Date: 2026-10-18
Status: accepted

## Context

Milestone 1 must behave like Python `re` for the syntax it accepts, but
`regex-syntax` is a Rust-dialect parser. Its HIR applies Rust `regex` class,
assertion, and flag semantics, and it loses the distinction between `$` and
`\Z`. Passing Python patterns straight into HIR would silently change the
meaning of several operators:

- Rust `\w` includes combining marks, connector punctuation, and join
  controls; Python means Unicode alphanumerics plus `_`.
- Rust `\s` follows Unicode `White_Space`; Python's `str.isspace()` also
  includes U+001C through U+001F.
- Rust `$` is end-only; Python `$` also matches before a final newline, and
  HIR `EndLF` matches before any newline.
- `[a&&b]` is an intersection in Rust and three literals in Python.
- Rust accepts `\<`, `\>`, `\b{...}`, `\p{...}`, nested classes, and POSIX
  classes that Python treats differently or rejects.

Token replacement handles only a few cases, such as `\Z` and octal spelling.
Class definitions, assertions, and class set operators are semantic, not
lexical.

## Decision

- Parse patterns with the `regex-syntax` AST, not HIR, because the AST keeps
  the syntax distinctions needed for Python behavior and for positioned
  errors.
- Validate a strict milestone 1 subset and lower the AST in this crate.
  Reject unsupported or ambiguous syntax instead of adopting Rust semantics.
- Normalize `\Z` to `\z` before parsing. Enforce Python numeric-escape rules:
  backreferences outside a class, octal inside a class, and octal values at
  most `0o377`.
- Milestone 1 accepts literals, `.`, explicit classes and ranges, alternation,
  greedy repetition, `^`, `\A`, `\Z`, `\z`, grouping, fixed-width hex escapes,
  and Python octal escapes. It rejects `$`, `\w`, `\d`, `\s`, `\b`, flags,
  lazy repetition, named groups, `\N{...}`, `\p{...}`, brace hex, Rust word
  assertions, class set operators, nested classes, POSIX classes, and
  repetition of an assertion.
- Milestone 2 adds Python `\w`, `\d`, and `\s` tables generated from CPython
  data, Python word boundaries, `$`, flags, lazy repetition, and captures.
- Use CPython `re` as the oracle for the accepted subset, including a
  randomized differential harness. Use Rust `regex` only where both agree.

## Alternatives rejected

- Lower `regex-syntax` HIR directly: silent Rust semantics, no way to
  distinguish `$` from `\Z`, and no place to reject Rust-only syntax before it
  is interpreted.
- Rewrite only a few tokens and keep HIR: fixes `\Z` and octal but not class
  definitions, `$`, word boundaries, class set operators, or flags.
- Write a full pattern parser now: unnecessary. The `regex-syntax` AST gives
  syntax positions, Unicode property parsing, and error reporting; only the
  lowering and Python-specific semantics need to be ours.
- Implement the full Python dialect in milestone 1: delays proving the
  matching model and forces Unicode table generation before the engine is
  measured. The strict subset is honest and testable against `re`.
- Accept Rust extensions as a superset: a Python pattern containing `[a&&b]`
  or `\<` would change meaning, which is exactly the failure this project
  must avoid.
