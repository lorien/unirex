# Benchmark Core Lease

Date: 2026-10-18
Status: accepted

## Context

Several agents can run benchmarks on `regexus`. The benchmark uses the first
four physical P-cores, logical CPUs `0,2,4,6`, with SMT siblings `1,3,5,7`
idle. CPU affinity only restricts our own threads; it does not stop another
process from using the same cores. The `web` user has no delegated `cpuset`
controller, so a kernel-enforced cpuset partition is not available without
root. The governor also remains outside the benchmark.

A cooperative lease is the available mechanism.

## Decision

- Use an exclusive advisory `flock` on `/tmp/core4.lock` on the benchmark
  host. The file is created mode `0666`.
- Acquire the lease before warmup and hold it through the entire benchmark
  matrix.
- Write owner metadata: user, PID, host, CPU list `0,2,4,6`, start time, and
  command.
- If the lease is busy, read the owner, wait up to `--lock-wait`, and hard-fail
  on timeout.
- Release on normal exit, error, or crash. `flock` releases with the process.
- Both the Rust and Python runners support `--lock`, `--lock-wait`, and
  `--no-lock`. A matrix may hold one shell-level lease and pass `--no-lock` to
  its child points.
- Document that the lease is advisory. Every benchmark agent on the host must
  use it; it cannot stop a non-cooperating process.

## Alternatives rejected

- `cpuset` cgroup partition: requires root or delegated cpuset; the `web` user
  does not have it.
- `systemd` `AllowedCPUs`: needs the cpuset controller delegated to the user
  manager.
- `isolcpus`: requires a boot change and reboot.
- Per-core locks: more files and deadlock ordering, with no benefit for a
  single fixed four-core set.
- No lease: concurrent benchmarks contaminate each other and produce
  misleading numbers.
- Relying only on CPU affinity: restricts our threads but leaves other
  processes free to use the cores.

## Consequences

- The lease is advisory, not enforced by the kernel. A non-cooperating process
  can still steal the cores.
- Per-worker CPU efficiency and per-core load/frequency recording remain the
  detection layer. Contaminated runs are rejected, not reported.
- A stale metadata block can remain after a crash, but the lock itself is
  released; the next holder overwrites the metadata.
- The fixed path `/tmp/core4.lock` is host-local and does not coordinate across
  machines or separate mount namespaces.
