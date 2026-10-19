# Architecture

## Current state

Milestones 1, 3, 4, and 5 are implemented: a standalone crate with a strict
Python-dialect subset, a Thompson program, a group-0 PikeVM, a unit-wide
first-unit prefilter, a lazy DFA over UCS-1, UCS-2, and UCS-4, and a PyO3
binding. On the Telegram benchmark the one-thread end-to-end time is at parity
with CPython `re` (0.185 s versus 0.192 s on `regexus`). Captures and the
remaining Python classes and flags remain.

## Compilation pipeline

1. Pattern text goes through a Python-dialect adapter. `\Z` is normalized to
   `\z`; numeric escapes are validated with Python's rule that `\1` and `\11`
   are backreferences outside a class but octal inside one, and that octal
   values stay within `0o377`; Rust-only syntax is rejected.
2. `regex-syntax` parses the normalized pattern into its AST. The adapter uses
   the AST, not HIR, because the AST preserves the distinction between `$` and
   `\Z` and lets the adapter reject Rust-only constructs before they acquire
   Rust semantics.
3. This crate validates the current strict subset and lowers the AST into
   a `Program`: a flat instruction vector plus a class set. Case folding and
   Unicode categories are resolved at compile time in later milestones.
4. The lazy DFA handles the no-capture fast path for patterns with a safe
   first-unit set. The PikeVM remains the fallback, the capture engine, and the
   general matcher.

Compilation is a pure function of the pattern. A compiled `Pattern` is immutable
and can be reused across haystacks and threads.

## Python dialect adapter

`regex-syntax` is a Rust-dialect parser, not a neutral syntax tree. The
current engine therefore accepts a strict subset and rejects everything else with a positioned
error:

- Accepted: literals, `.`, explicit classes and ranges, alternation, greedy
  repetition, `^`, `\A`, `\Z`, `\z`, grouping, fixed-width hex escapes, and
  Python octal escapes.
- Rejected: `$`, `\w`, `\W`, `\d`, `\D`, `\s`, `\S`, `\b`, `\B`, flags,
  lazy repetition, named groups, `\N{...}`, `\p{...}`, brace hex, Rust word
  assertions, class set operators, nested classes, POSIX classes, repetition
  of an assertion, and all by-design-unsupported constructs.

Milestone 2 adds Python's `\w`, `\d`, and `\s` class tables, Python word
boundaries, `$`, flags, lazy repetition, and captures. The class tables must
be generated from CPython data for a documented Unicode version; the engine
must not silently substitute Rust `regex` class definitions.

## Haystack

The public haystack type is an enum, not a trait object:

    pub enum Ucs<'a> {
        Ucs1(&'a [u8]),
        Ucs2(&'a [u16]),
        Ucs4(&'a [u32]),
    }

It exposes `len()` in code units, `get(i) -> u32`, `kind()`, and slicing by
character index for building results. `get` widens a unit to `u32`. The hot
search loop is monomorphized for the three unit types by dispatching once in
`Pattern::find_from`. It is never dispatched per character.

The core crate accepts safe slices only. The unsafe work of deriving those
slices from a Python `str` belongs to the binding (milestone 5) and is
validated there.

## Instruction set

The lowered program uses a small Thompson-style instruction set:

- `Char(u32)` — consume one code point equal to the given value.
- `Class(class_id)` — consume one code point in the compiled class.
- `Split(a, b)` — epsilon transition to either target, in priority order.
- `Jump(pc)` — epsilon transition to one target.
- `Assert(kind)` — zero-width assertion: start or end in the current subset.
- `Match` — report the current position.

Repetition is compiled to `Split` and `Jump`; bounded `{n,m}` is unrolled with
a hard repetition limit. An empty-body repetition compiles to an epsilon loop
that the visited set terminates.

Milestone 2 adds `Save(slot)` for captures and assertions for `$` and Python
word boundaries.

## Classes

The canonical class form is a sorted, non-overlapping list of inclusive
`(u32, u32)` ranges, plus a negation flag. Membership uses binary search.
The engine lowers explicit classes and `.`. The first-unit prefilter uses a
prepared packed pair scan for a single unit; per-kind class bitmaps and
two-level tables remain future optimizations.

Every accelerator must be derived from the same range list, so membership is
identical across UCS kinds. The lazy DFA builds separate equivalence-class
maps for UCS-1, UCS-2, and UCS-4.

## PikeVM

The matcher is the standard linear-time PikeVM:

- Two thread lists, current and next.
- A sparse set of visited program counters, keyed by a generation counter,
  prevents duplicate threads at one position.
- A thread holds a program counter and the start position of its search. This
  is the group-0 replacement for a full capture slot array.
- Epsilon closure follows `Split`, `Jump`, `Save`, and `Assert` without
  consuming input. Assertions read the current position and the haystack
  length.
- Consuming instructions advance by one code unit.
- Thread order encodes leftmost-first priority: the earlier thread in the
  list wins when two threads reach `Match` at the same position.
- For an unanchored search, a new start thread is added at each position
  until a match is found. The current position is then completed to select
  the best match at the leftmost start.
- Allocations are bounded and reused per search.

Complexity is O(n * m) in haystack length and program size, with no
backtracking.

### Reusable scratch

`Pattern::scratch()` allocates a `Scratch` holding the two thread lists, their
visited sets, and the epsilon-closure stack. `find_with`, `is_match_with`, and
`find_iter_with` reuse it across searches. The convenience `find` and
`find_iter` allocate a temporary `Scratch` per call.

A `Scratch` is not shared between threads. The benchmark gives each worker its
own. Reusing scratch is required for meaningful engine measurements: without
it, per-search allocation dominates the scan for short posts.

### Empty matches

`find_iter` implements the Python 3.7+ rule. After an empty match at position
`p`, the next search starts at `p` but ignores an empty match exactly at `p`.
This allows a non-empty alternative at the same position to be found, then
advances one code point when none exists. The test suite checks the rule
against CPython spans.

## Captures

The lazy DFA tracks group 0 only, which is what keeps its state space small:
`Save` instructions are epsilon edges and never enter a DFA state. Group spans
are recovered afterwards, over the match span alone.

`Pattern::captures` runs a bounded backtracker anchored at the known match start
and requiring the known match end. That is fast because the span is short and
the search is anchored, and it is bounded by an explicit step budget. When the
budget is exhausted, or the span exceeds the backtracker limit, the same span is
re-derived with the PikeVM slot machinery instead, which is linear but
O(span * program).

The backtracker is bounded three ways: spans above 1,024 units are declined, a
step budget of `max(100_000, 200 * (span + 2) * instructions)` capped at
1,000,000 steps stops runaway searches, and recursion is limited to 1,024
frames because stack use otherwise grows with span and overflowed the default
stack in testing. Any of those bounds sends the match to the PikeVM path.

The fallback is linear but expensive: measured per match, 91 microseconds on a
1,024-unit span and 91.7 milliseconds on a 1,000,000-unit span, against 715
nanoseconds for a 31-unit span on the backtracker. See ADR-0017.

The leftmost-first rule is enforced by branch order in the backtracker and by
thread order in the PikeVM, never by post-processing spans. Capture values in
unbounded loops are matched against Python `re` and documented if a divergence
remains.

## Search bound

The DFA searches by taking a first-unit candidate from the prefilter and running
an anchored scan from it. Restarting that scan per candidate is quadratic when
scans survive to the end of the haystack, so the loop accumulates the units each
scan walked and aborts to the PikeVM once the total exceeds `8 * (len + 1)`.
That keeps search time linear in haystack length with the PikeVM's O(n * m) bound
as the worst case. See ADR-0018.

## CPython class tables

`\w`, `\d`, and `\s` come from tables generated by
`tools/gen_unicode_tables.py`, which scans every code point with CPython's own
`re` and emits sorted ranges into `src/unicode_tables.rs`. The file records the
CPython and Unicode versions, and CI runs the generator with `--check` so a
stale table cannot land.

Word boundaries and `$` depend on the unit before or after the current
position. `StateData` carries only the current position's state, so a program
containing either assertion sets `Program::uses_context` and the DFA declines
it, leaving those patterns on the PikeVM. Teaching the DFA to carry the
previous unit's word-ness is an open task; until then `\b` and `$` patterns
are correct but lose the DFA fast path.

## Prefilter

Milestone 3 derives a conservative first-unit set from the NFA start state by
following epsilon transitions. It is disabled when `Match` is reachable
(nullable pattern), when a first class is negated, when the set covers more
than 1,024 code points, or when it has more than 16 ranges.

When the thread list is empty and no match is pending, the PikeVM jumps to the
next candidate instead of advancing one unit at a time. For a single-unit
first set, `FirstSet::One` stores the native-endian unit needle and prepares a
packed pair byte finder per UCS width at compile time. A `Scan` borrows that
finder and searches the raw unit bytes in one pass; each candidate is checked
for unit alignment and exact unit equality, so false hits inside other units
are discarded. The short tail below the vector finder's minimum length is
scanned one unit at a time. On a CPU without the finder, the scan falls back to
a scalar unit search. A small range set still uses a scalar scan. See
ADR-0015.

The prefilter is an optimization only. A false negative is a correctness bug,
so uncertain extraction disables it. The full benchmark improved from 5.424 s
to 1.266 s at one thread before the DFA; the unit-wide scan and DFA together
bring the one-thread engine time to 0.0690 s on `regexus`. See
`benchmarks.md`, ADR-0010, and ADR-0015.

## Lazy DFA

Milestone 4 adds a lazy DFA over code-point equivalence classes. It is used
when a safe first-unit set exists; the prefilter supplies candidate starts and
the DFA scans forward from each candidate.

- Every literal and class range contributes boundaries. Each interval between
  boundaries is one equivalence class.
- Class maps are built for UCS-1 (256 entries), UCS-2 (65,536 entries), and
  UCS-4 (0x110000 entries). A map from a pattern that exceeds 65,535 classes
  disables the DFA.
- A DFA state holds consuming NFA instructions in priority order, pending
  `Assert(End)` instructions in priority order, and a match flag.
- Epsilon closure runs in priority order. The first `Match` marks the current
  position and prunes lower-priority states. A later match from a surviving
  higher-priority path overrides the previous end.
- `Assert(Start)` is resolved from whether the candidate is at position zero.
  `Assert(End)` is kept pending until the final position, then resolved in a
  final closure.
- States and transitions are cached in `Scratch`, not in `Pattern`, so workers
  do not contend. Transition rows are stored in one flat `Vec<u32>` indexed by
  `state_id * num_classes + class_id`, and a parallel compact match-flag array
  keeps the per-unit loop out of the state vector. The state cache is capped at
  4,096 states.
- On cache overflow, class-count overflow, or an out-of-contract UCS-4 unit,
  the DFA is disabled for that scratch and the PikeVM takes over.
- The DFA is an optimization, not the correctness authority. The PikeVM
  remains the general matcher for patterns without a safe first set, for
  captures, and for any construct outside the DFA path.

The local benchmark improved from 1.266 s to 0.360 s at one thread. After
caching initial DFA states, flattening transition rows, adding the match-flag
array, and switching the prefilter to a unit-wide scan, the remote `regexus`
benchmark reached 0.0690 s at one thread and 0.0258 s at four. Rust `regex`
bytes is 0.0712 s at one thread and 0.0209 s at four. UCS is slightly faster
than bytes at one thread, but 1.24 times slower at four because each worker
scans wider UCS storage and keeps its own DFA cache. See `benchmarks.md`.

## Python binding

Milestone 5 uses PyO3 and maturin. `src/python.rs` exposes
`unirex.compile(pattern)` and `unirex.Pattern` with `findall`, `find`, and
`is_match`.

- UCS access uses CPython's `PyUnicode_KIND`, `PyUnicode_DATA`, and
  `PyUnicode_GET_LENGTH`, not private PEP 393 offsets. `PyUnicode_READY` is
  called first for pre-3.12 interpreters.
- The binding constructs borrowed `Ucs` slices while the `Bound<PyString>` is
  alive. Strings are immutable; no Python object is touched in a detached scan.
- Result strings come from `PyUnicode_Substring` using match code-point
  offsets. There is no UTF-8 encode or decode in the result path.
- Each compiled `Pattern` has a stable pattern id. A thread-local cache keeps
  one `Scratch` per pattern per thread, capped at four patterns. A call takes
  the cached scratch, scans without a lock, and returns it. Dropping the
  `Pattern` clears its current thread's entry; other threads release their entry
  at thread exit or when the cache evicts it. See ADR-0016.
- `findall` appends result strings directly to a Python list instead of
  building an intermediate `Vec`.
- Short strings match under the GIL. Haystacks of at least 4,096 code units
  release the GIL around the scan and build result strings afterward.
- The extension is non-abi3 because the UCS data accessors are outside the
  limited API. Version-specific wheels are expected.
- `finditer` yields `Match` objects with `span`, `start`, `end`, `group`,
  `groups`, and `groupdict`; spans and slots are collected up front, so a
  `Match` never holds a scratch or a haystack borrow.
- `findall` returns `re`-compatible group tuples when the pattern has groups,
  and matched strings when it has none. Capture extraction happens while the
  match is produced, over the match span only, so it costs work proportional to
  matched text rather than to haystack length.

End-to-end benchmark on `regexus`, full corpus, one thread, GIL:
`unirex` was 0.1851 s median, 0.1830 s minimum; CPython `re` was 0.1923 s
median, 0.1899 s minimum. The engine is now at parity with, and slightly
faster than, CPython `re` on this benchmark. GIL threads 2-4 do not scale
meaningfully. See `benchmarks.md`.

## Memory and safety

- The core engine uses safe slices. The only `unsafe` is constructing slices
  through CPython's documented UCS accessors, with a safety argument tied to
  the live `Bound<PyString>` and string immutability.
- Compile limits bound pattern length, program size, repetition count, class
  count, and total class ranges.
- The DFA cache has a hard state cap. The PikeVM thread lists and visited set
  are sized by the program, not by the haystack.
- Per-call allocation is avoided on the hot path; buffers are reused.

## Module layout

    src/lib.rs        public API, Match, Pattern, Matches, MatchesWith
    src/ucs.rs        Ucs enum, kind, unit access, slicing
    src/compile.rs    Python-dialect adapter, AST validation, lowering
    src/pikevm.rs     Scratch, thread lists, epsilon closure, matching
    src/dfa.rs        lazy DFA and PikeVM fallback
    src/python.rs     PyO3 module and CPython UCS access
    src/error.rs      error types and displays
    examples/bench.rs  self-contained Telegram engine-only benchmark
    examples/oracle.rs TSV oracle used by the Python differential harness
    tools/bench_python.py  end-to-end Python benchmark
    tools/test_python_binding.py  binding smoke test against re
    tools/python_diff.py  randomized comparison against CPython re
    tests/m1.rs       accepted subset, Python spans, UCS kinds, adversarial
