# Leftmost-First Semantics

Date: 2026-10-18
Status: accepted

## Context

Python `re` and POSIX regular expressions disagree about which match to
report. Python uses leftmost-first: the earliest start wins, and alternation
order plus greedy or lazy quantifiers decide among matches at that start.
POSIX uses leftmost-longest. The Rust `regex` crate is leftmost-first but
differs from Python in a few corners, including `$` before a trailing newline
and capture values in unbounded loops. The engine will eventually back a
Python `str` path, so the compatibility target must be explicit.

## Decision

- Match leftmost-first with Python `re` priority.
- Follow Python 3.7 and later for empty-match iteration: after an empty match
  at position `p`, retry at `p` requiring a non-empty match, then advance one
  code point.
- Let `$` match before a trailing newline, as Python does.
- Resolve `\b` and `\w` through HIR's Unicode word class; never silently
  substitute an ASCII-only class.
- During milestone 1, use the Rust `regex` crate as the differential oracle on
  decoded text, with a corpus that avoids the known divergences. From
  milestone 2, use Python `re` as the oracle on generated UCS strings.
- Document every remaining divergence in `spec/docs/semantics.md` and cover it
  with a test.

## Alternatives rejected

- POSIX leftmost-longest: incompatible with Python `re` and with the intended
  `rurex` str path.
- Rust `regex` semantics as the final target: `$` and some capture results
  would differ from Python.
- Best-effort compatibility without a written contract: divergences would be
  discovered by users instead of by tests.
- Implementing zero-width iteration by advancing one code point after every
  empty match: this is the pre-3.7 Python behavior and misses non-empty
  matches at the same position.
