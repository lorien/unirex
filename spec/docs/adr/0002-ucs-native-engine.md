# UCS-Native Engine

Date: 2026-10-18
Status: accepted

## Context

`rurex` matches UTF-8, while CPython stores `str` as UCS-1, UCS-2, or UCS-4
code units. On a cold non-ASCII `str`, `rurex` must borrow a cached UTF-8 copy
or transcode the buffer. On the Telegram corpus the cold transcode is roughly
38 percent of the str time, and CPython's `re` avoids it by reading the UCS
buffer directly. The project must determine whether a UCS-native engine can
be correct and fast enough to replace that path.

## Decision

Build a standalone Rust crate that:

- parses patterns with `regex-syntax` and lowers HIR into a small Thompson
  instruction set,
- runs a PikeVM over code points directly on UCS-1, UCS-2, and UCS-4 slices,
- keeps the core free of any Python dependency and free of UTF-8 conversion
  in the matching path,
- adds captures, a prefilter, an optional lazy DFA, and finally a PyO3 binding
  in that order.

The core is tested on plain slices before any Python integration. A compiled
program is immutable and reusable.

## Alternatives rejected

- Optimize the UTF-8 transcode further. `rurex` already has a SIMD
  transcoder; the copy remains and cannot be removed by tuning it.
- Reuse the `regex` crate's internal NFA. Its alphabet is UTF-8 bytes, its
  internals are private, and it cannot match UCS units natively.
- Reuse the `regex-automata` DFA. Its input is `&[u8]`, its spans are byte
  offsets, and Unicode classes are compiled to UTF-8 byte sequences; a UCS
  engine needs a different compiler and alphabet.
- Start with a DFA. Determinization is harder to get right and harder to
  debug; the PikeVM is the simplest engine that proves linear time and
  correctness.
- Write a pattern parser first. `regex-syntax` already provides HIR, Unicode
  classes, flag handling, and error positions.
