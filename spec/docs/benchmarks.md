# Benchmarks

## Primary target

The `rurex` Telegram benchmark is the real-world target:

- Corpus: 1,000,000 posts, 526,880,801 uncompressed bytes, about 470 MB of
  UTF-8 text, mostly Arabic and Persian.
- Average length: about 426 code units per post.
- Mentions: 334,136 in 186,226 posts.
- Pattern: `@[a-zA-Z][a-zA-Z0-9_]{4,31}`.

The corpus is not committed. It is a GitHub release asset of the `rurex`
project:

- URL: `https://github.com/lorien/rurex/releases/download/bench-data-telegram-v1/tg-1m.json.gz`
- gzip size: 93,506,895 bytes.
- gzip SHA-256:
  `eb91db735fcf1a9e6096a59e959471cb9a3c4f41438f864e800cdb66f994023b`
- Uncompressed SHA-256:
  `02c51223e24db5b05acedd88ca6123167addeebc9328e450f7d0afd4bdec6c3d`

The harness resolves the corpus in this order: `$RUREX_TG_DATA` pointing at a
`.jsonl` or `.gz` file, then a cached copy at `var/bench-data/tg-1m.json.gz`,
then a checksum-verified download. It reads the gzip stream directly; the
527 MB file is never materialized.

If the corpus is unavailable, generate a synthetic corpus with a similar
code-point distribution and label every result as synthetic.

## What to measure

1. Engine-only time for `find` and `findall` over UCS-2.
2. End-to-end Python time, including result construction, once the binding
   exists.
3. Cold and warm runs separately.
4. GIL builds with 1 and 4 workers; free-threaded builds with 1 and 4 workers.
5. Engine time without the prefilter and with it, so the prefilter's
   contribution is visible.

Report raw numbers, repetition counts, machine, operating system, Rust
version, and the exact command. Summaries belong in the session report.

## Baselines

From `rurex` on the same corpus, GIL, one thread:

- `rurex` str: about 0.75 s.
- `rurex` bytes: about 0.35 s.
- `re` str: about 0.47 s.
- `re` bytes: about 0.51 s.

The cold transcode is roughly 38 percent of the `rurex` str time.

## Success criteria

- Beat 0.75 s end to end.
- Ideally approach 0.45 s.
- Do not regress ASCII `str`, bytes input, or complex patterns in the
  integration prototype.
- Correctness checks pass before any number is recorded.

## Additional patterns

The Telegram pattern is short-string dominated. Also measure:

- A literal scan over an 8 KB text.
- `\d{3,4}` extraction at several densities.
- A pattern with a large Unicode class.
- An anchored pattern and a pattern with alternation.

These prevent tuning the engine only for one corpus and expose per-kind
classification costs.

## Milestone order

Milestone 3 benchmarks the PikeVM and the prefilter. The result decides
whether milestone 4 adds a lazy DFA. Do not build the DFA before that
measurement.
