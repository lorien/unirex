# Benchmark Protocol

Date: 2026-10-18
Status: accepted (the `at` diagnostic was removed after the DFA work)

## Context

The project needs a reproducible answer to one question: how fast is the
UCS-native engine on the real Telegram corpus, and how far is it from a
mature regex engine? Earlier discussions proposed relying on a local `rurex`
checkout for the corpus and baselines, which does not work when this
repository is deployed elsewhere. The benchmark also has to measure matching,
not allocation or corpus parsing, and it must not silently count the wrong
number of mentions.

## Decision

- Use a custom Rust example, `examples/bench.rs`, not a generic criterion
  benchmark. The workload is one pass over a fixed corpus, not a repeated
  microbenchmark with warm-up.
- Resolve the corpus in this order: `--corpus`, `UNIREX_TG_DATA`,
  `var/bench-data/tg-1m.json.gz`, then a download from the public release URL.
  Verify SHA-256 before use. The harness never depends on another checkout.
- Pack each post into the narrowest CPython storage kind and keep flat UCS
  buffers. Loading, packing, and pattern compilation are untimed.
- Scan every post exactly once per run. Report the cold first run, repeated
  warm runs, median, minimum, nanoseconds per post, and MB/s.
- Provide `ucs` for this engine, `bytes` and `str` for the Rust `regex`
  baselines, and `at` for a `memchr` lower bound. These are same-machine
  baselines and need no external repository.
- Compute an independent naive mention count during loading. Every engine
  mode must match it. For the full corpus, assert 334,136 mentions in 186,226
  posts.
- Default to one thread. `--threads N` splits posts into contiguous ranges;
  each worker gets its own `Scratch`. GIL and free-threaded comparisons belong
  to the Python binding milestone.
- Keep engine-only results separate from end-to-end Python results. The
  historical `rurex` and CPython numbers are external context, not a gate.

## Alternatives rejected

- Depend on a local `rurex` checkout for the corpus or baselines: breaks the
  benchmark on any other machine and couples two repositories.
- `criterion`: its warm-up and repeated-run model does not match a single
  cold pass over a 1M-post corpus, and it does not solve corpus resolution or
  correctness checks.
- Force every post to UCS-2: misrepresents CPython storage for ASCII and
  astral posts. The actual kind distribution is part of the result.
- Compare engine-only time directly with `rurex` end-to-end time: the latter
  includes Python call overhead, result construction, and the transcode this
  engine avoids.
- A Python-only harness: it would add a Python dependency to a Rust benchmark
  and still need a second implementation for engine access. The Python
  differential harness remains separate.
- Trust either the engine or a single baseline for the mention count: the
  naive counter is an independent oracle.
- Download without a checksum: an unreproducible or corrupted corpus would
  invalidate every result.
