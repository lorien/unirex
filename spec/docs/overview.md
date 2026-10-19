# Overview

## What this repository is

`unirex` is an experimental, linear-time regular-expression engine that
matches directly against CPython's PEP 393 string storage: UCS-1, UCS-2, and
UCS-4. It starts as a standalone Rust crate with no Python dependency, so the
matching model can be tested on plain slices. A PyO3 binding exposes the same
engine to Python; the long-term goal is an alternative `str` path for `rurex`.

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
  anchors `^ $ \A \z \b \B`, capture groups, flags `i m s x`, and Python's
  Unicode class semantics, which are not yet implemented (milestone 2).
- Bounded compile time and memory, including a capped lazy DFA.
- A first-unit prefilter and a lazy DFA, both implemented.
- A PyO3 binding, implemented; the core remains usable without Python.

The engine accepts the strict subset documented in `spec/docs/semantics.md`:
literals, `.`, explicit classes, alternation, greedy repetition, `^`, `\A`,
`\Z`, `\z`, grouping, and fixed-width or Python octal escapes. `$`, `\w`,
`\b`, flags, lazy repetition, and named groups are rejected until milestone 2.

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

Milestones 1, 3, 4, and 5 are implemented. The current layout is:

- `AGENTS.md` — pointer for coding agents.
- `README.md` — supported syntax, rejected syntax, design summary, commands.
- `spec/docs/` — current design, conventions, semantics, testing, benchmarks,
  milestones, and the open-task list.
- `spec/skills/` — workflow files.
- `spec/report/` — session reports.
- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `pyproject.toml` — crate,
  toolchain, and maturin metadata.
- `src/lib.rs` — public API: `Ucs`, `Pattern`, `Match`, `Error`, `Scratch`, and
  the `Matches` and `MatchesWith` iterators.
- `src/ucs.rs` — the three-kind haystack view and the unit search trait.
- `src/compile.rs` — Python-dialect adapter, AST validation, and lowering.
- `src/pikevm.rs` — Scratch, the group-0 PikeVM, and DFA dispatch.
- `src/dfa.rs` — lazy DFA over code-point equivalence classes.
- `src/python.rs` — PyO3 module and CPython UCS access.
- `src/error.rs` — compile and usage errors.
- `tests/m1.rs` — accepted subset, Python spans, UCS kinds, adversarial
  timing, scratch reuse, and the Rust `regex` common-subset differential.
- `examples/bench.rs` — self-contained Telegram engine-only benchmark.
- `examples/oracle.rs` — TSV oracle used by the Python differential harness.
- `tools/python_diff.py` — randomized comparison against CPython `re`.
- `tools/bench_python.py` — end-to-end Python benchmark.
- `tools/test_python_binding.py` — binding smoke test against `re`.
- `tools/telegram_benchmark.py` — full Telegram matrix and report trigger.
- `tools/telegram_report.py` — Markdown, SVG chart, and HTML report renderer.
- `docs/telegram-benchmark.md` — generated public Telegram benchmark report.
- `docs/img/` — generated SVG chart assets for that report.
- `var/` — gitignored corpus cache, raw benchmark results, and local
  artifacts.

The benchmark harness, first-unit prefilter, lazy DFA, and PyO3 binding exist.
The one-thread end-to-end Telegram result is now slightly faster than CPython
`re`. Remaining work is Python class/flag/capture compatibility (milestone 2)
and the open tasks in `spec/docs/plan.md`.
