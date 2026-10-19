## Report on task: Record the capture decision and characterise the fallback

### Done

- Added `spec/docs/adr/0017-capture-extraction.md`: why `Save` stays out of the
  DFA, the backtracker-first policy with PikeVM fallback, the rejected
  alternatives with their measured costs, and the consequences. Catalogued it in
  `spec/docs/index.md`.
- Fixed a defect found while planning: the backtracking budget
  `max(100_000, 200 * (span + 2) * instructions)` reached about 2.6 billion
  steps at the old 65,536-unit span limit, seconds of work for a single match.
  It is now capped at 1,000,000 steps through a named `backtrack_budget`
  function that has its own unit test.
- Fixed a second defect found by the new test: the backtracker is recursive, and
  a 2,000-unit span overflowed the stack in a debug build. Spans are now limited
  to 1,024 units and recursion to 1,024 frames; either bound sends the match to
  the PikeVM.
- Added `pathological_pattern_falls_back_and_stays_correct`, which forces the
  abort path with `(a|a)*b` over a 2,001-unit span and checks the group span
  against CPython `re`. The fallback path is now covered by a test.
- Added an ignored `capture_costs` test that prints per-match cost for both
  paths. Documented the command in `spec/docs/testing.md`.

### Measured

Per match, release build, single thread:

- 31-unit Telegram span: backtracker 715 ns, PikeVM 38.8 us.
- 1,024-unit span, `(a|a)*b`: backtracker 14.0 us (aborts and falls through),
  PikeVM 91.1 us.
- 65,536-unit span: backtracker declines in 62 ns, PikeVM 5.65 ms.
- 1,000,000-unit span: backtracker declines in 17 ns, PikeVM 91.7 ms.

End to end on 100,000 posts: group-free 383 ns/post against `re` at 576, and
capturing `findall` 697 ns/post against 570.

### Spec/ADR amendments

- [acted] ADR-0017 records the capture strategy and its rejected alternatives.
- [acted] `spec/docs/architecture.md` states the three backtracking bounds and
  the fallback cost, and `spec/docs/testing.md` documents the timing probe.

### Future-task notes

- [open] A single enormous match costs the PikeVM rate: 91.7 ms for a
  1,000,000-unit span. Patterns that capture long text therefore have a latency
  cliff. A resumable backtracker, or a reverse pass that narrows the capture
  window further, is the next optimisation if that matters.
- [open] Match objects and `finditer` remain the next user-visible step;
  `findall` is still the only way to read a group.
- [open] Capturing `findall` is 1.22 times slower than `re` on the corpus. The
  remaining cost is per-match extraction plus Python object building; the
  binding still collects spans into a `Vec` and clones slots per match.

### Tooling/process

- [acted] Writing the ADR first surfaced the budget arithmetic, and the new
  fallback test immediately exposed the stack overflow. Both defects came from
  work that was supposed to be documentation only.
- [acted] A short unit test over `backtrack_budget` is a better guard than
  inferring the cap from a timing run.
