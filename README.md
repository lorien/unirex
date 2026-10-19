# unirex

[![CI](https://github.com/lorien/unirex/actions/workflows/ci.yml/badge.svg)](https://github.com/lorien/unirex/actions/workflows/ci.yml)

An experimental linear-time regex engine that matches against CPython's PEP 393
UCS-1/UCS-2/UCS-4 string buffers directly, with no UTF-8 transcode and no
backtracking.

**Status: experimental.** The engine implements a strict Python syntax subset
with the `re` API surface: `compile`, `search`, `match`, `fullmatch`,
`findall`, `finditer`, `sub`, `subn`, `split`, `Match` objects, and a
`re.error`-compatible exception. Lookaround and pattern backreferences are
rejected with a positioned error rather than reinterpreted with Rust `regex`
semantics. The one-thread end-to-end time is at parity with
CPython `re` on the Telegram benchmark; four-thread scaling is worse than Rust
`regex`.

## Install

Published versions are **placeholders that reserve the name** and contain no
code:

- PyPI `unirex 0.0.1` installs no modules, so `import unirex` fails.
- crates.io `unirex 0.0.1` exposes no items.

They exist only so the names are not taken. The first functional release will
be `0.1.0` or later. Until then, build from this repository.

Rust:

```rust
use unirex::{Pattern, Ucs};

let re = Pattern::new("a+").unwrap();
let hay = Ucs::Ucs2(&[0x61, 0x61]);
let m = re.find(hay).unwrap();
assert_eq!((m.start(), m.end()), (0, 2));
```

`Ucs` treats every unit as one code point. UCS-2 lone surrogates are ordinary
code points and are not paired.

Python:

```sh
python3 -m venv .venv
. .venv/bin/activate
pip install maturin
maturin develop --release
```

```python
import unirex

rx = unirex.compile(r"@[a-zA-Z][a-zA-Z0-9_]{4,31}")
rx.findall("hello @alice123")  # ['@alice123']
rx.find("hello @alice123")
rx.is_match("hello @alice123")

unirex.match(r"(\d+)", "123x").span()   # (0, 3)
unirex.sub(r"(\w+)@(\w+)", r"\2:\1", "me@host")  # 'host:me'

grouped = unirex.compile(r"@([a-zA-Z])([a-zA-Z0-9_]{4,31})")
grouped.groups                 # 2
grouped.findall("hello @alice123")  # [('a', 'lice123')]
```

## Why UCS instead of UTF-8

CPython stores `str` as a contiguous array of UCS code units plus a `kind`
field. An engine that matches UTF-8 must either borrow CPython's cached UTF-8
copy or transcode; on a cold non-ASCII string the transcode is roughly 38
percent of `rurex`'s `str` time. CPython's own `re` reads the UCS buffer and
compares code points directly, with no encoding step.

Matching the UCS buffer directly also skips UTF-8 decoding when building
result strings and yields character offsets without conversion. On the
Telegram corpus, 77.2 percent of posts contain no `@`, so a first-unit
prefilter skips the bulk of the work.

## Supported syntax

- Literals and escaped metacharacters, `.`, and explicit character classes and
  ranges including negation.
- Alternation and grouping. Capture values are available through `findall`
  and the Rust slot API; match objects are not implemented yet.
- Greedy repetition: `*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}`.
- Anchors `^`, `\A`, `\Z`, `\z`.
- Escapes `\a \f \t \n \r \v`, `\xNN`, `\uNNNN`, `\UNNNNNNNN`, and Python
  octal escapes.
- CPython `\w`, `\d`, `\s` and negations, standalone or inside a bracket class,
  with tables generated from CPython `re` itself.
- Word boundaries `\b` and `\B`, and `$` before a final newline.
- Flags `i`, `m`, and `s`, global or scoped as `(?i:...)`, with case folding
  generated from CPython.
- Lazy repetition (`*?`, `+?`, `??`, `{n,m}?`), named groups
  (`(?P<name>...)`), and verbose mode `(?x)`.

Python numeric escape rules are enforced: outside a class, `\1` and `\11` are
backreferences and are rejected; inside a class they are octal. Octal values
above `0o377` are rejected.

## Rejected syntax

Every unsupported construct fails at compile time with a positioned error. It
is never silently reinterpreted with Rust `regex` semantics.

- `\N{...}`, `\p{...}`, brace hex such as `\x{41}`.
- Rust assertions `\b{...}`, `\<`, `\>`.
- Class set operators `&&`, `--`, `~~`, nested classes, POSIX classes.
- Backreferences, lookaround, atomic groups, possessive quantifiers,
  conditionals, recursion.

## Design

Patterns are parsed with `regex-syntax`'s AST. This crate validates the strict
Python subset and lowers the AST to a small Thompson program; it does not use
`regex-syntax` HIR, whose classes and assertions follow Rust `regex` semantics.

Matching is a lazy DFA over code points, fed by a unit-wide packed pair
first-unit prefilter. The PikeVM, with two thread lists and a sparse visited
set, remains the fallback and the group-0 engine. Group 0 is tracked without
capture slots. Complexity is O(n * m) in haystack length and program size, with
no backtracking. Compile size, repetition counts, class count, and class-range
count are capped.

The Python 3.7+ empty-match iteration rule is implemented: after an empty match
at position `p`, retry at `p` requiring a non-empty match, then advance one
code point.

## Status

| Milestone | State |
|---|---|
| 1. UCS haystack, Python subset, group-0 PikeVM | implemented |
| 2. Python classes, flags, captures | implemented except `x` |
| 3. Prefilter and benchmark | implemented |
| 4. Lazy DFA | implemented |
| 5. PyO3 binding and end-to-end benchmark | implemented, 3.10-3.15 and 3.14t/3.15t |
| Continuous integration | implemented |

Known gaps: the verbose-mode `[ ]` divergence and the absence of a published
functional release. See `spec/docs/milestones.md`.

## Performance

Telegram corpus, 1,000,000 posts, remote `regexus` (Intel i5-13500, first four
physical P-cores `0,2,4,6`, SMT siblings idle). Engine-only medians, measured by
`examples/bench.rs` calling the Rust crate directly, with no Python involved:

| Threads | unirex crate, UCS | Rust `regex` crate, bytes |
|---|---|---|
| 1 | 0.0690 s | 0.0712 s |
| 4 | 0.0258 s | 0.0209 s |

UCS is about 3.1 percent faster at one thread and 1.24x slower at four; it
scales 2.67x from one to four P-cores while bytes scales 3.41x.

End-to-end Python with the GIL, full corpus, one thread: the `unirex` Python
binding (installed wheel, `rx.findall(text)` per post) takes 0.1851 s median
and 0.1830 s minimum, against stdlib `re` at 0.1923 s median and 0.1899 s
minimum. GIL threads 2-4 do not scale meaningfully. These rows are the Python
binding, not the Rust crate, so they are not comparable with the table above.

The full matrix, charts, and methodology are in
[`docs/telegram-benchmark.md`](docs/telegram-benchmark.md).

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --release
python3 tools/python_diff.py --patterns 1000
cargo run --release --example bench -- --self-test
PYTHONPATH=var/py python3 tools/test_python_binding.py
```

The Python differential harness generates random patterns from the supported
subset and compares `find_iter` spans with CPython `re`. The engine-only
benchmark downloads the corpus into `var/bench-data/`, verifies its SHA-256,
and accepts `--corpus` or `UNIREX_TG_DATA` for an offline copy.

`spec/docs/` is the source of truth for the design, semantics, milestones,
benchmark protocol, and conventions; start at
[`spec/docs/index.md`](spec/docs/index.md). Working agreements and the task list
are in `spec/skills/` and `spec/docs/plan.md`.

## License

MIT. See [`LICENSE`](LICENSE).
