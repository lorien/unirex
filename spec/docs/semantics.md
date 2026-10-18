# Semantics

## Compatibility target

The target is Python `re` behavior for the supported syntax. During milestone
1 the differential oracle is the Rust `regex` crate on UTF-8 text decoded to
code points, because the engine is group-0-only and has no captures yet.
Milestone 2 switches the oracle to Python `re` on generated UCS-1, UCS-2, and
UCS-4 strings.

Where Python `re` and Rust `regex` disagree, Python wins for the final engine
unless a divergence is explicitly documented here. Milestone 1 differential
patterns avoid the known disagreements listed below.

## Supported syntax

- Literals, including escaped metacharacters.
- `.` — any code point except newline, or any code point with `s`.
- Character classes, including negated classes and the Unicode classes
  produced by `regex-syntax` HIR.
- Alternation.
- Greedy and lazy `*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}`.
- Anchors `^`, `$`, `\A`, `\z`, `\b`, `\B`.
- Capture groups and non-capturing groups.
- Flags `i` (ignore case), `m` (multiline), `s` (dot matches newline), `x`
  (verbose), including scoped inline groups.

Unsupported by design and rejected at compile time:

- Backreferences.
- Lookaround: `(?=...)`, `(?!...)`, `(?<=...)`, `(?<!...)`.
- Atomic groups.
- Possessive quantifiers.
- Conditional groups.
- Recursion.

`regex-syntax` performs the parse; the lowering stage rejects any HIR variant
it does not support. Nothing unsupported is silently approximated.

## Matching rules

### Leftmost-first

The engine reports the match that starts earliest. Among matches at the same
start, alternation order and greedy or lazy quantifiers decide, exactly as in
Python `re`. This is not POSIX leftmost-longest.

### Empty matches and iteration

Python 3.7 and later uses this rule: after an empty match at position `p`,
retry at `p` requiring a non-empty match; if that fails, advance one code
point and continue. Milestone 2 implements and tests this against `re`,
including patterns that can match empty at the start or end of the haystack.

### `$`

Python `re` lets `$` match before a trailing newline. Rust `regex` does not.
The final engine follows Python. Milestone 1's differential corpus avoids `$`
until the behavior is implemented and tested.

### `\b` and `\w`

Word characters come from the HIR Unicode word class, not from an ASCII
shortcut. A word boundary checks the code points on both sides of the current
position. If the class is too large to compile efficiently, the fallback is
documented; it is never a silent ASCII-only substitution.

### End-of-text spelling

The crate accepts `\z` as the end-of-text anchor, matching `regex-syntax`.
Python spells it `\Z` through 3.13 and accepts `\z` from 3.14. The Python
binding maps Python's `\Z` to the crate's `\z` and documents the mapping.

### Surrogates and UCS kinds

UCS-2 units are treated as individual code points, including lone surrogates.
Surrogates are not paired: CPython stores a real supplementary character as
UCS-4, so a valid surrogate pair cannot occur in compact UCS-2 storage. This
matches `re` and removes the `UnicodeEncodeError` that the UTF-8 `rurex` path
currently raises for lone surrogates.

UCS-4 values above U+10FFFF cannot occur in a valid CPython string. The core
does not validate every unit on the hot path; such values are outside the
contract. Classification never indexes out of bounds for them, and the Python
binding never passes them.

### Unicode version

`regex-syntax` and CPython may ship different Unicode tables. Any difference
that changes a match is treated as a divergence, documented here, and covered
by a test. The engine does not silently pick one table.

## Flags

- `i` — case-insensitive matching, resolved through HIR at compile time.
- `m` — `^` and `$` match at line boundaries.
- `s` — `.` matches newline.
- `x` — whitespace and `#` comments ignored outside classes.
- Scoped inline flags apply to their group and nest.

LOCALE and DEBUG are rejected. ASCII and UNICODE flags are not part of the
first milestones; the engine is Unicode-native for `str`. If they are added
later, their scoped behavior is recorded here and in an ADR.

## Known divergences to pin

- `$` before a trailing newline: Python yes, Rust `regex` no. Python wins.
- Capture values in unbounded loops: Python and Rust `regex` differ, for
  example `^((x|y)*)*` reports `""` in Python and `"xyy"` in Rust. Milestone 2
  targets Python and adds differential tests before accepting the behavior.
- Possessive quantifiers are rejected, not compiled as nested repetition.
- `\Z` and `\z`: the crate uses `\z`; the binding normalizes Python's `\Z`.
- Empty-match iteration follows Python 3.7 and later, not older behavior.
