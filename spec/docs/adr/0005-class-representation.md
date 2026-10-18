# Class Representation

Date: 2026-10-18
Status: accepted

## Context

Character classes, Unicode categories, case folding, and `\w` are resolved at
compile time through `regex-syntax` HIR. Matching then needs a fast "is this
code point in the class" test. The cost differs by haystack kind: a 256-entry
table is cheap for UCS-1, a 64K-entry table is 8 KB per class, and UCS-4 can
hold almost anything. A large pattern cache multiplies the memory.

## Decision

- The canonical form is a sorted, non-overlapping list of inclusive
  `(u32, u32)` ranges.
- Per kind, a class may carry an accelerator derived from the same ranges:
  - UCS-1: a 256-bit bitmap.
  - UCS-2: a 64K-bit bitmap (8 KB) when the pattern uses few classes,
    otherwise range search.
  - UCS-4: range search with binary search; a two-level BMP table may be added
    if profiling justifies it.
- Case folding, Unicode categories, and `\w` are expanded at compile time, not
  looked up per character.
- Per-pattern class-map memory is bounded by a compile limit. The DFA's
  equivalence-class maps are a separate structure built only in milestone 4.

## Alternatives rejected

- One representation for every kind: either the UCS-4 path becomes a 4 GB
  table or the UCS-1 path pays for range search it does not need.
- A 64K bitmap for every class on every kind: 8 KB per class is acceptable for
  one pattern and unacceptable for a cache of many patterns.
- `HashSet<u32>`: per-character hashing and poor cache behavior.
- Runtime Unicode property lookup: moves compile-time work into the hot loop
  and depends on a table version at match time.
- Storing UTF-8 byte sequences, as the `regex` crate does: the engine's
  alphabet is code points, not bytes.
