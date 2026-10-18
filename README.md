# exp_rurex

Experimental linear-time regular-expression engine that matches directly
against CPython's PEP 393 UCS storage: UCS-1, UCS-2, and UCS-4, with no UTF-8
conversion in the matching path.

Milestones 1, 3, and 4 are implemented: a standalone Rust crate with a
strict Python-compatible syntax subset, a first-unit prefilter, and a lazy
DFA. Milestone 2 compatibility work and the milestone 5 Python binding
remain. It is not a full `re` replacement.

## Supported syntax

- Literals and escaped metacharacters.
- `.`
- Explicit character classes and ranges, including negation.
- Alternation.
- Greedy repetition: `*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}`.
- Anchors `^`, `\A`, `\Z`, `\z`.
- Grouping; capture groups are accepted but only group 0 is exposed.
- Escapes `\a \f \t \n \r \v`, `\xNN`, `\uNNNN`, `\UNNNNNNNN`, and Python
  octal escapes.

Python numeric escape rules are enforced: outside a class, `\1` and `\11`
are backreferences and are rejected; inside a class they are octal. Octal
values above `0o377` are rejected.

## Rejected syntax

Unsupported syntax is rejected with a positioned error, never reinterpreted
with Rust `regex` semantics:

- `\w`, `\W`, `\d`, `\D`, `\s`, `\S`, `\b`, `\B`.
- `$`.
- Flags `i`, `m`, `s`, `x`, and inline flags.
- Lazy repetition.
- `\N{...}`, `\p{...}`, brace hex such as `\x{41}`.
- Rust assertions `\b{...}`, `\<`, `\>`.
- Class set operators `&&`, `--`, `~~`, nested classes, POSIX classes.
- Named capture groups.
- Backreferences, lookaround, atomic groups, possessive quantifiers,
  conditionals, recursion.

## Design

Patterns are parsed with `regex-syntax`'s AST. This crate validates the strict
Python subset and lowers the AST to a small Thompson program; it does not use
`regex-syntax` HIR, whose classes and assertions follow Rust `regex`
semantics. Matching is a PikeVM over code points with two thread lists and a
sparse visited set. Group 0 is tracked without capture slots.

Complexity is O(n * m) in haystack length and program size, with no
backtracking. Compile size, repetition counts, class count, and class-range
count are capped.

The Python 3.7+ empty-match iteration rule is implemented: after an empty
match at position `p`, retry at `p` requiring a non-empty match, then advance
one code point.

## Usage

```rust
use exp_rurex::{Regex, Ucs};

let re = Regex::new("a+").unwrap();
let hay = Ucs::Ucs2(&[0x61, 0x61]);
let m = re.find(hay).unwrap();
assert_eq!((m.start(), m.end()), (0, 2));
```

`Ucs` treats every unit as one code point. UCS-2 lone surrogates are ordinary
code points and are not paired.

## Commands

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --release
python3 tools/python_diff.py --patterns 1000
cargo run --release --example bench -- --self-test
cargo run --release --example bench -- --mode ucs --limit 100000 --repeat 3
```

The Python differential harness generates random patterns from the supported
subset and compares `find_iter` spans with CPython `re`.

## Benchmark

`examples/bench.rs` is a self-contained engine-only Telegram benchmark. It
downloads the corpus into `var/bench-data/` and verifies its SHA-256, or uses
`--corpus` / `RUREX_TG_DATA` for an offline copy. No other checkout is needed.

Modes: `ucs` (this engine), `bytes` and `str` (Rust `regex` baselines).

Full-corpus results, Intel i5-8350U, Rust 1.97.1. Compare only the same
thread count.

One thread:

- PikeVM: 5.424 s.
- Prefilter: 1.266 s.
- Lazy DFA: 0.360 s.
- Rust `regex` bytes: 0.169 s.
- Rust `regex` str: 0.185 s.

Four threads:

- Prefilter: 0.413 s.
- Lazy DFA: 0.110 s.
- Rust `regex` bytes: 0.061 s.
- Rust `regex` str: 0.061 s.

The DFA is 3.5x faster than the prefiltered PikeVM. It is about 2.1x slower
than the bytes baseline at one thread and 1.8x slower at four threads. It does
not beat the same-thread baseline. The next step is the Python binding and the
end-to-end comparison.

## Not yet implemented

Capture values, Python `\w`/`\d`/`\s` classes, Python word boundaries, `$`,
flags, lazy repetition, and the PyO3 binding. See
`spec/docs/milestones.md`.
