## Report on task: Remote benchmark with core lease

### Task (archived from plan.md)

## Collect a load-aware benchmark baseline

Status: done

Run the engine-only and end-to-end benchmarks on an idle laptop with
`--cpu-list`, `--warmup-seconds`, CPU-time accounting, and
`--min-efficiency`. The current numbers predate this protocol and are
provisional. The last attempt was rejected because an external benchmark kept
the machine loaded. Record CPU frequency, load, per-worker efficiency, and the
exact CPU list. Acceptance: the requested number of clean runs for UCS, bytes,
and str at 1 and 4 threads, and for `exp_rurex` and `re` end to end at 1
thread, with all conditions recorded.

The owner changed the execution target to `regexus` and added the core lease
requirement.

### Done

- Implemented an exclusive `flock` lease on `/tmp/core4.lock` in both the Rust
  and Python benchmarks.
- Added `--lock`, `--lock-wait`, and `--no-lock`. The lease is held from before
  warmup through the whole run and released on exit, error, or crash.
- Owner metadata records user, PID, host, CPUs `0,2,4,6`, start time, and
  command. A waiting process sees the owner and hard-fails on timeout.
- Verified contention and release: a holder blocks a contender; a normal exit
  and a killed process both release the lock.
- Added ADR-0014 and updated `benchmarks.md`, `README.md`, `dev.md`,
  `testing.md`, `AGENTS.md`, `index.md`, and `plan.md`.
- Rsynced the project to `web@regexus:/web/benchmark/exp_rurex`, excluding
  `target`, `var`, `.git`, virtualenvs, `__pycache__`, and built modules.
- Built and tested remotely as `web` with Rust 1.99.1. All 8 unit tests and 15
  integration tests passed. The Python extension built and the binding smoke
  test passed.
- Downloaded and SHA-256 verified the corpus on `regexus`. This was a hard
  requirement; no local corpus fallback was used.
- Ran the engine and Python matrices under one shell-level lease, with
  `--no-lock` in the children.

### Results: remote `regexus`, full corpus

Machine: Intel Core i5-13500, 14 physical cores, 20 logical threads, Debian,
Rust 1.99.1, Python 3.13.5. First four physical P-cores `0,2,4,6`; SMT
siblings `1,3,5,7` idle. Frequency 4.6 to 4.8 GHz. Load 1.5 to 2.1 from other
work; per-worker efficiency 0.91 to 1.0. Governor stayed `powersave`; turbo was
enabled. `/tmp/core4.lock` was held for the whole matrix.

Engine-only medians:

- 1 thread: UCS 0.1912 s, Rust `regex` bytes 0.0712 s, str 0.0713 s.
- 2 threads: UCS 0.0983 s, bytes 0.0380 s, str 0.0380 s.
- 3 threads: UCS 0.0694 s, bytes 0.0268 s, str 0.0267 s.
- 4 threads: UCS 0.0536 s, bytes 0.0210 s, str 0.0211 s.

UCS scales 3.56 times from 1 to 4 threads; bytes scales 3.39 times. UCS is
2.69 times slower than bytes at one thread and 2.55 times slower at four.

End-to-end Python, GIL:

- 1 thread: `exp_rurex` 0.2960 s median, 0.2941 s minimum; CPython `re`
  0.1909 s median, 0.1903 s minimum.
- 2 threads: `exp_rurex` 0.2972 s; `re` 0.1957 s.
- 3 threads: `exp_rurex` 0.3330 s; `re` 0.3572 s.
- 4 threads: `exp_rurex` 0.5474 s; `re` 0.4419 s.

GIL threads do not scale. The meaningful end-to-end number is one thread:
`exp_rurex` is about 1.55 times slower than CPython `re`. It is below the
0.75 s and 0.45 s targets on this machine, but the remote CPU is much faster
than the original laptop. No `rurex` engine was run.

### Spec/ADR amendments

- [acted] Optimize the DFA fast path. Initial-state caching and dense
  transition rows improved engine-only UCS from 0.1912 s to 0.1322 s and
  end-to-end from 0.2960 s to 0.2308 s.
- [open] Add a process-based Python mode for many-core end-to-end runs. GIL
  threads do not scale.
- [open] Verify CPython 3.10 through 3.15 and free-threaded 3.14t.
- [open] The lease is advisory. It cannot stop a non-cooperating process;
  cpuset isolation needs root or a delegated cpuset controller.
- [open] Add per-kind engine timing to the remote matrix.

### Future-task notes

- [open] Keep `/tmp/core4.lock` for the first four P-cores on `regexus`.
- [acted] Re-run the remote matrix after any DFA change. Re-ran the full
  engine and Python matrix after the unit-wide prefilter and flat transitions.
- [open] Add a lock-contention test to CI, not only a manual check.
- [open] Record package temperature and throttling if `turbostat` is installed.

### Tooling/process

- [acted] Build as `web` with the existing Rust 1.99.1 toolchain; no install was
  needed.
- [acted] Corpus download was mandatory. It succeeded and verified; no local
  fallback was used.
- [acted] The shell-level lease plus `--no-lock` children held one lease for the
  whole matrix.
- [open] Python frequency sampling showed idle values on some points even when
  CPU efficiency was near 1.0. Treat the Python frequency field as diagnostic,
  not definitive; engine-only frequency samples were stable at 4.6 to 4.8 GHz.
- [open] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue; use the `rg` form from `testing.md`.
