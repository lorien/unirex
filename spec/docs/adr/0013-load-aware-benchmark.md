# Observational Benchmark Protocol

Date: 2026-10-18
Status: accepted

## Context

Laptop benchmark results were wall-clock only. The CPU governor is
`powersave`, turbo is enabled, frequency moves between roughly 0.4 GHz and
3.6 GHz, and other tasks can occupy cores. An external four-thread benchmark
was running during the first load-aware attempt. Wall time alone cannot tell
whether a slow run is the engine, a low-frequency state, or preemption.

The benchmark must not require root, must not change the governor, and must
not report a contaminated run as an engine result.

## Decision

Use an observational protocol. The runner only reads system state and never
writes sysfs.

- Pin every worker to a requested CPU with `sched_setaffinity`.
- Warm up for a minimum number of passes and a minimum duration, then sample
  `scaling_cur_freq`.
- Measure per-worker CPU time (`CLOCK_THREAD_CPUTIME_ID`, `time.thread_time`)
  alongside wall time.
- Discard a run when any worker's CPU time divided by its wall time is below
  `--min-efficiency` (default 0.90). Retry up to `--retries` extra attempts.
- Record `/proc/loadavg`, per-worker core IDs, efficiencies, frequency before
  and after, and the CPU list in the report.
- Fail when no clean runs can be obtained. Do not silently report a loaded run.
- Keep engine results separate from Python end-to-end results and from
  external numbers.

## Alternatives rejected

- Set the governor to `performance`: requires root and changes the machine,
  so results are not reproducible without the same privilege. The owner can do
  it separately, but the benchmark must work without it.
- Report wall time only: cannot distinguish preemption from engine cost.
- Report only the minimum: a single lucky run hides a loaded environment and
  is not a typical result.
- Use a remote many-core machine: useful later, but the current target is the
  laptop and the protocol should work anywhere.
- Change process priority or use real-time scheduling: requires privileges and
  can starve other tasks.

## Consequences

- Under external load the benchmark may reject every run and exit with an
  error. That is the intended behavior.
- Absolute times still depend on observed frequency. The report records the
  frequency and must not present a low-frequency run as a best-case result.
- Multi-thread efficiency is per worker, not aggregate, so unequal chunk sizes
  are not mistaken for preemption.
- Python GIL threads do not scale; multi-core end-to-end needs processes or
  free-threaded Python.
