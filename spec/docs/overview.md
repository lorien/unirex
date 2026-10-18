# Overview

## What this repository is

`exp_rurex` is an experimental, linear-time regular-expression engine that
matches directly against CPython's PEP 393 string storage: UCS-1, UCS-2, and
UCS-4. It starts as a standalone Rust crate with no Python dependency, so the
matching model can be tested on plain slices. A PyO3 binding comes later, and
the long-term goal is an alternative `str` path for `rurex`.

The engine exists to answer one question: can UCS-native matching remove the
UTF-8 transcode and still be correct and fast enough to beat the current
`rurex` path?

## Motivation

`rurex` matches UTF-8. CPython stores `str` as a contiguous array of UCS code
units with a `kind` field. To scan a non-ASCII `str`, `rurex` either borrows
CPython's cached UTF-8 copy or transcodes the UCS buffer. CPython's own `re`
reads the UCS buffer and compares code points directly, with no encoding step.

Telegram corpus: 1,000,000 posts, about 470 MB of UTF-8 text, mostly Arabic
and Persian, pattern `@[a-zA-Z][a-zA-Z0-9_]{4,31}`. Measured under the GIL
with one thread:

- `re` str: about 0.47 s.
- `rurex` str: about 0.75 s.
- `rurex` bytes: about 0.35 s.
- `re` bytes: about 0.51 s.
- The cold transcode is roughly 38 percent of the `rurex` str time.
- 77.2 percent of posts contain no `@`, so a first-unit prefilter can skip
  most transcodes.

A UCS-native engine also avoids UTF-8 decoding when constructing result
strings and can return character offsets directly.

## Scope

In scope:

- Linear-time matching in the haystack length for a fixed pattern. No
  exponential backtracking, including on `(a+)+b` and `(x+x+)+y`.
- Leftmost-first matching compatible with Python `re` for the supported
  subset, with every divergence documented.
- Native UCS-1, UCS-2, and UCS-4 haystacks; no UTF-8 conversion in the
  matching path.
- Literals, `.`, character classes, alternation, greedy and lazy `* + ? {n,m}`,
  anchors `^ $ \A \z \b \B`, capture groups, flags `i m s x`, and the Unicode
  classes exposed by `regex-syntax` HIR.
- Bounded compile time and memory, including a capped lazy DFA if one is
  added.
- A first-unit prefilter and, if the benchmark justifies it, a lazy DFA.
- A PyO3 binding as a later milestone, not a first milestone.

Out of scope by design, rejected at compile time: backreferences, lookaround,
atomic groups, possessive quantifiers, conditionals, and recursion.

The repository is not a replacement for CPython's `re` and does not ship a
general-purpose package. It is an experiment that may later contribute a
`str` path to `rurex`.

## Success criteria

- Adversarial patterns terminate in seconds, not minutes, on adversarial
  input.
- Differential tests pass against the reference implementation on the
  supported subset, across UCS-1, UCS-2, and UCS-4.
- Equivalent code-point sequences produce identical matches in every UCS
  kind, including lone surrogates on the UCS-2 path.
- The Telegram benchmark beats the current `rurex` str path of about 0.75 s
  end to end and ideally approaches 0.45 s.
- After integration, ASCII `str`, bytes input, and complex patterns do not
  regress.

## Repository layout

The implementation layout is created by milestone 1. The intended layout is:

- `AGENTS.md` — pointer for coding agents.
- `spec/docs/` — current design, conventions, semantics, testing, benchmarks,
  milestones, and the open-task list.
- `spec/skills/` — workflow files.
- `spec/report/` — session reports.
- `Cargo.toml`, `rust-toolchain.toml` — crate and toolchain.
- `src/lib.rs` — public API: `Ucs`, `Regex`, `Match`, `Error`.
- `src/ucs.rs` — the three-kind haystack view.
- `src/compile.rs` — HIR lowering into the instruction program.
- `src/classes.rs` — class set and per-kind membership structures.
- `src/pikevm.rs` — linear-time simulation, group 0 first, captures later.
- `src/prefilter.rs` — required-first-unit extraction and scanning.
- `src/dfa.rs` — optional lazy DFA, added only after the benchmark.
- `src/error.rs` — compile and usage errors.
- `tests/` — unit, differential, adversarial, and fuzz tests.
- `benches/` — benchmark harness and the Telegram corpus reader.
- `var/` — gitignored corpus cache and local artifacts.

At present no implementation exists; the documents describe the intended
design and `spec/docs/plan.md` lists the milestones.
