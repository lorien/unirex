# DFA Match Tracking and Assertions

Date: 2026-10-18
Status: accepted

## Context

ADR-0010 triggered the lazy DFA after the prefilter benchmark. The DFA must
preserve the engine's leftmost-first semantics, handle the current `Start` and
`End` assertions, stay bounded, and fall back to the PikeVM when it cannot
represent a pattern. The target is not only the Telegram pattern; the engine
must remain correct on every supported test case.

## Decision

- Partition the code-point alphabet into equivalence classes from every literal
  and class range. Build a class map for UCS-1, UCS-2, and UCS-4.
- A DFA state contains consuming NFA instructions in priority order, pending
  `Assert(End)` instructions in priority order, and a flag saying a match was
  reached at the current position.
- Compute epsilon closure in priority order. The first `Match` sets the flag and
  prunes all lower-priority states. A later match from a surviving
  higher-priority path overrides the previous match end.
- Resolve `Start` from whether the candidate is at position zero. Keep `End`
  assertions pending while the position is not the end, then resolve them in a
  final closure at the end of the haystack.
- Store the state and transition cache in `Scratch`, not in `Regex`, so threads
  do not contend. Cap the state cache at 4,096 states.
- On cache overflow, class-count overflow, or an out-of-contract UCS-4 unit,
  disable the DFA for that scratch and fall back to the PikeVM.
- Use the DFA only when a safe first-unit set exists. The existing prefilter
  selects candidates, and the DFA scans forward from each candidate. Patterns
  without a safe first set remain correct through the PikeVM.
- Keep the DFA as an optimization. The PikeVM remains the correctness
  authority for captures, cache overflow, and any construct outside the DFA
  path.

## Alternatives rejected

- An unanchored DFA that adds start states at every position: more complex
  leftmost and assertion context handling, and the prefilter already gives the
  target pattern a cheap candidate set.
- A DFA limited to assertion-free patterns: correct, but needlessly narrow.
  Start and end assertions are handled with position context.
- DFA for start and end plus an anchored PikeVM for every span: keeps a
  PikeVM scan per match and loses most of the DFA benefit for no-capture
  searches.
- A tagged DFA for captures: a larger design; captures remain on the anchored
  PikeVM.
- Reusing the byte-oriented `regex-automata` DFA: its alphabet and spans are
  UTF-8 bytes, not UCS code points.
- An unbounded DFA cache: violates the bounded-memory requirement.

## Consequences

- The DFA fast path requires a safe first-unit set. Broad, negated, and
  nullable patterns stay on the PikeVM.
- Word boundaries and flags arrive in milestone 2. The DFA must either learn
  their context or route those patterns to the PikeVM; it must not silently
  assume they are absent.
- The class maps cost a few megabytes per scratch. Sharing them across threads
  is a later optimization if the Python binding creates many scratches.
- Every DFA result must be checked against the PikeVM and CPython on the
  differential suites, including `Start`/`End` priority cases and cache
  overflow.
