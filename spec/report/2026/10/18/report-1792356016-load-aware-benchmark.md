## Report on task: Load-aware benchmark runner

Ad-hoc work requested by the owner after observing that CPU load and frequency
made wall-clock results unreliable.

### Done

- Added pinning to the Rust benchmark:
  - `--cpu-list LIST` supports `2`, `0,2,4,6`, and `0-3`.
  - Each worker calls `sched_setaffinity` before the barrier and records its
    actual core with `sched_getcpu`.
- Added pinning to the Python benchmark with `os.sched_setaffinity`, and core
  reporting from `/proc/self/task/<tid>/stat`.
- Added `--warmup N` and `--warmup-seconds F` to both runners. The DFA caches
  and CPU frequency are warm before the measured runs.
- Added per-worker CPU time:
  - Rust uses `CLOCK_THREAD_CPUTIME_ID`.
  - Python uses `time.thread_time()`.
- Added `--min-efficiency` and `--retries`. A run is discarded when any
  worker's CPU time divided by wall time falls below the threshold. The runner
  retries, and fails if it cannot obtain the requested number of clean runs.
- Recorded `loadavg`, frequency before and after, CPU list, worker cores, and
  worker efficiencies in the JSON reports.
- Added ADR-0013 and updated `benchmarks.md`, `README.md`, `dev.md`,
  `testing.md`, `plan.md`, and `index.md`.
- The runner never writes sysfs and never changes the governor.

### Results and current status

The machine was under external load during the first load-aware runs: a
four-thread `rurex` frontend benchmark held the load near 3.

- Four-thread UCS runs were rejected because per-worker efficiency was about
  0.5 to 0.8.
- One-thread bytes had no clean runs; one-thread str had one clean run.
- One-thread UCS produced five clean runs at a median of 0.429 s with observed
  frequency 2.8 GHz to 3.0 GHz. That is not comparable to the earlier 0.360 s
  at higher frequency, so it is not a new official result.
- A pinned idle-core test before the external load reached 3.6 GHz after
  0.5 seconds of a busy loop, confirming that warmup works without changing
  the governor.

No new official engine or end-to-end numbers are claimed. The existing
numbers are marked provisional in the docs.

### Spec/ADR amendments

- [acted] Run the load-aware baseline. Executed on `regexus` with the first
  four physical P-cores and `/tmp/core4.lock`; 1-4 thread engine and Python
  matrices recorded.
- [acted] Use the load-aware baseline before optimizing the DFA or comparing
  end-to-end numbers. The remote baseline is now the reference.
- [open] For many-core end-to-end runs, add a process-based mode. Python GIL
  threads do not scale.
- [open] The governor remains `powersave`. The owner may set `performance`
  separately, but the benchmark must work without that.

### Future-task notes

- [open] Record CPU package temperature and thermal throttling if `turbostat`
  is available; currently the runner records frequency but not temperature.
- [acted] Keep external benchmark processes out of benchmark runs. The
  `/tmp/core4.lock` lease was used for the remote matrix.
- [acted] Rerun the 1-thread and 4-thread engine matrix and the `unirex`
  versus `re` end-to-end pair. Completed remotely.

### Tooling/process

- [acted] CPU efficiency now distinguishes preemption from engine cost. The
  runner correctly rejected the loaded four-thread runs.
- [acted] Per-worker efficiency is used instead of aggregate efficiency, so
  unequal chunk sizes are not mistaken for preemption.
- [open] `loadavg` after a run includes the benchmark's own workers. Use the
  per-worker efficiency and frequency for contamination decisions; a
  process-external load sampler can be added later.
- [open] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue; use the `rg` form from `testing.md`.
