## Report on task: Implement self-contained Telegram benchmark

Ad-hoc work requested by the owner: make the benchmark portable and run it.

### Done

- Added reusable `Scratch` search buffers to `pikevm.rs` and the public API:
  `Regex::scratch`, `find_with`, `is_match_with`, and `find_iter_with`. The
  convenience `find` and `find_iter` still allocate a temporary scratch.
  Added a reuse test across UCS-1 and UCS-2 haystacks.
- Added `examples/bench.rs`, a self-contained engine-only benchmark:
  - Corpus resolution from `--corpus`, `RUREX_TG_DATA`, the verified
    `var/bench-data/tg-1m.json.gz` cache, or the public release URL.
  - Streaming gzip and JSONL parsing with `flate2` and `serde_json`.
  - Narrowest CPython kind per post, with flat UCS-1/2/4 buffers.
  - Modes `ucs`, `bytes`, and `str`; `--threads`, `--limit`,
    `--repeat`, `--json`, and `--self-test`.
  - Untimed loading and compilation; cold first run plus repeated runs;
    median and minimum; `black_box`.
  - Independent naive mention counter and full-corpus constants.
  - SHA-256 download verification with `ureq` and `sha2`.
- Added ADR-0009 and updated `benchmarks.md`, `dev.md`, `architecture.md`,
  `overview.md`, `testing.md`, `conventions.md`, `index.md`, `README.md`, and
  `plan.md`.
- Verified portability from `/tmp/exp_rurex_portable`: `--self-test` and a
  tiny `--corpus` file work with no `/web` path. Also tested download,
  checksum verification, and caching against a local HTTP server.
- Verified both published corpus checksums: gzip and uncompressed.

### Results: full corpus, 1M posts

Machine: Intel Core i5-8350U, 8 logical cores, Linux, Rust 1.97.1. All modes
reported 334,136 mentions in 186,226 posts.

- `ucs`, 1 thread: median 5.424 s, first 5.379 s, 5,424 ns/post.
- `ucs`, 4 threads: median 1.796 s.
- `bytes` (Rust `regex`), 1 thread: median 0.151 s, first 0.120 s.
- `str` (Rust `regex`), 1 thread: median 0.188 s.

Kind distribution: 210,313 UCS-1 posts, 331,272 UCS-2 posts, 458,415 UCS-4
posts. A forced UCS-2 benchmark would misrepresent about 46 percent of the
corpus.

Conclusion: the milestone 1 PikeVM is about 36 times slower than the mature
Rust `regex` bytes engine single-threaded. The four-thread PikeVM was 1.796 s;
no same-thread bytes baseline was measured in that session. A first-unit
prefilter skips most posts (77 percent contain no `@`),
but the remaining posts would still cost roughly 1.2 s at the measured
per-post rate. The prefilter is necessary but probably not sufficient; the
lazy DFA is likely required. Milestone 3 was raised to priority 4 to measure
the prefixed engine before deciding.

Raw JSON is under `var/bench-results/` and is intentionally not committed.

### Spec/ADR amendments

- [acted] Milestone 3 must add the prefilter for all three UCS kinds and
  rerun this harness. The prefilter was implemented and measured: 1.266 s at
  one thread and 0.413 s at four threads.
- [acted] If the prefixed PikeVM remains far from the bytes baseline,
  prioritize milestone 4 (lazy DFA) over milestone 2. It remains far: 1.266 s
  versus 0.179 s. ADR-0010 records the decision and milestone 4 is now
  priority 4.
- [acted] Add a benchmark mode that scans only posts containing `@`. The
  prefilter now skips directly to `@` candidates inside each post, which is
  stronger than a post-level mode.
- [open] The docs mention a synthetic corpus fallback, but the harness does
  not implement one. Either add it or remove the claim.
- [open] The full 1M-post run is too heavy for ordinary CI. Run the benchmark
  self-test in CI and the full corpus manually or nightly.
- [acted] At milestone 5, compare `--threads` results with GIL and
  free-threaded CPython. GIL 1-thread end-to-end was measured; GIL 4-thread
  runs were not meaningfully faster. Free-threaded verification remains a
  separate open task.

### Future-task notes

- [open] Extend the benchmark to report time per UCS kind, not only the total.
  This will show whether the UCS-4-heavy posts dominate.
- [open] Add an optional external-baseline ingestion path for separately
  produced `rurex` and CPython numbers. The required benchmark must remain
  self-contained.
- [acted] Keep the benchmark baselines in sync with the engine modes. The
  `at`/`memchr` diagnostic was later removed because it was not a regex
  engine or a baseline.

### Tooling/process

- [open] `ureq` and `sha2` add dev-only build weight. They are acceptable for
  a self-contained downloader, but revisit if build time becomes a problem.
- [open] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue; use the `rg` form from `testing.md`.
