# Architecture

## Current state

Milestones 1, 3, 4, and 5 are implemented: a standalone crate with a strict
Python-dialect subset, a Thompson program, a group-0 PikeVM, a first-unit
prefilter, a lazy DFA over UCS-1, UCS-2, and UCS-4, and a PyO3 binding.
Captures and the remaining Python classes and flags remain.

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

Compilation is a pure function of the pattern. A compiled `Regex` is immutable
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
`Regex::find_from`. It is never dispatched per character.

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
The engine lowers explicit classes and `.`. The first-unit prefilter uses
`memchr` for single units; per-kind class bitmaps and two-level tables remain
future optimizations.

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

`Regex::scratch()` allocates a `Scratch` holding the two thread lists, their
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

Milestone 2 adds slot arrays to the PikeVM. The leftmost-first rule is
enforced by thread order, not by post-processing spans. Capture values in
unbounded loops are matched against Python `re` and documented if a divergence
remains.

## Prefilter

Milestone 3 derives a conservative first-unit set from the NFA start state by
following epsilon transitions. It is disabled when `Match` is reachable
(nullable pattern), when a first class is negated, when the set covers more
than 1,024 code points, or when it has more than 16 ranges.

When the thread list is empty and no match is pending, the PikeVM jumps to the
next candidate instead of advancing one unit at a time. A single unit is
searched with `memchr` over the raw unit bytes; for UCS-2 and UCS-4 a byte hit
is checked against the full unit, so endianness and false hits inside other
units are handled. A small range set uses a scalar scan.

The prefilter is an optimization only. A false negative is a correctness bug,
so uncertain extraction disables it. The full benchmark improved from 5.424 s
to 1.266 s at one thread, which is not enough; see `benchmarks.md` and
ADR-0010.

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
- States and transitions are cached in `Scratch`, not in `Regex`, so workers
  do not contend. The state cache is capped at 4,096 states.
- On cache overflow, class-count overflow, or an out-of-contract UCS-4 unit,
  the DFA is disabled for that scratch and the PikeVM takes over.
- The DFA is an optimization, not the correctness authority. The PikeVM
  remains the general matcher for patterns without a safe first set, for
  captures, and for any construct outside the DFA path.

The full benchmark improved from 1.266 s to 0.360 s at one thread and from
0.413 s to 0.110 s at four threads. The Rust `regex` bytes baseline is 0.169 s
at one thread and 0.061 s at four threads; the DFA does not beat the
same-thread baseline. See `benchmarks.md`.

## Python binding

Milestone 5 uses PyO3 and maturin. `src/python.rs` exposes
`exp_rurex.compile(pattern)` and `exp_rurex.Regex` with `findall`, `find`, and
`is_match`.

- UCS access uses CPython's `PyUnicode_KIND`, `PyUnicode_DATA`, and
  `PyUnicode_GET_LENGTH`, not private PEP 393 offsets. `PyUnicode_READY` is
  called first for pre-3.12 interpreters.
- The binding constructs borrowed `Ucs` slices while the `Bound<PyString>` is
  alive. Strings are immutable; no Python object is touched in a detached scan.
- Result strings come from `PyUnicode_Substring` using match code-point
  offsets. There is no UTF-8 encode or decode in the result path.
- Each compiled `Regex` owns a scratch pool. A call takes a scratch, scans
  without holding the pool lock, and returns it.
- Short strings match under the GIL. Haystacks of at least 4,096 code units
  release the GIL around the scan and build result strings afterward.
- The extension is non-abi3 because the UCS data accessors are outside the
  limited API. Version-specific wheels are expected.
- Only group 0 is exposed. Patterns with capture groups return the whole match,
  not `re.findall`'s group tuples.

End-to-end benchmark, full corpus, cold fresh process, one thread, GIL:
`exp_rurex` was 0.596 s to 0.604 s per run, with a median of 0.598 s over five
runs. CPython `re` was 0.490 s to 0.648 s, median 0.501 s. The binding works,
but the engine is about 1.2 times slower than CPython `re`. See
`benchmarks.md`.

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

    src/lib.rs        public API, Match, Regex, Matches, MatchesWith
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
