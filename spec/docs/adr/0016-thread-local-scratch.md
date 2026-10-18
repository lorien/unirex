# Thread-Local Scratch Reuse

Date: 2026-10-19
Status: accepted

## Context

Each search needs a `Scratch`, which owns the lazy DFA cache and its class
maps. Allocating one per call is unacceptable: the UCS-4 class map alone is
about 2.2 MB. The binding originally kept a `Mutex<Vec<Scratch>>` per compiled
`Regex`; every `findall` call locked the pool, popped a scratch, scanned, and
locked again to return it. Profiling showed those two lock operations were a
noticeable part of the in-extension samples.

A scratch is not shared between threads. That makes thread-local storage a
natural fit, but a thread-local cache also has to avoid confusing scratches
from two different compiled patterns: the DFA state and class maps depend on
the program.

## Decision

- Give every compiled `Regex` a stable, process-unique integer id from an
  atomic counter. Ids are never reused.
- Keep a thread-local `Vec<(u64, Scratch)>` capped at four entries. A call
  searches for its pattern id, removes and returns that scratch, or allocates
  a new one on a miss.
- Return the scratch to the same thread-local cache after the scan. The cache
  evicts the oldest entry when full.
- Clear the current thread's entries when the `Regex` is dropped. Entries in
  other threads are released when those threads exit or the cache evicts them.
- Take and return the scratch outside any optional GIL detach, so the scratch
  never crosses a thread boundary.

## Alternatives rejected

- Keep the per-call `Mutex` pool: correct and simple, but two lock operations
  per call remained visible in the profile.
- Allocate a scratch per call: the DFA cache and UCS class maps are too large
  for that.
- Use `Arc<()>` plus `Weak` identity checks in a thread-local cache: safe, but
  measured slower than the mutex because of weak upgrades and retain passes.
- Use one unnamed thread-local slot: faster for one pattern, but alternating
  two patterns would discard and rebuild a multi-megabyte scratch on every
  switch.
- Store the scratch in the `Regex` without synchronization: a compiled regex
  is shared across threads, and a `Scratch` is not shareable.

## Consequences

A thread retains at most four scratches, so a process using several patterns
per thread has bounded extra memory. Pattern ids are never reused, so a stale
thread-local entry cannot be mistaken for a new pattern. Dropping a `Regex`
does not immediately free entries held by other threads, but those entries are
released at thread exit or eviction. The binding remains safe Rust apart from
the documented CPython UCS pointer conversion.
