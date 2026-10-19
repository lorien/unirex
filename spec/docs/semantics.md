# Semantics

## Compatibility target

The target is Python `re` behavior for the supported syntax. The current
engine accepts a strict subset and rejects everything else, so no accepted
pattern can silently acquire Rust `regex` semantics.

The oracle for the accepted subset is CPython `re` itself. `tests/m1.rs` pins
spans that were generated with CPython, and `tools/python_diff.py` compares
random patterns and haystacks against `re`. The Rust `regex` crate is used
only for the smaller common subset where both engines agree.

Milestone 2 adds the remaining Python classes, flags, `$`, and word
boundaries, and switches the full differential suite to Python `re` on
generated UCS-1, UCS-2, and UCS-4 strings.

## Current supported subset

Accepted:

- Literals and escaped metacharacters.
- `.`
- Explicit character classes and ranges, including negation.
- Alternation.
- Greedy `*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}`.
- Anchors `^`, `\A`, `\Z`, `\z`.
- Grouping: `(...)`, `(?:...)`, and capture groups. Capture values are exposed
  through the Python `findall` method and the Rust `Regex::captures` slot API;
  `Match` itself still reports group 0 only.
- Escapes `\a \f \t \n \r \v`, `\xNN`, `\uNNNN`, `\UNNNNNNNN`, and Python
  octal escapes.
- CPython `\w`, `\d`, `\s` and their negations, both standalone and inside a
  bracket class. The tables are generated from CPython `re` itself by
  `tools/gen_unicode_tables.py`, so they follow the CPython Unicode version
  rather than Rust `regex`.
- Word boundaries `\b` and `\B`.
- `$`: end of the haystack, or just before a final newline.
- Flags `i`, `m`, and `s`, global at the start of the pattern or scoped as
  `(?i:...)`. Case folding uses tables generated from CPython `re.IGNORECASE`,
  so special pairs such as `k`/`\u212A`, `s`/`\u017F`, and `i`/`\u0130`
  behave like CPython.
- Lazy repetition `*?`, `+?`, `??`, and `{n,m}?`.
- Named groups `(?P<name>...)`, exposed through the Python `Match` API and
  `Regex::capture_names`.
- Known divergence: with verbose mode set, `regex-syntax` strips whitespace
  inside character classes, so `[ ]` raises a parse error where Python accepts
  it. Whitespace is only stripped outside classes in CPython.
- Verbose mode `x`, global or scoped. ASCII whitespace and `#` comments are
  stripped outside classes; escaped whitespace such as `\ ` stays literal.

Rejected with a positioned error:

- Rust-style `(?<name>...)` group syntax.
- `\N{...}`, `\p{...}`, and brace hex such as `\x{41}`.
- Rust assertions `\b{...}`, `\<`, `\>`.
- Class set operators `&&`, `--`, `~~`, nested classes, and POSIX classes.
- Repetition of an assertion, for example `^*` or `\A*`.
- Backreferences, lookaround, atomic groups, possessive quantifiers,
  conditionals, and recursion.

## Matching rules

### Leftmost-first

The engine reports the match that starts earliest. Among matches at the same
start, alternation order and greedy quantifiers decide, as in Python `re`.
This is not POSIX leftmost-longest.

### Empty matches and iteration

Python 3.7 and later uses this rule: after an empty match at position `p`,
retry at `p` requiring a non-empty match; if that fails, advance one code
point and continue. `find_iter` implements this rule and is tested against
CPython spans, including `x*` on `"ab"` and `|a` on `"a"`.

### `$`

`$` is currently rejected. Python lets it match before a trailing
newline; Rust `regex` does not, and HIR `EndLF` matches before any newline.
Milestone 2 implements Python `$` with a dedicated assertion.

### `\b` and `\w`

Currently rejected. They use Python's word definition, which is not the
same as Rust `regex`'s: Rust includes combining marks, connector punctuation,
and join controls, while Python means Unicode alphanumerics plus `_`.
Milestone 2 generates Python-equivalent tables and uses them for `\w`, `\b`,
`\B`, and `\d`/`\s` where needed.

### End-of-text spelling

The crate accepts both `\Z` and `\z` as the end-of-text anchor. Python 3.10
through 3.13 spell it `\Z`; Python 3.14 also accepts `\z`. The adapter
normalizes `\Z` to `\z` before parsing. `\Z` matches only at the end, not
before a trailing newline.

### Numeric escapes

Python's rule is enforced:

- Outside a character class, `\1` and `\11` are backreferences and are
  rejected. A backslash followed by exactly three octal digits, such as
  `\111`, is an octal escape.
- Inside a character class, numeric escapes are octal regardless of the
  number of digits, so `[\1]` matches U+0001 and `[\11]` matches U+0009.
- An octal escape whose value exceeds `0o377` is rejected, as in Python.
- `\8` and `\9` are rejected.

`\xNN`, `\uNNNN`, and `\UNNNNNNNN` are accepted. Brace forms such as
`\x{41}` are rejected because Python does not accept them.

### Character classes

The current subset supports literal characters and ranges only. Python treats
`&&`, `--`, `~~`, nested `[`, and POSIX forms as literals or as ordinary
class contents; Rust `regex` gives them set-operation meanings. They are
rejected instead of being reinterpreted. `[a&b]` and `[a~b]` are accepted
because a single `&` or `~` is a literal in both dialects.

`[]]`, `[^]]`, `[a-]`, and `[-a]` follow Python's literal-bracket and
literal-hyphen rules. `[\b]` is rejected even though Python treats it as a
backspace, because the current engine does not implement class escapes beyond the
numeric and fixed-width forms above.

### Repetition

Only greedy repetition is accepted. Bounded repetition is unrolled under a
hard limit. A direct repetition of an assertion is rejected, matching
Python's "nothing to repeat" error. `a{,2}` is rejected for now; Python 3.11
and later accept it, and it is not part of the current subset.

`(?#...)` comments are not accepted by the AST parser and are outside the
subset.

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

The current engine has no Unicode property tables: `\p{...}` is rejected and
explicit classes contain exactly the code points written in the pattern.
There is therefore no Unicode-version dependence until milestone 2 adds
Python class tables. Those tables must be generated for a documented CPython
Unicode version, and any version-dependent divergence must be recorded here.

## Deferred Python behavior

These are intentionally not part of the current subset and are tracked for
milestone 2 or later:

- `$` before a trailing newline.
- Python `\w`, `\d`, `\s`, and word boundaries.
- Flags `i`, `m`, `s`, `x`, including scoped flags.
- Lazy repetition and named capture groups.
- Match objects, `sub`, `split`, and named capture groups. Capture values work
  for `findall` only, including the last-iteration rule in unbounded loops.
- `a{,2}` and `(?#...)`.
- `\N{...}` named character escapes.
- ASCII/UNICODE flag semantics.
