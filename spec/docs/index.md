# Spec Index

Catalog of the documents under `spec/docs/`. Recommended reading order:

1. [overview.md](overview.md) — the project, its motivation, scope, success
   criteria, and repository layout.
2. [conventions.md](conventions.md) — document, ADR, Rust, benchmark, and
   commit conventions.
3. [architecture.md](architecture.md) — compilation pipeline, haystack,
   instruction set, classes, PikeVM, prefilter, DFA, and the Python binding.
4. [semantics.md](semantics.md) — supported syntax, matching rules, flags,
   surrogates, and documented divergences.
5. [milestones.md](milestones.md) — the five milestones and their acceptance
   criteria.
6. [testing.md](testing.md) — test layers, commands, and per-session final
   checks.
7. [benchmarks.md](benchmarks.md) — corpus provenance, metrics, baselines, and
   success criteria.
8. [dev.md](dev.md) — prerequisites, dependencies, and the local build loop.
9. [publishing.md](publishing.md) — registry placeholders, version policy,
   credentials, and publish verification.
10. [plan.md](plan.md) — the open-task list.

## Decision records

- [adr/0000-adr-format.md](adr/0000-adr-format.md) — the ADR format itself.
- [adr/0001-spec-driven-agent-workflow.md](adr/0001-spec-driven-agent-workflow.md)
  — `AGENTS.md` as a pointer, `spec/docs/` as the source of truth, and the
  plan, report, and ADR lifecycle.
- [adr/0002-ucs-native-engine.md](adr/0002-ucs-native-engine.md) — a
  standalone UCS-native Thompson NFA and PikeVM over a `regex-syntax` parse,
  with no UTF-8 conversion in the matching path (parser choice revised by
  ADR-0008).
- [adr/0003-leftmost-first-semantics.md](adr/0003-leftmost-first-semantics.md)
  — Python-compatible leftmost-first matching, including empty-match
  iteration and `$` before a trailing newline.
- [adr/0004-ucs-haystack-enum.md](adr/0004-ucs-haystack-enum.md) — the borrowed
  `Ucs` enum, monomorphized search, and safe slices in the core.
- [adr/0005-class-representation.md](adr/0005-class-representation.md) —
  canonical sorted ranges with per-kind accelerators and compile-time class
  resolution.
- [adr/0006-lazy-dfa-after-benchmark.md](adr/0006-lazy-dfa-after-benchmark.md)
  — the PikeVM first; a capped lazy DFA only if the benchmark justifies it.
- [adr/0007-cpython-ucs-contract.md](adr/0007-cpython-ucs-contract.md) —
  UCS-2 units as code points including lone surrogates, validated PEP 393
  layout detection, and results built from the UCS buffer.
- [adr/0008-python-dialect-adapter.md](adr/0008-python-dialect-adapter.md) —
  parse the `regex-syntax` AST, validate a strict Python subset, lower it in
  this crate, and reject Rust-only syntax instead of using Rust semantics.
- [adr/0009-benchmark-protocol.md](adr/0009-benchmark-protocol.md) —
  self-contained Telegram benchmark: corpus resolution and checksum, actual
  UCS kinds, reusable scratch, same-machine Rust baselines, and an independent
  mention-count oracle.
- [adr/0010-lazy-dfa-required.md](adr/0010-lazy-dfa-required.md) — the
  prefilter improves the engine 4.3x but not enough; the lazy DFA is now
  required and milestone 4 is prioritized.
- [adr/0011-dfa-match-tracking.md](adr/0011-dfa-match-tracking.md) — lazy DFA
  state representation, leftmost-first match pruning, pending end assertions,
  bounded cache, and PikeVM fallback.
- [adr/0012-cpython-ucs-access.md](adr/0012-cpython-ucs-access.md) — the
  binding uses CPython's UCS accessors and `PyUnicode_Substring`, not private
  PEP 393 offsets or UTF-8 conversion.
- [adr/0013-load-aware-benchmark.md](adr/0013-load-aware-benchmark.md) —
  observational benchmarking with pinning, warmup, CPU time, per-worker
  efficiency, and rejection of contaminated runs.
- [adr/0014-benchmark-core-lease.md](adr/0014-benchmark-core-lease.md) —
  cooperative `/tmp/core4.lock` lease for the first four physical P-cores,
  with `--lock`, `--lock-wait`, and `--no-lock`.
- [adr/0015-unit-wide-prefilter-scan.md](adr/0015-unit-wide-prefilter-scan.md)
  — prepared native-endian packed pair scanning for UCS-2/UCS-4 first units,
  with full-unit verification and a scalar tail fallback.
- [adr/0016-thread-local-scratch.md](adr/0016-thread-local-scratch.md) —
  per-thread scratch reuse keyed by a stable pattern id, replacing the
  per-call `Mutex` pool.
- [adr/0017-capture-extraction.md](adr/0017-capture-extraction.md) — capture
  slots stay out of the DFA; group spans are recovered over the match span with
  a bounded backtracker and a PikeVM fallback.
- [adr/0018-bounded-candidate-work.md](adr/0018-bounded-candidate-work.md) —
  the anchored candidate loop is capped and falls back to the PikeVM, keeping
  search linear instead of quadratic, after a reverse-pass design produced
  wrong leftmost-first spans.
- [adr/0019-anchored-start-fast-rejection.md](adr/0019-anchored-start-fast-rejection.md)
  — a compile-time instruction-level fixpoint marks programs whose every
  accepting run crosses a start assertion; unanchored searches over such
  programs try only the first position, making `search(r'\Ay', s)` constant
  time.
