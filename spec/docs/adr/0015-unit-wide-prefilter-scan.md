# Unit-Wide First-Unit Prefilter Scan

Date: 2026-10-19
Status: accepted

## Context

The lazy DFA reduced the Telegram benchmark, but profiling showed that the
first-unit prefilter was still the largest engine cost. For a single unit it
searched the raw UTF-16/UTF-32 storage with `memchr` and then checked each byte
hit against the unit boundary and the full unit value. On UCS-4 this scans
four bytes per unit and restarts the byte search for every false hit. A local
400k-post probe measured about 0.028 s for UCS-2 and 0.051 s for UCS-4 in the
old prefilter, out of about 0.091 s for the full pattern scan.

Searching for the whole native-endian unit with a packed pair finder avoided
most false hits and scanned once. A safe implementation is available in
`memchr::arch::x86_64::avx2::packedpair::Finder`: it is copyable, owns no
borrow, and exposes `find_prefilter` after checking CPU support. It measured
about 0.008 s for UCS-2 and 0.025 s for UCS-4 in the same probe.

## Decision

For a single-unit first set, compile a native-endian unit needle for UCS-2 and
UCS-4 and prepare a packed pair finder for each width at pattern compile time.
Store the finder in the compiled `FirstSet`. A `Scan` borrows the prepared
finder for one search, so the finder is not copied or rebuilt per candidate.

The scan is still only a prefilter:

- a candidate is accepted only when its byte offset is unit-aligned and the
  full unit equals the target;
- the short tail below the vector finder's minimum length is scanned one unit
  at a time;
- when no packed pair finder is available, the width falls back to a scalar
  unit scan.

Range first sets keep the existing scalar membership scan. The PikeVM and DFA
semantics do not change.

## Alternatives rejected

- Keep byte-wise `memchr`: it scans multiple bytes per unit and restarts on
  unaligned hits; the profiling showed it dominated the scan.
- Use `memmem::Finder` per search: it is safe, but the finder is large and its
  setup cost is paid per post. Storing it in the compiled pattern would need a
  self-referential or leaked needle.
- Use direct AVX2 intrinsics in the core: it measured slightly faster, but the
  project keeps the core engine in safe Rust and allows `unsafe` only in the
  Python binding.
- Use the architecture-independent packed pair finder: it restarts on one byte
  and measured slower than the old byte scan for UCS-4.
- Optimize only the DFA: after the prefilter fix the DFA was no longer the
  dominant cost; the first-unit scan had to be fixed first.

## Consequences

The prefilter is now a safe, prepared, unit-wide scan with a correct scalar
fallback. Compiled patterns keep two small finders (UCS-2 and UCS-4) instead
of only a target unit. The engine remains correct on short haystacks and on
CPUs without the vector finder. The full-corpus one-thread engine time on
`regexus` moved from 0.1322 s to 0.0699 s, slightly ahead of Rust `regex`
bytes at 0.0716 s.
