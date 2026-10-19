# Bounded Candidate Work Instead of a Reverse Pass

Date: 2026-10-19
Status: accepted

## Context

The DFA search found a first-unit candidate with the prefilter, then ran an
anchored scan from each candidate. That is quadratic when a scan survives to the
end of the haystack and fails: `a+b` over a run of `a` took 2.17 s at 32,000
units and grew about 4x per doubling. The README and `overview.md` claim a
linear-time engine, so the bound was simply false for such patterns.

The alternative worth taking seriously is the design Rust `regex` uses: one
unanchored forward pass to find the match end, then a reverse pass over a
reversed program to recover the start. That keeps leftmost-first exact in O(n).

## Decision

Keep the anchored candidate loop and bound its total work. Each anchored scan
reports how many units it walked, the loop accumulates that, and exceeding
`8 * (len + 1)` aborts to the PikeVM, which is O(n * m) unconditionally. The
search cost therefore stays linear in haystack length with a constant factor,
and the fallback is an engine that is already the correctness authority.

`a+b` at 32,000 units is now 1.1 ms and doubles with the input instead of
growing fourfold. The Telegram pattern is unaffected because its scans end
quickly.

## Alternatives rejected

- A reverse program and a second pass to find the start: implemented and
  reverted. Getting the leftmost-first *end* out of an unanchored forward pass
  requires distinguishing threads that started at the winning position, which
  the current state representation cannot do. A trial implementation returned
  `(2, 3)` for `[a-c]+` on `xxabcxx`, and would have returned `(0, 2)` instead of
  `(0, 1)` for `a|ab` on `aab`. Shipping that would have traded a slow path for
  wrong spans.
- Caching failed scans per position: unsound, because a scan started later
  reaches different states.
- Doing nothing and documenting the quadratic case: the linearity claim is the
  project's premise, and the fix is small.

## Consequences

Worst case is O(n * m) from the PikeVM, plus a bounded 8x factor of wasted DFA
work before the abort. Patterns that trip the budget are slower than a native
unanchored DFA would be, but correct and linear.

A release-only test (`search_time_is_linear_in_haystack_length`) asserts that
four times the input costs less than eight times the time for `a+b`, `a+b$`, and
`(a|aa)+b`, so the regression cannot return unnoticed.

The real fix remains open: an unanchored forward DFA that can report the
leftmost-first end, which needs per-state awareness of the winning start.
