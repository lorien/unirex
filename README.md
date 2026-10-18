# exp_rurex

Experimental linear-time regular-expression engine that matches directly
against CPython's PEP 393 UCS storage: UCS-1, UCS-2, and UCS-4, with no UTF-8
conversion in the matching path.

Milestones 1, 3, 4, and 5 are implemented: a standalone Rust crate with a
strict Python-compatible syntax subset, a unit-wide first-unit prefilter, a
lazy DFA, and a PyO3 binding. On the Telegram benchmark the one-thread
end-to-end time is at parity with CPython `re`. Milestone 2 compatibility work
remains, so it is not a full `re` replacement.

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
semantics. Matching is a lazy DFA over code points, fed by a packed pair
first-unit prefilter; the PikeVM with two thread lists and a sparse visited set
remains the fallback and group-0 engine. Group 0 is tracked without capture
slots.

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
python3 tools/telegram_benchmark.py --cpu-list 0,2,4,6 --repeat 5 --warmup-seconds 2
python3 tools/telegram_report.py --results-dir var/bench-results/telegram
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

The generated public report is
[`docs/telegram-benchmark.md`](docs/telegram-benchmark.md). It is produced by
`tools/telegram_benchmark.py`, which runs the engine-only and Python matrices
and then calls `tools/telegram_report.py`. The self-contained HTML twin is
written to `docs/telegram-benchmark.html` and is gitignored.

Full-corpus results on `regexus` (Intel i5-13500, first four physical P-cores
`0,2,4,6`, one thread per core, SMT siblings idle, `/tmp/core4.lock` held):

- 1 thread: UCS 0.0689 s, Rust `regex` bytes 0.0715 s, str 0.0715 s.
- 2 threads: UCS 0.0388 s, bytes 0.0380 s, str 0.0376 s.
- 3 threads: UCS 0.0296 s, bytes 0.0270 s, str 0.0271 s.
- 4 threads: UCS 0.0257 s, bytes 0.0209 s, str 0.0212 s.

UCS is about 3.6 percent faster than Rust `regex` bytes at one thread and
1.23x slower at four. It scales 2.68x from one to four P-cores, while bytes
scales 3.43x.

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

- `exp_rurex`: median 0.1887 s, minimum 0.1838 s.
- CPython `re`: median 0.1912 s, minimum 0.1905 s.

`exp_rurex` is 1.3 percent faster than CPython `re` at the median and 3.5
percent faster at the minimum. GIL threads 2-4 do not scale meaningfully.

## Not yet implemented

Capture values, Python `\w`/`\d`/`\s` classes, Python word boundaries, `$`,
flags, and lazy repetition. See `spec/docs/milestones.md`.
