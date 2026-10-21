# 0019: Anchored-start fast rejection

Date: 2026-10-21

Status: accepted

## Context

CPython's `re` optimizes anchored patterns: `re.search(r'\Ay', s)` over a
10M-character haystack answers in microseconds, and `test_re.py` asserts the
timing bound directly (`test_search_anchor_at_beginning`, under 0.1 s for
`search`, `split`, `findall`, `finditer`, and `sub` on two patterns). The
unirex search loop instead ran the first-unit prefilter over the whole
haystack and advanced a candidate position past every unit, taking roughly
0.1 s per operation and 0.7 s for the test's ten calls.

## Decision

Compute a program-level `anchored_start` flag at compile time, and let it
short-circuit the unanchored search loop:

- `A[pc]` holds when every path from `pc` to `Match` crosses an
  `Assert(Start)` instruction. A path through a start assertion is anchored
  regardless of what follows; consuming and zero-width instructions inherit
  their successor's verdict; a split requires it on both branches; `Match`
  itself does not satisfy it. A fixpoint from `false` over the instruction
  list settles the flag; the program is start-anchored when `A[start]` holds.
  `^` lowers to `Assert(Start)` only without `MULTILINE`, so `(?m)^y` is
  correctly not anchored, and nullable prefixes (`a?^y`) and vacuous anchors
  (`y\A`) are handled by the path formulation rather than a syntax check.
- In `pikevm::find`, an anchored program skips the lazy-DFA candidate scan and
  returns `None` immediately when the search start has advanced past the
  first position. In `pikevm::find_pike`, the loop seeds threads only at the
  initial position, never applies the first-unit prefilter, and breaks when
  the seeded threads die without a match.

The flag is sound by construction: it is set only when no accepting run can
begin at a position other than the first, which is exactly the condition
under which skipping the rest of the haystack preserves results.

## Alternatives rejected

- Syntax-level detection (an AST walk looking for a leading `\\A` or `^`):
  nullable prefixes, alternations, and repetitions make the predicate easy to
  get wrong, and flags such as `MULTILINE` change what `^` lowers to. The
  instruction-level fixpoint has no such cases.
- Making the lazy DFA itself anchored: the candidate scan it replaces is the
  expensive part for rejection-heavy anchored patterns, and the PikeVM over a
  single start position is already constant work.
- Anchoring in the Python binding only: `find_iter`, the bytes iterators, and
  the capture paths all route through `pikevm::find`, so one flag covers
  every operation, including `split` and `sub`.
