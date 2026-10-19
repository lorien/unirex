## Report on task: Optimize prefilter and DFA for re parity

### Done

- Profiled the engine on the cached 400k-post slice. The old first-unit
  prefilter used byte-wise `memchr` over UTF-16/UTF-32 storage and cost about
  0.028 s for UCS-2 and 0.051 s for UCS-4 out of about 0.091 s for the whole
  pattern scan.
- Replaced it with a prepared, unit-wide packed pair scan. `FirstSet::One`
  now stores a native-endian unit needle and a packed pair finder per UCS
  width. `Scan` borrows the finder for the whole search, verifies unit
  alignment and the full unit after each candidate, and scans short tails one
  unit at a time. A scalar fallback covers CPUs without the vector finder.
- Flattened DFA transition rows into one `Vec<u32>` and added a compact
  parallel match-flag array so the per-unit loop does not index the state
  vector to read `matched`.
- Removed the per-call scratch `Mutex`. The binding now keeps one scratch per
  pattern per thread in a capped thread-local cache keyed by a stable pattern
  id, and clears the current thread's entry when the `Regex` is dropped.
- Changed `findall` to append result strings directly to a `PyList` instead of
  building an intermediate `Vec<Py<PyString>>`.
- Added scan tests for all widths, short tails, targets above `0xFF/0xFFFF/
  0x10FFFF`, and unaligned byte hits. Extended the Python binding smoke test
  with two alternating regexes, a long string above the GIL-detach threshold,
  and four threads sharing the same compiled patterns.
- Ran the full local checks and the full `regexus` engine and Python matrices
  under one `/tmp/core4.lock` lease on CPUs `0,2,4,6`.

### Results

Local laptop, 1,000,000 posts, one thread, release build, CPU `4`:

- UCS: 0.1456 s median.
- Rust `regex` bytes: 0.1518 s median.

Remote `regexus`, full corpus, first four physical P-cores `0,2,4,6`, SMT
siblings idle, `/tmp/core4.lock` held, load 1.05 to 1.47, efficiency 0.92 to
1.0, frequency 4.6 to 4.8 GHz.

Engine-only medians:

- 1 thread: UCS 0.0699 s, Rust `regex` bytes 0.0716 s, str 0.0711 s.
- 2 threads: UCS 0.0393 s, bytes 0.0381 s, str 0.0376 s.
- 3 threads: UCS 0.0299 s, bytes 0.0265 s, str 0.0267 s.
- 4 threads: UCS 0.0261 s, bytes 0.0210 s, str 0.0210 s.

End-to-end Python, GIL, medians and minima:

- 1 thread: `unirex` 0.1884 s / 0.1835 s; CPython `re` 0.1914 s /
  0.1913 s.
- 2 threads: `unirex` 0.1735 s / 0.1658 s; `re` 0.1907 s / 0.1877 s.
- 3 threads: `unirex` 0.1880 s / 0.1657 s; `re` 0.1906 s / 0.1816 s.
- 4 threads: `unirex` 0.1838 s / 0.1775 s; `re` 0.1879 s / 0.1834 s.

The one-thread end-to-end result is 1.6 percent faster than CPython `re` at
the median and 4.1 percent faster at the minimum. Engine-only UCS is 2.0
percent faster than Rust `regex` bytes at one thread, but 1.24 times slower at
four threads.

### Spec/ADR amendments

- [acted] Documented the unit-wide prefilter in `architecture.md` and added
  ADR-0015, recording the safe packed pair choice and the rejected byte-wise,
  `memmem`-per-search, and direct-AVX2 alternatives.
- [acted] Added ADR-0016 for thread-local scratch reuse and annotated
  ADR-0012's revised scratch-pool detail.
- [acted] Updated `benchmarks.md`, `README.md`, and `milestones.md` with the
  final remote matrix and parity result.
- [acted] Updated `testing.md` for the scan tests and the expanded binding
  smoke test.
- [open] Range first sets still use a scalar membership scan. Add a bitmap or
  range scan if a real pattern needs a narrow multi-unit first set.
- [open] Four-thread UCS scales 2.68 times while bytes scales 3.41 times.
  Profile per-worker DFA cache and class-map memory before changing the
  representation again.
- [open] Share immutable DFA class maps across scratches to cut the ~2.4 MB
  per-scratch cost, especially with several patterns per thread.
- [open] Add per-kind engine and end-to-end timing so UCS-4's share is visible
  without a separate probe.

### Future-task notes

- [open] Add process-based Python for many-core end-to-end runs; the GIL
  prevents meaningful thread scaling.
- [open] Verify CPython 3.10 through 3.15 and free-threaded 3.14t. Only 3.13
  GIL was tested.
- [open] Keep the Python binding smoke test in CI and keep the full 1M-post
  benchmark out of ordinary CI.
- [open] The Python frequency samples remain noisy; the 2-4 thread Python
  medians are reported for completeness, not as scaling evidence.
- [acted] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue; use the `rg` form from `testing.md`.
  Resolved: the command now uses the `rg` form from `testing.md`.

### Tooling/process

- [acted] Profiling the prefilter before changing the DFA representation was
  decisive: the first-unit scan, not determinization, was the dominant cost.
- [acted] Caching the prepared finder in the compiled pattern avoided the
  per-search `memmem::Finder` setup cost and its large finder value.
- [acted] Reusing the finder by reference instead of copying it per search
  removed a ~200-byte copy on every post.
- [acted] A single-slot or small-vector thread-local scratch cache is much
  cheaper than a `Mutex` pool, but the first Arc/Weak version was slower than
  the mutex because of upgrade and retain work; a stable integer id fixed it.
- [acted] Re-ran the full remote matrix after the DFA and binding changes, as
  the previous report required.
