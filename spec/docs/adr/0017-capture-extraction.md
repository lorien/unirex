# Capture Extraction Outside the DFA

Date: 2026-10-19
Status: accepted

## Context

Capture groups need slot arrays, and the lazy DFA is the fast path for locating
matches. Putting slot values into the DFA is not cheap: `StateData` is interned
by its exact content, so every distinct slot assignment becomes a distinct
state, against a 4,096-state cache cap and a 4 million transition-cell cap.
Merging assignments instead requires tag-set semantics with priority rules
subtle enough that a mistake yields silently wrong group values.

The alternative is to leave the DFA at group 0 and recover group spans
afterwards. The engine already knows the match span exactly: the prefilter
supplies the candidate start and the DFA supplies the end, so the recovery
window is the match itself, not the haystack.

Recovering that window with the PikeVM worked but measured 14.2 microseconds per
match on the Telegram pattern, because `{4,31}` unrolls into a large program and
the PikeVM simulates it at every position. Capturing `findall` was 6.9 times
slower than group-free.

## Decision

- Compile groups into `Save(slot)` instructions. The DFA treats `Save` as an
  epsilon edge, so slot markers never enter `StateData` and the group-0 path is
  unchanged, state count included.
- Recover slots over the match span only, with a bounded backtracker anchored at
  the known start and requiring the known end. Its step budget is
  `max(100_000, 200 * (span + 2) * instructions)`, capped at 1,000,000 steps.
- Bound recursion explicitly: spans above 1,024 units and recursion deeper than
  1,024 frames go straight to the PikeVM, because the backtracker's stack use
  otherwise tracks span length and overflowed the default stack in testing.
- Fall back to PikeVM slot extraction when the backtracker aborts or declines.
  Unmatched groups are `usize::MAX` in Rust and `""` in Python `findall`.

## Alternatives rejected

- Slots in the DFA state identity: state count multiplies against a 4,096 cap.
- Tagged DFA with tag sets: research-grade priority semantics, and a wrong merge
  silently produces wrong captures rather than an error or a fallback.
- PikeVM-only extraction: simplest, measured 14.2 microseconds per match and a
  6.9x slowdown on capturing `findall`.
- Backtracker only, without budget or depth bounds: unbounded on patterns like
  `(a|a)*b`, and stack exhaustion on long spans.
- Defer captures until the rest of milestone 2: pairing sessions and `findall`
  would remain unable to read a group.

## Consequences

The group-free path is untouched by construction. On 200,000 Telegram posts,
group-free `findall` runs at 409 ns/post against CPython `re` at 503, while
capturing `findall` runs at 625 ns/post against 531: the advantage is lost when
groups are read.

Backtracking cost is far below the PikeVM when it applies. Release
measurements, per match: 715 ns versus 38.8 microseconds on the 31-unit
Telegram span.

The fallback is linear in span times program size and is much slower: 91
microseconds on a 1,024-unit span, 5.6 milliseconds on 65,536 units, and 91.7
milliseconds on a 1,000,000-unit span. A single enormous match therefore has a
visible latency, which matters for patterns that capture long text even though
it does not affect the Telegram workload.

Every aborted-backtracker match pays the PikeVM cost, so patterns with deep
ambiguity are slow but correct rather than fast and wrong.
