## Report on task: Optimize the DFA fast path

### Task (archived from plan.md)

## Optimize the DFA fast path

Status: done

Remote `regexus`, one thread on the first four physical P-cores: end-to-end
`unirex` is 0.296 s median versus 0.191 s for CPython `re`. Engine-only, the
DFA is 0.191 s versus 0.071 s for Rust `regex` bytes. The binding overhead is
not the main gap; the per-candidate DFA setup and transition lookup are.
Optimize the DFA fast path and rerun both the engine-only and end-to-end
benchmarks. Acceptance: engine-only moves substantially toward 0.071 s, and
end-to-end moves toward the same-machine CPython `re` time of 0.191 s.

References: ADR-0010, ADR-0011, ADR-0012, ADR-0013, ADR-0014

### Done

- Cached DFA initial states for the four assertion contexts: interior,
  start-of-text, end-of-text, and start-and-end. `find_at` no longer runs an
  epsilon closure and allocates a `seen` vector for every prefilter candidate.
- Replaced the transition `HashMap<(state_id, class_id), ...>` with dense
  per-state rows indexed by equivalence-class ID. Unknown and dead transitions
  use sentinels. A transition-cell cap falls back instead of allowing
  unbounded memory.
- Removed the per-cache-miss clone of the state’s consume vector.
- Added correctness coverage through the existing DFA-vs-PikeVM, overflow,
  differential, and Python binding tests.
- Synced the change to `regexus`, rebuilt as `web`, and re-ran the engine and
  Python matrices under `/tmp/core4.lock` on CPUs `0,2,4,6`.
- Copied the raw remote JSON back to `var/bench-results/remote/` and updated
  `benchmarks.md`, `README.md`, `architecture.md`, `milestones.md`, and
  `plan.md`.

### Results: remote `regexus`, full corpus

Machine: Intel Core i5-13500, first four physical P-cores `0,2,4,6`, SMT
siblings idle. Frequency 4.6 to 4.8 GHz; load 0.4 to 1.5; efficiency 0.93 to
1.0. `/tmp/core4.lock` held.

Engine-only medians:

- 1 thread: UCS 0.1322 s, Rust `regex` bytes 0.0712 s, str 0.0713 s.
- 2 threads: UCS 0.0689 s, bytes 0.0380 s, str 0.0380 s.
- 3 threads: UCS 0.0469 s, bytes 0.0268 s, str 0.0267 s.
- 4 threads: UCS 0.0366 s, bytes 0.0210 s, str 0.0211 s.

UCS improved from 0.1912 s to 0.1322 s at one thread, a 1.45x improvement, and
from 0.0536 s to 0.0366 s at four threads. It remains 1.86x slower than bytes
at one thread and 1.74x slower at four.

End-to-end Python, GIL:

- 1 thread: `unirex` 0.2308 s median, 0.2289 s minimum; CPython `re`
  0.1914 s median, 0.1903 s minimum.
- `unirex` improved from 0.2960 s to 0.2308 s. It is now 1.21x slower than
  `re`.
- GIL threads 2-4 do not scale.

### Spec/ADR amendments

- [acted] Engine-only UCS is still 1.86x slower than Rust `regex` bytes and
  end-to-end is 1.21x slower than `re`. Resolved by the unit-wide prefilter,
  flat DFA transitions, and direct result-list construction: engine-only UCS
  is now 0.0699 s versus bytes 0.0716 s, and end-to-end is 0.1884 s versus
  `re` 0.1914 s on `regexus`.
- [open] Add process-based Python for many-core end-to-end runs.
- [open] Add per-kind engine timing.
- [open] Verify CPython 3.10-3.15 and free-threaded 3.14t.

### Future-task notes

- [acted] If further DFA work is chosen, profile candidate count, DFA steps,
  transition hits and misses before changing the representation again.
  Profiling showed the first-unit scan dominated; the prefilter and DFA
  hot-loop changes were based on those measurements.
- [acted] Re-run the remote matrix after any DFA change. Re-ran the full
  engine and Python matrix on `regexus` under `/tmp/core4.lock`.
- [open] Keep the `/tmp/core4.lock` lease and first-four-P-core placement.

### Tooling/process

- [acted] The initial-state allocation was the dominant per-candidate cost;
  caching it produced a 1.45x engine improvement.
- [acted] Dense per-state transition rows made the per-unit lookup hash-free;
  the memory cap keeps the fallback safe.
- [open] Python frequency samples remain diagnostic; engine-only frequency
  samples were stable at 4.6-4.8 GHz.
- [open] The marker command in `spec/skills/work.md` still contains the
  literal-pipe `grep` issue; use the `rg` form from `testing.md`.
