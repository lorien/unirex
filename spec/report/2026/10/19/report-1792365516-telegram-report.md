## Report on task: Generate the Telegram benchmark report

### Done

- Added `tools/telegram_benchmark.py`, a single matrix entry point that:
  - optionally builds the release bench and extension;
  - holds `/tmp/core4.lock` for the whole matrix and passes `--no-lock` to
    the child runners;
  - runs engine-only `ucs`, `bytes`, and `str` at 1 to 4 workers;
  - runs Python `exp_rurex` and `re` at 1 to 4 workers;
  - writes JSON and a manifest under `var/bench-results/telegram/`;
  - calls the report renderer unless `--no-render` or `--limit` is used.
- Added `tools/telegram_report.py`, which:
  - loads and validates every expected matrix point;
  - verifies the full-corpus counts;
  - reads corpus, environment, load, and frequency metadata from the JSON and
    manifest;
  - generates two SVG line charts with the Python standard library;
  - writes `docs/telegram-benchmark.md`, `docs/img/telegram_*.svg`, and the
    self-contained `docs/telegram-benchmark.html`;
  - avoids matplotlib and markdown dependencies.
- Added `gil_enabled` to the Python benchmark JSON so a future free-threaded
  run is distinguishable from a GIL run. The report currently rejects mixed
  free-threaded points rather than publishing GIL prose over them.
- Ran the full matrix on `regexus` under the core lease with five repeats and
  a two-second warmup. The generated report is committed from that run.
- Updated `README.md`, `spec/docs/benchmarks.md`, `spec/docs/dev.md`,
  `spec/docs/overview.md`, and `spec/docs/testing.md` for the new workflow and
  current results.
- Verified the report generator from saved JSON, parsed both SVG files as XML,
  checked all local Markdown links and image paths, and re-ran the standard Rust
  and Python checks.

### Results

Remote `regexus`, full corpus, first four physical P-cores `0,2,4,6`:

- Engine-only 1 thread: UCS 0.0689 s, Rust `regex` bytes 0.0715 s, str
  0.0715 s.
- Engine-only 4 threads: UCS 0.0257 s, bytes 0.0209 s, str 0.0212 s.
- End-to-end Python 1 thread: `exp_rurex` 0.1887 s median and 0.1838 s
  minimum; CPython `re` 0.1912 s median and 0.1905 s minimum.
- The generated page reports 1.3 percent faster at the median and 3.5 percent
  faster at the minimum for the one-thread end-to-end result.

### Spec/ADR amendments

- [acted] Added the published-report workflow to `spec/docs/benchmarks.md`
  and `spec/docs/dev.md`.
- [acted] Added the new tools and `docs/` assets to the repository layout in
  `spec/docs/overview.md`.
- [open] The report currently renders the GIL matrix only. Implement a
  free-threaded section when CPython 3.14t results exist, grouping points by
  `gil_enabled` and adding a second chart and table.
- [open] The generated charts are SVG. If a target Markdown viewer does not
  render SVG images, add a PNG renderer with Pillow or matplotlib while
  keeping the HTML self-contained.
- [open] The README and spec documents duplicate the latest benchmark numbers.
  On each new matrix run, update them or replace the duplicates with a link to
  the generated page.
- [open] Add a small synthetic report fixture to CI so the renderer and HTML
  twin are checked without the full 1M-post matrix.

### Future-task notes

- [open] The full matrix is intentionally manual; ordinary CI should run only
  the report self-check and the existing smoke tests.
- [open] Keep raw JSON and the full corpus under `var/`. Never commit them.
- [open] Keep the generated HTML gitignored; commit the Markdown and SVG chart
  assets so the page has no broken images.
- [open] Keep the core lease for the whole matrix; the orchestrator now does
  this by default.
- [open] Re-run the orchestrator rather than the individual runners when
  publishing a new page, so the manifest and report stay in sync.

### Tooling/process

- [acted] The `--build` option makes the remote run one command while still
  allowing prebuilt binaries.
- [acted] The orchestrator deletes old `*.json` in its results directory before
  a measured run, preventing duplicate points from a previous matrix.
- [acted] The renderer reads all prose numbers from the JSON, so rerunning the
  matrix cannot leave hand-copied values in the generated page.
- [open] Python frequency samples are still diagnostic; the report summarizes
  the engine-only frequency from active workers and leaves Python samples in
  the raw JSON.
