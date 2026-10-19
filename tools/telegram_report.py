#!/usr/bin/env python3
"""Render the generated Telegram benchmark report.

Reads the JSON files written by tools/telegram_benchmark.py (or by the
individual example/bench and tools/bench_python.py runners), then writes:

    docs/telegram-benchmark.md
    docs/telegram-benchmark.html   (self-contained, gitignored)
    docs/img/telegram_*.svg

The SVG charts use only the Python standard library, so the report has no
matplotlib or markdown dependency. Numbers in the prose and tables are read
from the JSON; rerunning the benchmark cannot leave a hand-copied number
behind.

Usage:
    python3 tools/telegram_report.py --results-dir var/bench-results/telegram
    python3 tools/telegram_report.py --results-dir var/bench-results/telegram --no-html
"""

from __future__ import annotations

import argparse
import html
import json
import math
import re
import sys
from datetime import datetime
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_RESULTS = ROOT / "var" / "bench-results" / "telegram"
DEFAULT_OUT = ROOT / "docs"

PATTERN = r"@[a-zA-Z][a-zA-Z0-9_]{4,31}"
FULL_POSTS = 1_000_000
FULL_MENTIONS = 334_136
FULL_POSTS_WITH = 186_226
THREADS = (1, 2, 3, 4)
ENGINE_MODES = ("ucs", "bytes", "str")
PY_ENGINES = ("unirex", "re")

COLOR_EXP = "#CE422B"
COLOR_RE = "#4C72B0"
COLOR_STR = "#55A868"


def fmt_seconds(value: float) -> str:
    return f"{value:.4f}"


def fmt_ratio(value: float) -> str:
    return f"{value:.2f}"


def pct_faster(faster: float, reference: float) -> str:
    percent = (reference - faster) / reference * 100.0
    if percent >= 0:
        return f"{percent:.1f} percent faster"
    return f"{abs(percent):.1f} percent slower"


def load_results(results_dir: Path):
    engine: dict[tuple[str, int], dict] = {}
    python: dict[tuple[str, int], dict] = {}
    manifest: dict = {}
    if not results_dir.is_dir():
        raise SystemExit(f"results directory not found: {results_dir}")

    for path in sorted(results_dir.glob("*.json")):
        try:
            data = json.loads(path.read_text())
        except json.JSONDecodeError as exc:
            raise SystemExit(f"invalid JSON in {path}: {exc}") from exc
        if path.name == "manifest.json":
            manifest = data
            continue
        threads = data.get("threads")
        if data.get("mode") in ENGINE_MODES and isinstance(threads, int):
            key = (data["mode"], threads)
            if key in engine:
                raise SystemExit(f"duplicate engine point {key} in {results_dir}")
            engine[key] = data
        elif data.get("engine") in PY_ENGINES and isinstance(threads, int):
            key = (data["engine"], threads)
            if key in python:
                raise SystemExit(f"duplicate Python point {key} in {results_dir}")
            python[key] = data
        else:
            print(f"ignoring unrelated JSON: {path}", file=sys.stderr)

    missing = []
    for mode in ENGINE_MODES:
        for threads in THREADS:
            if (mode, threads) not in engine:
                missing.append(f"engine {mode} {threads}t")
    for name in PY_ENGINES:
        for threads in THREADS:
            if (name, threads) not in python:
                missing.append(f"python {name} {threads}t")
    if missing:
        raise SystemExit(
            "results are incomplete; missing: " + ", ".join(missing)
        )
    free = [
        (name, threads)
        for (name, threads), point in python.items()
        if point.get("gil_enabled") is False
    ]
    if free:
        raise SystemExit(
            "free-threaded Python points are present, but this report renders "
            "the GIL matrix only: " + ", ".join(f"{n} {t}t" for n, t in free)
        )
    return engine, python, manifest


def validate_points(engine: dict, python: dict) -> None:
    for (mode, threads), point in engine.items():
        if point.get("limit") is not None:
            raise SystemExit(
                f"engine {mode} {threads}t used --limit; the report needs the full corpus"
            )
        if point.get("posts") != FULL_POSTS or point.get("count") != FULL_MENTIONS:
            raise SystemExit(
                f"engine {mode} {threads}t: unexpected counts "
                f"posts={point.get('posts')} mentions={point.get('count')}"
            )
        if point.get("posts_with_count") != FULL_POSTS_WITH:
            raise SystemExit(
                f"engine {mode} {threads}t: unexpected posts_with_count="
                f"{point.get('posts_with_count')}"
            )
    for (name, threads), point in python.items():
        if point.get("limit") is not None:
            raise SystemExit(
                f"python {name} {threads}t used --limit; the report needs the full corpus"
            )
        if point.get("posts") != FULL_POSTS or point.get("mentions") != FULL_MENTIONS:
            raise SystemExit(
                f"python {name} {threads}t: unexpected counts "
                f"posts={point.get('posts')} mentions={point.get('mentions')}"
            )
        if point.get("posts_with_mentions") != FULL_POSTS_WITH:
            raise SystemExit(
                f"python {name} {threads}t: unexpected posts_with_mentions="
                f"{point.get('posts_with_mentions')}"
            )


def corpus_stats(engine: dict) -> dict:
    ucs = engine[("ucs", 1)]
    ucs1_units = int(ucs.get("ucs1_units", 0))
    ucs2_units = int(ucs.get("ucs2_units", 0))
    ucs4_units = int(ucs.get("ucs4_units", 0))
    return {
        "posts": int(ucs["posts"]),
        "utf8_bytes": int(ucs["utf8_bytes"]),
        "ucs_storage_bytes": ucs1_units + 2 * ucs2_units + 4 * ucs4_units,
        "units": int(ucs["units"]),
        "mentions": int(ucs["count"]),
        "posts_with": int(ucs["posts_with_count"]),
        "ucs1_posts": int(ucs.get("ucs1_posts", 0)),
        "ucs2_posts": int(ucs.get("ucs2_posts", 0)),
        "ucs4_posts": int(ucs.get("ucs4_posts", 0)),
        "ucs1_units": ucs1_units,
        "ucs2_units": ucs2_units,
        "ucs4_units": ucs4_units,
    }


def environment(engine: dict, python: dict, manifest: dict) -> dict:
    ucs = engine[("ucs", 1)]
    py = python[("unirex", 1)]
    measured_at = manifest.get("generated_at")
    if not measured_at:
        measured_at = datetime.now().astimezone().isoformat(timespec="seconds")

    loads = [
        float(point["load_before"])
        for point in list(engine.values()) + list(python.values())
        if isinstance(point.get("load_before"), (int, float))
    ]
    # Python frequency samples are diagnostic and often show an idle core;
    # report the engine-only range, which is the stable one.
    freqs = []
    for point in engine.values():
        values = point.get("freq_after_mhz") or []
        workers = int(point.get("threads", len(values)) or 0)
        for value in values[:workers]:
            if value:
                freqs.append(int(value))
    efficiencies = []
    for point in engine.values():
        for value in point.get("worker_efficiencies") or []:
            efficiencies.append(float(value))

    return {
        "cpu": manifest.get("cpu") or ucs.get("cpu") or py.get("cpu") or "unknown CPU",
        "os": manifest.get("os") or ucs.get("os") or "unknown OS",
        "python": manifest.get("python") or py.get("python") or "unknown Python",
        "rustc": manifest.get("rustc") or ucs.get("rustc") or "unknown Rust",
        "commit": manifest.get("commit") or "workspace snapshot",
        "measured_at": measured_at,
        "date": measured_at[:10],
        "load_min": min(loads) if loads else None,
        "load_max": max(loads) if loads else None,
        "freq_min": min(freqs) if freqs else None,
        "freq_max": max(freqs) if freqs else None,
        "efficiency_min": min(efficiencies) if efficiencies else None,
        "efficiency_max": max(efficiencies) if efficiencies else None,
        "limit": manifest.get("limit"),
        "repeat": int(ucs.get("repeat", 0) or 0),
        "cpu_list": manifest.get("cpu_list") or ucs.get("cpu_list") or [],
    }


def nice_ticks(vmax: float, count: int = 5) -> list[float]:
    if vmax <= 0:
        return [0.0, 1.0]
    raw = vmax / count
    magnitude = 10.0 ** math.floor(math.log10(raw))
    step = magnitude
    for multiplier in (1.0, 2.0, 2.5, 5.0, 10.0):
        step = multiplier * magnitude
        if step >= raw:
            break
    top = math.ceil(vmax / step) * step
    return [index * step for index in range(int(round(top / step)) + 1)]


def tick_label(value: float) -> str:
    if value == 0:
        return "0"
    if value >= 1:
        return f"{value:g}"
    return f"{value:.3f}".rstrip("0").rstrip(".")


def line_chart_svg(
    path: Path,
    title: str,
    xs: list[int],
    series: list[tuple[str, str, str | None, list[float]]],
    y_label: str = "wall time for 1M posts (s)",
) -> None:
    width, height = 920, 520
    left, right, top, bottom = 82, 190, 64, 68
    plot_w = width - left - right
    plot_h = height - top - bottom
    values = [value for _, _, _, points in series for value in points]
    vmax = max(values)
    ticks = nice_ticks(vmax * 1.05, 5)
    ytop = ticks[-1]

    def sx(x: int) -> float:
        if len(xs) == 1:
            return left + plot_w / 2
        return left + (x - xs[0]) / (xs[-1] - xs[0]) * plot_w

    def sy(value: float) -> float:
        return top + plot_h - (value / ytop) * plot_h

    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
        f'viewBox="0 0 {width} {height}" role="img" aria-label="{html.escape(title)}">',
        '<rect width="100%" height="100%" fill="white"/>',
        f'<text x="{left}" y="34" font-family="DejaVu Sans, sans-serif" '
        f'font-size="18" font-weight="700" fill="#1f2328">{html.escape(title)}</text>',
    ]

    for tick in ticks:
        y = sy(tick)
        parts.append(
            f'<line x1="{left}" y1="{y:.1f}" x2="{left + plot_w}" y2="{y:.1f}" '
            f'stroke="#d9d9d9" stroke-width="1"/>'
        )
        parts.append(
            f'<text x="{left - 12}" y="{y + 4:.1f}" text-anchor="end" '
            f'font-family="DejaVu Sans, sans-serif" font-size="11" fill="#555555">'
            f"{tick_label(tick)}</text>"
        )

    parts.append(
        f'<line x1="{left}" y1="{top + plot_h}" x2="{left + plot_w}" '
        f'y2="{top + plot_h}" stroke="#666666" stroke-width="1.4"/>'
    )
    parts.append(
        f'<line x1="{left}" y1="{top}" x2="{left}" y2="{top + plot_h}" '
        f'stroke="#666666" stroke-width="1.4"/>'
    )
    for x in xs:
        px = sx(x)
        parts.append(
            f'<line x1="{px:.1f}" y1="{top + plot_h}" x2="{px:.1f}" '
            f'y2="{top + plot_h + 6}" stroke="#666666" stroke-width="1"/>'
        )
        parts.append(
            f'<text x="{px:.1f}" y="{top + plot_h + 25}" text-anchor="middle" '
            f'font-family="DejaVu Sans, sans-serif" font-size="12" fill="#333333">'
            f"{x}</text>"
        )

    for label, color, dash, points in series:
        coords = " ".join(f"{sx(x):.1f},{sy(value):.1f}" for x, value in zip(xs, points))
        dash_attr = f' stroke-dasharray="{dash}"' if dash else ""
        parts.append(
            f'<polyline points="{coords}" fill="none" stroke="{color}" '
            f'stroke-width="2.6" stroke-linejoin="round" stroke-linecap="round"'
            f'{dash_attr}/>'
        )
        for x, value in zip(xs, points):
            parts.append(
                f'<circle cx="{sx(x):.1f}" cy="{sy(value):.1f}" r="4.2" '
                f'fill="white" stroke="{color}" stroke-width="2.2"/>'
            )

    legend_x = left + plot_w + 26
    legend_y = top + 14
    for index, (label, color, dash, _) in enumerate(series):
        y = legend_y + index * 25
        dash_attr = f' stroke-dasharray="{dash}"' if dash else ""
        parts.append(
            f'<line x1="{legend_x}" y1="{y}" x2="{legend_x + 32}" y2="{y}" '
            f'stroke="{color}" stroke-width="2.6"{dash_attr}/>'
        )
        parts.append(
            f'<circle cx="{legend_x + 16}" cy="{y}" r="4" fill="white" '
            f'stroke="{color}" stroke-width="2"/>'
        )
        parts.append(
            f'<text x="{legend_x + 42}" y="{y + 4}" '
            f'font-family="DejaVu Sans, sans-serif" font-size="11" fill="#333333">'
            f"{html.escape(label)}</text>"
        )

    parts.append(
        f'<text x="{left + plot_w / 2:.1f}" y="{height - 18}" text-anchor="middle" '
        f'font-family="DejaVu Sans, sans-serif" font-size="12" fill="#333333">'
        f"worker threads</text>"
    )
    parts.append(
        f'<text x="18" y="{top + plot_h / 2:.1f}" text-anchor="middle" '
        f'transform="rotate(-90 18 {top + plot_h / 2:.1f})" '
        f'font-family="DejaVu Sans, sans-serif" font-size="12" fill="#333333">'
        f"{html.escape(y_label)}</text>"
    )
    parts.append("</svg>")
    path.write_text("\n".join(parts) + "\n")


def build_blocks(
    engine: dict, python: dict, manifest: dict, results_dir: Path
) -> tuple[list, dict, dict]:
    stats = corpus_stats(engine)
    env = environment(engine, python, manifest)
    py = {key: point for key, point in python.items()}
    eng = {key: point for key, point in engine.items()}

    def pym(engine_name: str, threads: int) -> float:
        return float(py[(engine_name, threads)]["median_s"])

    def pymin(engine_name: str, threads: int) -> float:
        return float(py[(engine_name, threads)]["min_s"])

    def em(mode: str, threads: int) -> float:
        return float(eng[(mode, threads)]["median_s"])

    repeats = env["repeat"] or eng[("ucs", 1)].get("repeat", 0)
    ucs_mb = stats["ucs_storage_bytes"] / 1e6
    utf8_mb = stats["utf8_bytes"] / 1e6
    posts = stats["posts"]
    mentions = stats["mentions"]
    posts_with = stats["posts_with"]
    pct_posts = posts_with / posts * 100.0
    py1 = pym("unirex", 1)
    re1 = pym("re", 1)
    pymin1 = pymin("unirex", 1)
    remin1 = pymin("re", 1)
    py4 = pym("unirex", 4)
    re4 = pym("re", 4)
    ucs1 = em("ucs", 1)
    bytes1 = em("bytes", 1)
    str1 = em("str", 1)
    ucs4 = em("ucs", 4)
    bytes4 = em("bytes", 4)
    str4 = em("str", 4)
    py_ratio1 = py1 / re1
    py_ratio4 = py4 / re4
    eng_ratio1 = ucs1 / bytes1
    eng_ratio4 = ucs4 / bytes4

    py_rows = []
    for threads in THREADS:
        py_rows.append(
            [
                str(threads),
                fmt_seconds(pym("unirex", threads)),
                fmt_seconds(pymin("unirex", threads)),
                fmt_seconds(pym("re", threads)),
                fmt_seconds(pymin("re", threads)),
                fmt_ratio(pym("unirex", threads) / pym("re", threads)),
            ]
        )

    eng_rows = []
    for threads in THREADS:
        eng_rows.append(
            [
                str(threads),
                fmt_seconds(em("ucs", threads)),
                fmt_seconds(em("bytes", threads)),
                fmt_seconds(em("str", threads)),
                fmt_ratio(em("ucs", threads) / em("bytes", threads)),
            ]
        )

    load_text = "not recorded"
    if env["load_min"] is not None:
        load_text = f"{env['load_min']:.2f} to {env['load_max']:.2f}"
    freq_text = "not recorded"
    if env["freq_min"] is not None:
        freq_text = f"{env['freq_min'] / 1000:.1f} to {env['freq_max'] / 1000:.1f} GHz"
    eff_text = "not recorded"
    if env["efficiency_min"] is not None:
        eff_lo = f"{env['efficiency_min']:.2f}"
        eff_hi = f"{env['efficiency_max']:.2f}"
        eff_text = eff_lo if eff_lo == eff_hi else f"{eff_lo} to {eff_hi}"
    try:
        raw_dir = results_dir.resolve().relative_to(ROOT)
    except ValueError:
        raw_dir = results_dir

    blocks = [
        ("h1", "Telegram @mention extraction: unirex vs CPython `re` and Rust `regex`"),
        (
            "p",
            "Real-world task, measured as one `findall(text)` pass over every "
            "post in the corpus. The extension matches CPython's UCS-1, UCS-2, "
            "or UCS-4 buffer directly; there is no UTF-8 conversion in the "
            "matching path. This page reports two separate measurements, built on "
            "the same pattern and the same corpus: the PyO3 extension called from "
            "Python, compared with CPython `re`, and the Rust crate called "
            "directly, compared with the `regex` crate. They are not "
            "interchangeable and their numbers are not comparable.",
        ),
        ("h2", "Two measurements, two processes"),
        (
            "table",
            ["Measurement", "Process", "What is called", "Included in the time"],
            [
                [
                    "End-to-end (first result table)",
                    f"CPython {env['python']} with the GIL",
                    "the PyO3 extension: `import unirex`, `rx.findall(text)`, "
                    "against CPython `re`",
                    "the scan, Python call overhead, list and result-string "
                    "construction",
                ],
                [
                    "Engine-only (second result table)",
                    "a Rust process, `examples/bench.rs`",
                    "the Rust crate through its `Ucs` API, against the `regex` "
                    "crate",
                    "only the scan, with a reused `Scratch`; no Python is "
                    "involved",
                ],
            ],
            "left",
        ),
        ("h2", "Methodology"),
        (
            "ul",
            [
                f"**Pattern** (Telegram username rules, 5 to 32 characters, "
                f"`[a-zA-Z0-9_]`, starting with a letter): `{PATTERN}`",
                f"**Corpus**: {posts:,} posts, {utf8_mb:.0f} MB of UTF-8 text "
                f"(mostly Arabic and Persian), {mentions:,} mentions in "
                f"{posts_with:,} posts ({pct_posts:.1f} percent of posts). The "
                f"benchmark resolves the corpus from the cache or the "
                f"[bench-data-telegram-v1 release]"
                f"(https://github.com/lorien/rurex/releases/tag/bench-data-telegram-v1) "
                f"and verifies its SHA-256 before timing.",
                f"**UCS storage**: the same posts occupy about {ucs_mb:.0f} MB when "
                f"packed as CPython UCS storage: {stats['ucs1_posts']:,} UCS-1, "
                f"{stats['ucs2_posts']:,} UCS-2, and {stats['ucs4_posts']:,} UCS-4 "
                f"posts. The engine scans these units directly.",
                "**End-to-end protocol (PyO3 extension in a Python process)**: "
                "each engine and thread point runs in a fresh Python process. The "
                "process loads the corpus, compiles the pattern once, runs untimed "
                "warm-up passes over the corpus, and then performs one timed "
                "`findall` per post over its slice. Workers are pinned to distinct "
                "CPUs and released by a barrier. The reported wall time is the "
                "maximum across workers.",
                "**Engine-only protocol (Rust crate, no Python)**: each post is "
                "scanned exactly once per run with a reused `Scratch`, called "
                "directly from `examples/bench.rs`. `ucs` uses the three CPython "
                "storage kinds, `bytes` uses Rust `regex::bytes` over UTF-8, and "
                "`str` uses Rust `regex` over UTF-8. An independent naive counter "
                "runs during loading and every mode must match it.",
                "**Core lease and contamination guard**: the whole matrix holds "
                "`/tmp/core4.lock`. Each point records CPU time, wall time, load, "
                "and per-CPU frequency. Runs below the efficiency floor or above "
                "the load limit are rejected and retried; a contaminated run is "
                "not published.",
                "**Timing window**: load, corpus resolution, pattern compilation, "
                "and warm-up are outside the window for both engines. The measured "
                "window starts when the barrier releases and stops when the "
                "worker's slice is finished.",
                f"**Repeats**: published numbers are medians of {repeats} timed "
                f"passes per point; the raw JSON records every run. Load was "
                f"{load_text}; engine-only frequency was {freq_text}; engine worker "
                f"efficiency was {eff_text}. Python frequency samples are kept in "
                f"the raw JSON as diagnostics.",
                f"**Interpreter**: CPython {env['python']} with the GIL. "
                f"Free-threaded CPython and other interpreter versions are not "
                f"measured on this page.",
            ],
        ),
        ("h2", "End-to-end: PyO3 extension called from Python"),
        (
            "p",
            "Both columns below are Python: the process imports the extension as "
            "`unirex` and calls `rx.findall(text)` per post, and the baseline is "
            "CPython's own `re` module. No Rust baseline appears in this table.",
        ),
        ("image", "End-to-end Python wall time by worker count", "img/telegram_end_to_end.svg"),
        (
            "table",
            [
                "Workers",
                "unirex (PyO3) median (s)",
                "unirex (PyO3) min (s)",
                "CPython re median (s)",
                "CPython re min (s)",
                "unirex/re",
            ],
            py_rows,
        ),
        (
            "p",
            "A ratio below 1 means `unirex` is faster. At one thread, "
            f"`unirex` is {fmt_seconds(py1)} s median and {fmt_seconds(pymin1)} s "
            f"minimum; CPython `re` is {fmt_seconds(re1)} s median and "
            f"{fmt_seconds(remin1)} s minimum. That is {pct_faster(py1, re1)} at the "
            f"median and {pct_faster(pymin1, remin1)} at the minimum.",
        ),
        (
            "p",
            "The GIL prevents meaningful scaling beyond one thread. The two- to "
            "four-worker medians stay close to the one-thread time, and the "
            "minimums are noisy because each scan is short and Python result "
            "construction is serialized. The one-thread result is the meaningful "
            "end-to-end comparison on this page.",
        ),
        ("h2", "Engine-only: Rust crate, no Python"),
        (
            "p",
            "Nothing in this table runs Python. `examples/bench.rs` calls the "
            "Rust crate directly through its `Ucs` haystack API and compares it "
            "with the `regex` crate over UTF-8. The PyO3 extension is not "
            "involved and these numbers exclude Python call overhead.",
        ),
        ("image", "Engine-only wall time by worker count", "img/telegram_engine_only.svg"),
        (
            "table",
            [
                "Workers",
                "unirex crate, UCS (s)",
                "regex crate, bytes (s)",
                "regex crate, str (s)",
                "UCS/bytes",
            ],
            eng_rows,
        ),
        (
            "p",
            "Engine-only and end-to-end numbers answer different questions. At "
            f"one thread, UCS is {fmt_seconds(ucs1)} s median versus "
            f"{fmt_seconds(bytes1)} s for Rust `regex` bytes and "
            f"{fmt_seconds(str1)} s for Rust `regex` str. The UCS path is "
            f"{pct_faster(ucs1, bytes1)} than the bytes baseline at one thread.",
        ),
        (
            "p",
            f"At four threads the picture changes: UCS is {fmt_seconds(ucs4)} s "
            f"versus {fmt_seconds(bytes4)} s for bytes and {fmt_seconds(str4)} s "
            f"for str, or {fmt_ratio(eng_ratio4)} times the bytes time. The UCS "
            f"path streams about {ucs_mb:.0f} MB of storage against "
            f"{utf8_mb:.0f} MB of UTF-8, and each worker keeps its own DFA cache. "
            f"This is the remaining four-thread scaling gap. It does not change "
            f"the one-thread end-to-end result.",
        ),
        ("h2", "Reading the numbers"),
        (
            "p",
            "In the end-to-end table `unirex` is the PyO3 extension; in the "
            "engine-only table it is the Rust crate. Whenever a row is ambiguous, "
            "the interface named in its header decides what was measured.",
        ),
        (
            "p",
            "Parity here means the one-thread end-to-end Telegram result, not "
            "general `re` compatibility. The supported dialect is still a strict "
            "subset: `\\w`, `\\d`, `\\s`, `\\b`, `$`, flags, lazy repetition, and "
            "capture values are not implemented yet, and only group 0 is exposed. "
            "A user who needs those constructs must continue to use CPython `re`.",
        ),
        (
            "p",
            "The engine-only numbers exclude Python call overhead, list "
            "construction, and result strings. They are useful for understanding "
            "the matcher, but they are not directly comparable with the "
            "end-to-end table.",
        ),
        ("h2", "Free-threaded Python"),
        (
            "p",
            "Not measured yet. The binding is verified on CPython 3.13 with the "
            "GIL; free-threaded CPython 3.14t and CPython 3.10 through 3.15 are "
            "open verification tasks. The report generator rejects "
            "free-threaded points until this section is implemented, so a mixed "
            "matrix cannot silently publish GIL prose.",
        ),
        ("h2", "Reproducing"),
        (
            "code",
            "sh",
            "cargo build --release --example bench\n"
            "cargo build --release --features extension-module\n"
            "mkdir -p var/py\n"
            "cp target/release/libunirex.so var/py/unirex.so\n"
            "python3 tools/telegram_benchmark.py --cpu-list 0,2,4,6 "
            "--repeat 5 --warmup-seconds 2\n"
            "\n"
            "# Re-render the report from saved JSON without rerunning:\n"
            "python3 tools/telegram_report.py --results-dir var/bench-results/telegram",
        ),
        ("hr",),
        (
            "p",
            f"*Measured {env['measured_at']} on {env['cpu']}; Python "
            f"{env['python']}; compiler {env['rustc']}; source {env['commit']}. "
            f"Raw JSON lives under "
            f"`{raw_dir}` and is not committed. "
            f"The standalone HTML twin is generated at "
            f"`docs/telegram-benchmark.html` and is gitignored.*",
        ),
    ]
    return blocks, stats, env


def render_markdown(blocks: list) -> str:
    lines: list[str] = []
    for block in blocks:
        kind = block[0]
        if kind == "h1":
            lines.extend([f"# {block[1]}", ""])
        elif kind == "h2":
            lines.extend([f"## {block[1]}", ""])
        elif kind == "p":
            lines.extend([block[1], ""])
        elif kind == "ul":
            for item in block[1]:
                lines.append(f"- {item}")
            lines.append("")
        elif kind == "table":
            headers, rows = block[1], block[2]
            align = block[3] if len(block) > 3 else "right"
            rule = "---:" if align == "right" else "---"
            lines.append("| " + " | ".join(headers) + " |")
            lines.append("|" + "|".join(rule for _ in headers) + "|")
            for row in rows:
                cells = [cell.replace("|", "\\|") for cell in row]
                lines.append("| " + " | ".join(cells) + " |")
            lines.append("")
        elif kind == "image":
            alt, path = block[1], block[2]
            lines.extend([f"![{alt}]({path})", ""])
        elif kind == "code":
            language, text = block[1], block[2]
            lines.extend([f"```{language}", text.rstrip(), "```", ""])
        elif kind == "hr":
            lines.extend(["---", ""])
        else:
            raise ValueError(f"unknown block {kind}")
    return "\n".join(lines).rstrip() + "\n"


def inline_html(text: str) -> str:
    escaped = html.escape(text, quote=False)
    escaped = re.sub(r"`([^`]+)`", r"<code>\1</code>", escaped)
    escaped = re.sub(r"\*\*([^*]+)\*\*", r"<strong>\1</strong>", escaped)
    escaped = re.sub(r"(?<!\*)\*([^*]+)\*(?!\*)", r"<em>\1</em>", escaped)
    escaped = re.sub(
        r"\[([^\]]+)\]\(([^)]+)\)", r'<a href="\2">\1</a>', escaped
    )
    return escaped


HTML_TEMPLATE = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  body {{ font-family: -apple-system, BlinkMacSystemFont, "Segoe UI",
         Helvetica, Arial, sans-serif; max-width: 980px; margin: 2rem auto;
         padding: 0 1rem; color: #1f2328; line-height: 1.6; }}
  code {{ background: #f6f8fa; padding: .2em .4em; border-radius: 6px;
         font-size: 85%; font-family: ui-monospace, Menlo, Consolas,
         monospace; }}
  pre {{ background: #f6f8fa; padding: 14px 16px; border-radius: 8px;
        overflow-x: auto; }}
  pre code {{ background: none; padding: 0; }}
  table {{ border-collapse: collapse; margin: 1rem 0; width: 100%; }}
  th, td {{ border: 1px solid #d1d9e0; padding: 6px 10px;
           text-align: right; }}
  th {{ background: #f6f8fa; }}
  td:first-child, th:first-child {{ text-align: left; }}
  img, svg {{ max-width: 100%; height: auto; }}
  figure {{ margin: 1.2rem 0; }}
  figcaption {{ color: #57606a; font-size: 90%; text-align: center; }}
  h1, h2 {{ border-bottom: 1px solid #d1d9e0; padding-bottom: .3em; }}
</style>
</head>
<body>
<div class="banner" style="background:#fff8c5;border:1px solid #d4a72c;
     border-radius:8px;padding:.6rem 1rem;margin-bottom:2rem;font-size:90%;">
Generated by <code>tools/telegram_report.py</code> from the benchmark JSON.
The Markdown source is <code>docs/telegram-benchmark.md</code>; regenerate with
<code>python3 tools/telegram_report.py --results-dir var/bench-results/telegram</code>.
</div>
{body}
</body>
</html>
"""


def render_html(blocks: list, title: str, out_dir: Path) -> str:
    body: list[str] = []
    for block in blocks:
        kind = block[0]
        if kind == "h1":
            body.append(f"<h1>{inline_html(block[1])}</h1>")
        elif kind == "h2":
            body.append(f"<h2>{inline_html(block[1])}</h2>")
        elif kind == "p":
            body.append(f"<p>{inline_html(block[1])}</p>")
        elif kind == "ul":
            body.append("<ul>")
            for item in block[1]:
                body.append(f"<li>{inline_html(item)}</li>")
            body.append("</ul>")
        elif kind == "table":
            headers, rows = block[1], block[2]
            body.append("<table><thead><tr>")
            for header in headers:
                body.append(f"<th>{inline_html(header)}</th>")
            body.append("</tr></thead><tbody>")
            for row in rows:
                body.append("<tr>")
                for cell in row:
                    body.append(f"<td>{inline_html(cell)}</td>")
                body.append("</tr>")
            body.append("</tbody></table>")
        elif kind == "image":
            alt, path = block[1], block[2]
            svg = (out_dir / path).read_text()
            body.append(
                f"<figure>{svg}<figcaption>{html.escape(alt)}</figcaption></figure>"
            )
        elif kind == "code":
            language, text = block[1], block[2]
            body.append(
                f'<pre><code class="language-{language}">'
                f"{html.escape(text.rstrip())}</code></pre>"
            )
        elif kind == "hr":
            body.append("<hr>")
        else:
            raise ValueError(f"unknown block {kind}")
    return HTML_TEMPLATE.format(title=html.escape(title), body="\n".join(body))


def generate(results_dir: Path, out_dir: Path, write_html: bool = True) -> None:
    engine, python, manifest = load_results(results_dir)
    validate_points(engine, python)
    blocks, _stats, _env = build_blocks(engine, python, manifest, results_dir)
    img_dir = out_dir / "img"
    img_dir.mkdir(parents=True, exist_ok=True)
    out_dir.mkdir(parents=True, exist_ok=True)

    py_xs = list(THREADS)
    py_series = [
        (
            "unirex (PyO3)",
            COLOR_EXP,
            None,
            [float(python[("unirex", t)]["median_s"]) for t in py_xs],
        ),
        (
            "CPython re",
            COLOR_RE,
            None,
            [float(python[("re", t)]["median_s"]) for t in py_xs],
        ),
    ]
    eng_series = [
        ("unirex crate (UCS)", COLOR_EXP, None, [float(engine[("ucs", t)]["median_s"]) for t in py_xs]),
        ("regex crate bytes", COLOR_RE, "7 4", [float(engine[("bytes", t)]["median_s"]) for t in py_xs]),
        ("regex crate str", COLOR_STR, "2 4", [float(engine[("str", t)]["median_s"]) for t in py_xs]),
    ]
    line_chart_svg(
        img_dir / "telegram_end_to_end.svg",
        "Telegram @mentions: end-to-end Python (PyO3 binding)",
        py_xs,
        py_series,
    )
    line_chart_svg(
        img_dir / "telegram_engine_only.svg",
        "Telegram @mentions: engine only (Rust crate)",
        py_xs,
        eng_series,
    )

    md_path = out_dir / "telegram-benchmark.md"
    md_path.write_text(render_markdown(blocks))
    print(f"wrote {md_path}")
    print(f"wrote {img_dir / 'telegram_end_to_end.svg'}")
    print(f"wrote {img_dir / 'telegram_engine_only.svg'}")
    if write_html:
        html_path = out_dir / "telegram-benchmark.html"
        html_title = blocks[0][1].replace("`", "")
        html_path.write_text(render_html(blocks, html_title, out_dir))
        print(f"wrote {html_path}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", type=Path, default=DEFAULT_RESULTS)
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_OUT)
    parser.add_argument("--no-html", action="store_true")
    args = parser.parse_args()
    generate(args.results_dir, args.out_dir, write_html=not args.no_html)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
