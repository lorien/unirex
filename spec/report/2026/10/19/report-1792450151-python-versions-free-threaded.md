## Report on task: Verify Python versions and free-threaded builds

### Task (archived from plan.md)

## Verify Python versions and free-threaded builds

Status: done

The binding is verified only on CPython 3.13 GIL locally; CI now smoke-tests
CPython 3.10 through 3.15 GIL. What remains is free-threaded 3.14t, the 100k
end-to-end benchmark per interpreter, and recording ABI or GIL-detach
differences. Acceptance: a wheel and a passing smoke test for each supported
interpreter, including the free-threaded build.

References: ADR-0007, ADR-0012

### Done

- Added free-threaded legs `3.14t` and `3.15t` to the `binding` CI matrix, which
  already covered CPython 3.10 through 3.15 with the GIL. `3.13t` is excluded
  on the owner's instruction; it is still experimental upstream.
- The smoke test detects `sys._is_gil_enabled()`, runs eight workers instead of
  four when the GIL is off, and reports the interpreter version and GIL state,
  so the free-threaded legs are visibly different in the log.
- CI now runs `tools/bench_python.py --engine unirex --limit 100000 --repeat 2`
  for every interpreter in the matrix, with the corpus cached, so per-version
  binding performance is measured on each run instead of asserted in prose.
- Recorded a local reference point on CPython 3.13.5: 100,000 posts, one
  thread, median 0.0541 s, minimum 0.0536 s, 43,456 mentions.
- Documented the coverage in `benchmarks.md` and `testing.md`, annotated
  ADR-0012's status with the verification range, and updated the milestone 5
  status in `milestones.md` and the README table.

### Notes

The free-threaded wheel is non-abi3 like the GIL wheel, so each interpreter gets
its own artifact. Nothing in the binding was changed for free-threading: the UCS
accessors read an immutable `str`, and the per-thread scratch was already keyed
outside any global lock. Whether that holds under `3.14t` and `3.15t` is now
checked on every push rather than assumed.

### Spec/ADR amendments

- [acted] ADR-0012 status now states the verified interpreter range and the
  free-threaded builds.
- [acted] `benchmarks.md` gained a per-interpreter benchmark section, and
  `testing.md` describes the free-threaded worker count and the CI benchmark.
- [open] The per-interpreter numbers live in CI logs only. If they should be
  tracked over time, a results file or a published table per release is needed.

### Future-task notes

- [open] No functional release exists on PyPI or crates.io; both registries
  still hold the empty `0.0.1` placeholders.
- [open] The verbose-mode `[ ]` divergence remains the only documented
  difference from CPython `re`.
- [open] A manylinux wheel job is still required before a real release; the CI
  legs build on the runner.

### Tooling/process

- [acted] Checking the actions manifest before writing the matrix avoided
  guessing: free-threaded builds exist for 3.13, 3.14, and 3.15, which is why
  the owner's exclusion of 3.13t was a real decision rather than a gap.
- [acted] Running the benchmark inside the matrix keeps the numbers honest; they
  appear in the log of the commit that produced them.
