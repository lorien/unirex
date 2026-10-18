# Architecture

## Current state

Milestone 1 is implemented: a standalone crate with a strict Python-dialect
subset, a Thompson program, and a group-0 PikeVM over UCS-1, UCS-2, and UCS-4.
Captures, the remaining Python classes and flags, the prefilter, the lazy DFA,
the benchmark, and the Python binding are later milestones.

## Compilation pipeline

1. Pattern text goes through a Python-dialect adapter. `\Z` is normalized to
   `\z`; numeric escapes are validated with Python's rule that `\1` and `\11`
   are backreferences outside a class but octal inside one, and that octal
   values stay within `0o377`; Rust-only syntax is rejected.
2. `regex-syntax` parses the normalized pattern into its AST. The adapter uses
   the AST, not HIR, because the AST preserves the distinction between `$` and
   `\Z` and lets the adapter reject Rust-only constructs before they acquire
   Rust semantics.
3. This crate validates the strict milestone 1 subset and lowers the AST into
   a `Program`: a flat instruction vector plus a class set. Case folding and
   Unicode categories are resolved at compile time in later milestones.
4. The PikeVM runs the program. A later milestone may add a lazy DFA for match
   start and end, with the PikeVM kept as the fallback and capture engine.

Compilation is a pure function of the pattern. A compiled `Regex` is immutable
and can be reused across haystacks and threads.

## Python dialect adapter

`regex-syntax` is a Rust-dialect parser, not a neutral syntax tree. Milestone 1
therefore accepts a strict subset and rejects everything else with a positioned
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
- `Assert(kind)` — zero-width assertion: start or end in milestone 1.
- `Match` — report the current position.

Repetition is compiled to `Split` and `Jump`; bounded `{n,m}` is unrolled with
a hard repetition limit. An empty-body repetition compiles to an epsilon loop
that the visited set terminates.

Milestone 2 adds `Save(slot)` for captures and assertions for `$` and Python
word boundaries.

## Classes

The canonical class form is a sorted, non-overlapping list of inclusive
`(u32, u32)` ranges, plus a negation flag. Membership uses binary search.
Milestone 1 lowers only explicit classes and `.`; per-kind accelerators
(bitmaps, two-level tables) are added with the prefilter in milestone 3.

Every accelerator must be derived from the same range list, so membership is
identical across UCS kinds. Class maps for the lazy DFA are separate and are
built only in milestone 4.

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

Milestone 3 derives the possible first code points from the pattern and scans
the UCS buffer before entering the engine. For ASCII first units in UCS-2 and
UCS-4, a raw-byte `memchr` with unit alignment and zero-byte checks is
effective. Multiple candidates use `memchr2` or `memchr3`, or a small set.
Non-ASCII first units are compared as units or with a range check.

A prefilter is an optimization only. It must never produce a false negative;
if extraction is uncertain, it is disabled for that pattern.

## Lazy DFA

Milestone 4 may add a lazy DFA over code-point equivalence classes:

- Collect all class-range boundaries used by the pattern and assign
  equivalence-class IDs.
- Build a per-kind class map: 256 entries for UCS-1, 64K entries or an
  interval map for UCS-2, interval search for UCS-4.
- Determinize on demand. A DFA state is a set of NFA states; transitions are
  cached in a hash map with a fixed state cap.
- On cache overflow, clear the cache and fall back to the PikeVM.
- Use the DFA to find match start and end. For captures, run the PikeVM
  anchored at the start over the matched window.

The byte-oriented `regex-automata` DFA is not reused. Its input is `&[u8]`,
its spans are byte offsets, its byte classes are 256 entries, and Unicode
classes are compiled to UTF-8 byte sequences. A UCS engine needs a different
alphabet and compiler.

## Python binding

Milestone 5 uses PyO3 and maturin. The binding:

- Detects and validates the PEP 393 layout of the running interpreter instead
  of hard-coding offsets; the `rurex` layout detector is the reference design.
- Obtains UCS kind, length, and data from a Python `str` only after
  validation. On failure it falls back to CPython's UTF-8 API or the existing
  `rurex` path.
- Builds result strings from the original UCS buffer, using
  `PyUnicode_Substring` where available and `PyUnicode_FromKindAndData` as the
  documented fallback.
- Matches under the GIL for short strings. For large haystacks it releases the
  GIL around the scan while keeping the `str` object alive, and never touches
  Python objects inside the detached region.
- Supports CPython 3.10 through 3.15, both GIL and free-threaded 3.14t.

## Memory and safety

- The core engine uses safe slices. The only `unsafe` is constructing slices
  from validated CPython pointers, with a documented safety argument.
- Compile limits bound pattern length, program size, repetition count, class
  count, and total class ranges.
- The DFA cache will have a hard state cap. The PikeVM thread lists and
  visited set are sized by the program, not by the haystack.
- Per-call allocation is avoided on the hot path; buffers are reused.

## Module layout

    src/lib.rs        public API, Match, Regex, Matches, MatchesWith
    src/ucs.rs        Ucs enum, kind, unit access, slicing
    src/compile.rs    Python-dialect adapter, AST validation, lowering
    src/pikevm.rs     Scratch, thread lists, epsilon closure, matching
    src/error.rs      error types and displays
    examples/bench.rs  self-contained Telegram benchmark
    examples/oracle.rs TSV oracle used by the Python differential harness
    tools/python_diff.py  randomized comparison against CPython re
    tests/m1.rs       accepted subset, Python spans, UCS kinds, adversarial
