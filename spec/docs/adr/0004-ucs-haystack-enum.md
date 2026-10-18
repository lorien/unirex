# UCS Haystack Enum

Date: 2026-10-18
Status: accepted

## Context

CPython stores a compact `str` in one of three unit widths. The matcher needs
a uniform view of all three without paying for a widening copy, and it must
not dispatch on the unit type for every character. The core crate also needs
to stay testable with plain Rust slices, away from CPython.

## Decision

Expose a borrowed enum at the outer API:

    pub enum Ucs<'a> {
        Ucs1(&'a [u8]),
        Ucs2(&'a [u16]),
        Ucs4(&'a [u32]),
    }

with `len()`, `get(i) -> u32`, `kind()`, and character-index slicing.
Monomorphize the hot search loop for the three unit types, or dispatch once
per search. The core accepts safe slices only. The binding detects and
validates the PEP 393 layout before constructing a `Ucs`, and falls back to
CPython's UTF-8 API or the existing `rurex` path if validation fails.

## Alternatives rejected

- A trait object over unit access: a virtual call per character in the hottest
  loop.
- A generic `Haystack` trait with static dispatch everywhere: workable, but it
  spreads the three-way type parameter through the whole API for no gain over
  an enum at the boundary.
- Always widening to `&[u32]`: removes the dispatch but allocates and copies
  the whole haystack, which is the cost the project exists to avoid.
- Keeping the engine generic over `u8`/`u16`/`u32` with `Into<u32>` bounds:
  the same monomorphization as the enum, with a less explicit public type and
  more trait machinery.
- Accepting raw bytes with alignment and endianness checks in the core: pushes
  unsafe and platform assumptions into the engine instead of the binding.
