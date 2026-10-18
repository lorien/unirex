# exp_rurex

Experimental linear-time regular-expression engine that matches directly
against CPython's PEP 393 UCS storage: UCS-1, UCS-2, and UCS-4, with no UTF-8
conversion in the matching path.

Milestones 1, 3, 4, and 5 are implemented: a standalone Rust crate with a
strict Python-compatible syntax subset, a first-unit prefilter, a lazy DFA,
and a PyO3 binding. Milestone 2 compatibility work remains. It is not a full
`re` replacement.

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
cargo run --release --example bench -- --mode ucs --limit 100000 --cpu-list 2 --warmup-seconds 1 --repeat 3 --min-efficiency 0.90
PYTHONPATH=var/py python3 tools/test_python_binding.py
PYTHONPATH=var/py python3 tools/bench_python.py --engine exp_rurex --cpu-list 2 --warmup-seconds 2 --repeat 5 --min-efficiency 0.90
```

The Python differential harness generates random patterns from the supported
subset and compares `find_iter` spans with CPython `re`.

## Benchmark

`examples/bench.rs` is a self-contained engine-only Telegram benchmark. It
downloads the corpus into `var/bench-data/` and verifies its SHA-256, or uses
`--corpus` / `RUREX_TG_DATA` for an offline copy. No other checkout is needed.

Modes: `ucs` (this engine), `bytes` and `str` (Rust `regex` baselines).

On `regexus`, benchmarks use the first four physical P-cores `0,2,4,6` and
hold the exclusive `/tmp/core4.lock` lease for the whole run.

Full-corpus results on `regexus` (Intel i5-13500, first four physical P-cores
`0,2,4,6`, one thread per core, SMT siblings idle, `/tmp/core4.lock` held):

- 1 thread: UCS 0.132 s, Rust `regex` bytes 0.071 s, str 0.071 s.
- 2 threads: UCS 0.069 s, bytes 0.038 s, str 0.038 s.
- 3 threads: UCS 0.047 s, bytes 0.027 s, str 0.027 s.
- 4 threads: UCS 0.037 s, bytes 0.021 s, str 0.021 s.

The DFA is 3.5x faster than the prefiltered PikeVM and scales 3.61x from one
to four P-cores. It remains about 1.86x slower than Rust `regex` bytes at one
thread and 1.74x slower at four.

Earlier laptop numbers were wall-clock only and are kept in
`spec/docs/benchmarks.md` as historical.

## Python binding

Build the extension with maturin:

```sh
python3 -m venv .venv
. .venv/bin/activate
pip install maturin
maturin develop --release
```

Alternatively, build with Cargo and place the shared object on `PYTHONPATH`:

```sh
cargo build --release --features extension-module
mkdir -p var/py
cp target/release/libexp_rurex.so var/py/exp_rurex.so
PYTHONPATH=var/py python3 tools/test_python_binding.py
```

Usage:

```python
import exp_rurex

rx = exp_rurex.compile(r"@[a-zA-Z][a-zA-Z0-9_]{4,31}")
rx.findall("hello @alice123")
rx.find("hello @alice123")
rx.is_match("hello @alice123")
```

End-to-end, one thread, GIL, full corpus, remote `regexus`:

- `exp_rurex`: median 0.231 s, minimum 0.229 s.
- CPython `re`: median 0.191 s, minimum 0.190 s.

`exp_rurex` is about 1.21x slower than CPython `re`. It is below the 0.75 s and
0.45 s targets on this machine, but the remote CPU is much faster than the
original laptop. GIL threads 2-4 do not scale.

## Not yet implemented

Capture values, Python `\w`/`\d`/`\s` classes, Python word boundaries, `$`,
flags, and lazy repetition. See `spec/docs/milestones.md`.
