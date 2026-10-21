## Report on task: Refresh the Telegram benchmark matrix on regexus

Ad-hoc task requested by the owner: the published Telegram page predated the
current engine, so re-run the full matrix on `regexus` and commit updated
results.

### Done

- Rsynced `bb0db84` to `web@regexus:/web/benchmark/unirex` (the documented
  excludes; the remote corpus and lock were already in place, lock free,
  load 0.16). Built the bench example and the extension, and ran
  `cargo test --release` remotely: 23 unit and 19 integration tests passed.
- Ran the full matrix under one `/tmp/core4.lock` lease:
  `tools/telegram_benchmark.py --cpu-list 0,2,4,6 --repeat 5
  --warmup-seconds 2` on CPUs `0,2,4,6`. Every point verified the 334,136
  mentions in 186,226 posts; one-thread efficiency was 1.00, multi-thread
  above 0.92, load 0.31 to 1.34. The renderer rewrote
  `docs/telegram-benchmark.md`, both SVGs, and the gitignored HTML twin.
- Pulled the 21 raw JSONs and the manifest back into
  `var/bench-results/telegram/` (gitignored) and the tracked report files.
- Updated the `Remote results on regexus` section of `spec/docs/benchmarks.md`
  with the new medians and the comparison story, and marked the old
  re-run note in the 2026-10-19 rename report `[acted]`.
- Cleaned up: removed the three A/B checkouts on `regexus` and the local
  worktrees.

### Results

New matrix (medians): engine UCS 0.0754/0.0418/0.0314/0.0268 s at 1-4
threads against `regex` bytes 0.0716/0.0378/0.0268/0.0211 s; binding
0.2099/0.2047/0.2053/0.1823 s against `re` 0.1902/0.1877/0.1921/0.1877 s.
The one-thread binding is 10.3 percent slower than `re` at the median; the
engine-only UCS path is 5.2 percent slower than `regex` bytes.

### What moved, and why (same-host A/B)

The previous matrix was generated before capture extraction landed, so the
page had been drifting with the engine, not with the machine. A/B on
`regexus` at the UCS one-thread point, with the `regex` baselines flat
between builds:

- `bd0b993` (previous matrix engine): UCS 0.0698 s
- `8380c85` (capture extraction, ADR-0017): UCS 0.0754 s
- `1e832ed` (pre-conformance parent): UCS 0.0785 s
- `bb0db84` (current): UCS 0.0764 s, and 0.0754 s in the published matrix run

So capture extraction costs about 9 percent on the engine-only scan; it
flipped both published ratios (UCS from faster than `regex` bytes to 1.05
times slower; the binding from 3.8 percent faster than `re` to 10.3 percent
slower). The conformance-gap commit is not the cause; it measures equal to
or slightly faster than its parent.

### Spec/ADR amendments

- [acted] `spec/docs/benchmarks.md`: new remote medians, environment facts,
  matrix provenance (date and commit), and a `What changed against the
  previous matrix` subsection recording the A/B attribution.

### Future-task notes

- [open] Recover the ~9 percent engine-only UCS cost introduced by capture
  extraction (`8380c85`, ADR-0017). The span-only `find_iter` path used by
  the benchmark does not need capture slots; profiling the added work in
  `find`/`find_pike` is the first step. This is engine work, not
  measurement work.
- [open] The raw JSON records no commit hash (`rustc: unknown`, no commit
  field). Stamp the engine commit into each result so provenance no longer
  depends on the report text.
- [open] Keep the remote checkout in sync before any future matrix: this
  refresh needed a full rsync, build, and remote test pass first.

### Tooling/process

- The report renderer's "crate worker efficiency was 1.00" line covers the
  one-thread points; multi-thread engine efficiencies were 0.92 to 0.97.
  The wording could name which points it summarizes.
