# Lazy DFA After Benchmark

Date: 2026-10-18
Status: accepted

## Context

The PikeVM is linear-time and simple to verify, but it is O(n * m) with a
thread list per position. A lazy DFA can be much faster on scan-heavy
patterns, at the cost of determinization work, cache memory, and a fallback
path when the cache fills. The project should not pay that complexity before
knowing whether the PikeVM is fast enough on the target corpus.

## Decision

Ship the PikeVM first. Build the lazy DFA only if the milestone 3 benchmark
shows it is needed. If built:

- Determinize on demand over code-point equivalence classes.
- Bound the DFA state cache with a fixed cap.
- On cache overflow, clear the cache and fall back to the PikeVM, or stop
  determinizing for that pattern.
- Use the DFA to find match start and end. For captures, run the PikeVM
  anchored at the start over the matched window.
- Never reuse the byte-oriented `regex-automata` DFA; build the UCS alphabet
  and compiler in this crate.

## Alternatives rejected

- DFA first: more complexity before the matching model and the benchmark are
  proven; harder to isolate correctness bugs.
- Never build a DFA: may leave performance on the table for scan-heavy
  patterns.
- An unbounded DFA cache: a pathological pattern can exhaust memory, violating
  the bounded-memory requirement.
- A tagged DFA for captures: a different design, and not needed for the first
  target patterns.
- Reusing `regex-automata`: its input, spans, and byte classes are UTF-8
  oriented, so only a small part could be reused and the compiler would still
  have to be rewritten.
