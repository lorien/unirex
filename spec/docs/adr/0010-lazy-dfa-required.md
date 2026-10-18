# Lazy DFA After the Prefilter Benchmark

Date: 2026-10-18
Status: accepted

## Context

Milestone 3 added a conservative first-unit prefilter. It improved the
full-corpus Telegram benchmark from 5.424 s to 1.266 s at one thread, and to
0.413 s at four threads. The same-machine Rust `regex` bytes baseline is
0.179 s at one thread. The prefilter is doing its job: the scan for `@`
candidates is cheap, and about 77 percent of posts contain no candidate at
all. The remaining cost is the PikeVM work at each candidate and its per-call
overhead.

The engine is still about seven times slower than the mature bytes engine at
one thread and about 2.3 times slower at four threads. The target comparison
is single-threaded, so relying on threads does not close the gap. Further
PikeVM micro-optimizations are unlikely to recover a factor of seven.

## Decision

Implement milestone 4 now: a lazy DFA over code-point equivalence classes.

- Build equivalence classes from the class-range boundaries used by the
  pattern.
- Determinize on demand. A DFA state is a set of NFA states; cache transitions
  in a hash map with a fixed state cap.
- On cache overflow, clear or stop determinizing and fall back to the PikeVM.
- Use the DFA to find match start and end for no-capture searches.
- Keep the first-unit prefilter as the candidate selector for unanchored
  searches.
- For captures, run the PikeVM anchored over the matched window.
- Build the UCS alphabet and compiler in this crate. Do not reuse the
  byte-oriented `regex-automata` DFA.

## Alternatives rejected

- Continue tuning the PikeVM: the measured gap is too large, and the
  per-candidate thread-list work is structural.
- Add a pattern-specific literal fast path for `@username`: it would improve
  this benchmark without giving a general engine, and it duplicates work the
  DFA can do.
- Depend on four threads: the target baseline is single-threaded, and four
  threads are still slower.
- Skip the DFA and go straight to the Python binding: the end-to-end result
  would be dominated by an engine that is already far behind.
- A tagged DFA for captures: a different and larger design; the anchored
  PikeVM over a matched window is enough for milestone 4.
- Reuse `regex-automata`: its alphabet, spans, and byte classes are UTF-8
  oriented; a UCS compiler is still required.

## Consequences

The DFA adds determinization, cache memory, and a fallback path. The cache
must be bounded and tested, and every DFA result must be checked against the
PikeVM on the differential suites. The prefilter remains useful and remains
the first optimization for unanchored searches.
