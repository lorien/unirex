## Report on task: Capture extraction without slowing the DFA

### Done

- Added `Inst::Save(slot)` markers around capture groups. The DFA treats them
  as epsilon edges, so capture instructions never enter a DFA state and the
  group-0 fast path is unchanged by construction.
- Added `Program::groups`, `Pattern::group_count`, and `Pattern::captures`, which
  fills `2 * (groups + 1)` slots for a match the DFA already located. Unmatched
  groups are `usize::MAX` in both slots.
- Recover group spans over the match span only, never over the haystack:
  a bounded backtracker anchored at the known start and requiring the known end,
  with an explicit step budget, falling back to the PikeVM slot machinery when
  the budget is exhausted or the span exceeds 65,536 units.
- Added PikeVM slot tracking (`CapLists`) with flat slot buffers, a fork arena,
  and a generation-marked seen array, used only by the fallback path.
- Exposed groups through the Python binding: `findall` returns `re`-compatible
  group tuples when the pattern has groups, a list of group strings for one
  group, and matched strings when it has none. Unmatched groups become `""`.
  Added a `groups` attribute.
- Reused the backtracking trail across calls through `Scratch`, removing one
  allocation per match.
- Added tests whose expected spans come from CPython `re`: 14 patterns covering
  nested groups, alternation priority, `(a|ab)` priority, optional groups,
  empty groups, bounded repetition, groups inside unbounded repetition, and the
  last-iteration rule. Plus UCS-2 and UCS-4 checks and an anchored-at-match
  check.
- Extended the Python binding smoke test to 23 patterns, including capture
  patterns and a detached long haystack with groups.
- Added `tools/bench_captures.py`, a diagnostic that compares `findall` with
  and without groups on the same posts.

### Measurements

Same 200,000 posts, one thread, release build, `var/py/unirex.so`:

- group-free `@[a-zA-Z][a-zA-Z0-9_]{4,31}`: 409 ns/post, `re` 503 ns/post.
- two groups `@([a-zA-Z])([a-zA-Z0-9_]{4,31})`: 625 ns/post, `re` 531 ns/post.
- three groups `@([a-zA-Z])([a-zA-Z0-9_]{4,31})_(x)?`: 733 ns/post, `re`
  524 ns/post.

The first version of this work used the PikeVM for extraction and cost 14.2 us
per match, making capturing `findall` 6.9 times slower than group-free. The
backtracker brought that to 0.48 us per match on a 29-unit haystack, a 29-fold
improvement, because it walks the span once instead of simulating the whole
program at every position.

### Honest conclusion

Group-free matching stays faster than `re`. Capturing `findall` is now 1.18
times slower than `re` and 1.40 times slower for the three-group pattern, so the
library loses its edge exactly when groups are used. Extraction costs roughly
0.2 to 0.6 microseconds per match on this corpus, which is the same order as the
Python object construction both engines pay.

### Spec/ADR amendments

- [acted] `spec/docs/architecture.md` now describes the capture design: DFA with
  epsilon `Save`, span-only recovery, bounded backtracker with PikeVM fallback.
- [acted] `spec/docs/semantics.md`, `spec/docs/milestones.md`, and
  `spec/docs/testing.md` no longer claim capturing is unimplemented, and the
  remaining gaps are named: match objects, `sub`, `split`, named groups.
- [acted] ADR needed for the capture strategy. The decision to keep `Save` out of
  the DFA and to prefer a bounded backtracker over the PikeVM is exactly the
  kind of tradeoff the ADR series exists for, and it is currently only recorded
  in `architecture.md` prose and this report.
  Resolved: ADR-0017 records the decision, the rejected alternatives, and the
  measured costs.

### Future-task notes

- [open] Match objects and `finditer` are the next user-visible step; `findall`
  is now the only way to read a group.
- [acted] The PikeVM fallback path has no performance characterisation. A pattern
  that forces backtracking to abort, such as `(a|a)*b` over a long span, should
  be measured and documented.
  Resolved: the ignored `capture_costs` test measures both paths at four span
  lengths, and the numbers are in ADR-0017 and `architecture.md`.
- [open] `captures()` re-runs extraction for every match even when the caller
  only wants spans. Rust `regex` avoids this with `find` versus `captures`; the
  Python binding now inherits the cost in `findall` because `re` semantics
  require the groups.
- [open] The Python binding collects spans into a `Vec` and clones slots per
  match before building objects. Driving the match loop and the capture pass in
  one core function would remove both allocations.

### Tooling/process

- [acted] Measuring before and after the extraction strategy was decisive: the
  PikeVM version looked correct and passed every test while being 6.9 times
  slower than group-free matching, which no correctness test would have caught.
- [acted] A temporary timing probe in the test module, removed before the
  commit, separated "PikeVM is slow here" from "slot bookkeeping is slow":
  8.7 us versus 14.2 us per call.
