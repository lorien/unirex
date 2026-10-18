# Architecture

## Compilation pipeline

1. Pattern text is parsed by `regex-syntax` into HIR. `regex-syntax` is the
   parser; the project does not write its own.
2. HIR is lowered into a `Program`: a flat instruction vector plus a class
   set. Case folding, Unicode categories, and `\w`-style classes are resolved
   through HIR at compile time, not at match time.
3. The program is run by the PikeVM. A later milestone may add a lazy DFA for
   match start/end, with the PikeVM kept as the fallback and capture engine.

Compilation is a pure function of the pattern and flags. A compiled `Regex`
is immutable and can be reused across haystacks and threads.

## Haystack

The public haystack type is an enum, not a trait object:

    pub enum Ucs<'a> {
        Ucs1(&'a [u8]),
        Ucs2(&'a [u16]),
        Ucs4(&'a [u32]),
    }

It exposes `len()` in code units, `get(i) -> u32`, `kind()`, and slicing by
character index for building results. `get` widens a unit to `u32`. The hot
search loop is monomorphized for the three unit types, or the unit type is
dispatched once per search. It is never dispatched per character.

The core crate accepts safe slices only. The unsafe work of deriving those
slices from a Python `str` belongs to the binding (milestone 5) and is
validated there.

## Instruction set

The lowered program uses a small Thompson-style instruction set:

- `Char(u32)` — consume one code point equal to the given value.
- `Class(class_id)` — consume one code point in the compiled class.
- `Split(a, b)` — epsilon transition to either target, in priority order.
- `Jump(pc)` — epsilon transition to one target.
- `Save(slot)` — record the current position in a capture slot.
- `Assert(kind)` — zero-width assertion: start, end, line start, line end,
  word boundary, or not-word-boundary.
- `Match` — report the current position.

Repetition is compiled to `Split` and `Jump` with explicit counters when a
bounded `{n,m}` needs them. An empty-body repetition is compiled so that it
cannot loop forever without consuming input.

## Classes

The canonical class form is a sorted, non-overlapping list of inclusive
`(u32, u32)` ranges. Per haystack kind, a compiled class may also carry an
accelerator:

- UCS-1: a 256-bit bitmap.
- UCS-2: a 64K-bit bitmap (8 KB) when a pattern uses few classes, otherwise
  sorted ranges. The choice is per pattern and bounded by memory limits.
- UCS-4: sorted ranges with binary search; a two-level table for the BMP may
  be added if profiling shows it matters.

Every accelerator is derived from the same range list, so membership is
identical across kinds. Class maps for the lazy DFA are separate: they map
code units to equivalence-class IDs and are built only in milestone 4.

## PikeVM

The matcher is the standard linear-time PikeVM:

- Two thread lists, current and next.
- A sparse set of visited program counters prevents duplicate threads at one
  position.
- A thread holds a program counter and, when captures are enabled, a slot
  array of `2 * (groups + 1)` offsets.
- Epsilon closure follows `Split`, `Jump`, `Save`, and `Assert` without
  consuming input. Assertions read the haystack and the current position.
- Consuming instructions advance by one code unit.
- Thread order encodes leftmost-first priority: the earlier thread in the
  list wins when two threads reach `Match` at the same position.
- For an unanchored search, a new start thread is added at each position
  until a match is found. The current position is then completed to select
  the best match at the leftmost start.
- For group-0-only searches, slot tracking is skipped entirely.
- Allocations are bounded and reused per `Regex` or per search context.

Complexity is O(n * m) in haystack length and program size, with no
backtracking.

## Captures

Milestone 2 adds slot arrays to the PikeVM. The leftmost-first rule is
enforced by thread order, not by post-processing spans. Zero-width iteration
follows Python 3.7 and later: after an empty match at position `p`, retry at
`p` requiring a non-empty match, then advance one code point. The exact rule
and its tests are in `spec/docs/semantics.md`.

## Prefilter

Milestone 3 derives the possible first code points from the HIR and scans the
UCS buffer before entering the engine. For ASCII first units in UCS-2 and
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
- The DFA cache has a hard state cap. The PikeVM thread lists and visited set
  are sized by the program, not by the haystack.
- Per-call allocation is avoided on the hot path; buffers are reused.
- Compile-time limits bound pattern size, program size, and class-map memory.

## Module layout

    src/lib.rs        public API and re-exports
    src/ucs.rs        Ucs enum, unit access, slicing
    src/compile.rs    regex-syntax HIR to Program
    src/classes.rs    class ranges and per-kind membership
    src/pikevm.rs     thread lists, epsilon closure, matching
    src/prefilter.rs  required first-unit extraction and scan
    src/dfa.rs        optional lazy DFA (milestone 4)
    src/error.rs      error types and displays
