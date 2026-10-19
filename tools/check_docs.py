#!/usr/bin/env python3
"""Check the documentation tree for links, markers, and stale names.

Run from anywhere:

    python3 tools/check_docs.py

Exit status is 0 when every check passes and 1 otherwise. The checks are
deliberately dependency-free: no Markdown parser, no third-party packages.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Paths whose Markdown is checked for local links.
LINK_ROOTS = ("README.md", "spec/skills", "spec/docs", "spec/report", "docs")

LINK = re.compile(r"\]\(([^)\s]+)\)")
FENCE = re.compile(r"^\s*(```|~~~)")
MARKER = re.compile(r"^- \[([a-z][a-z-]*)\]")
REPORT_NAME = re.compile(r"^report-\d+-[a-z0-9-]+\.md$")
REPORT_TITLE = "## Report on task: "

ALLOWED_MARKERS = {"open", "acted", "needs-decision"}
ALLOWED_STATUSES = {"new", "done", "deferred"}

# Identifiers that must not appear in code or current documentation. Each entry
# maps a forbidden string to the paths where it may still appear, because a
# report recording a removal, or the rule that forbids the name, has to name it.
FORBIDDEN = {
    "exp_rurex": {"spec/report", "spec/docs/conventions.md", "tools/check_docs.py"},
    "RUREX_": {"spec/report", "tools/check_docs.py"},
}

failures: list[str] = []
checks = 0


def fail(message: str) -> None:
    failures.append(message)


def md_files() -> list[Path]:
    seen: list[Path] = []
    for entry in LINK_ROOTS:
        path = ROOT / entry
        if path.is_file():
            seen.append(path)
        elif path.is_dir():
            seen.extend(sorted(path.rglob("*.md")))
    return seen


def strip_anchor(target: str) -> str:
    for sep in ("#", "?"):
        target = target.split(sep, 1)[0]
    return target


def check_links() -> None:
    global checks
    checks += 1
    for path in md_files():
        in_fence = False
        for number, line in enumerate(path.read_text().splitlines(), 1):
            if FENCE.match(line):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            for match in LINK.finditer(line):
                target = match.group(1)
                if target.startswith(("http://", "https://", "mailto:", "#")):
                    continue
                local = strip_anchor(target)
                if not local:
                    continue
                resolved = (path.parent / local).resolve()
                if not resolved.exists():
                    rel = path.relative_to(ROOT)
                    fail(f"{rel}:{number}: broken local link {target}")


def check_index_covers_docs() -> None:
    global checks
    checks += 1
    index = ROOT / "spec/docs/index.md"
    text = index.read_text()
    listed = {strip_anchor(m.group(1)) for m in LINK.finditer(text)}
    for path in sorted((ROOT / "spec/docs").rglob("*.md")):
        if path.name == "index.md" and path.parent == index.parent:
            continue
        rel = path.relative_to(index.parent).as_posix()
        if rel not in listed:
            fail(f"spec/docs/index.md: {rel} is not catalogued")


def check_forbidden_identifiers() -> None:
    global checks
    checks += 1
    tracked = [p for p in ROOT.rglob("*") if p.is_file()]
    skip = {".git", "target", "var", "docs/img"}
    for path in tracked:
        rel = path.relative_to(ROOT).as_posix()
        if any(rel == s or rel.startswith(s + "/") for s in skip):
            continue
        if path.suffix in {".html", ".svg", ".gz", ".so", ".whl", ".png"}:
            continue
        try:
            text = path.read_text()
        except (UnicodeDecodeError, OSError):
            continue
        for name, allowed in FORBIDDEN.items():
            if any(rel == a or rel.startswith(a + "/") for a in allowed):
                continue
            for number, line in enumerate(text.splitlines(), 1):
                if name in line:
                    fail(f"{rel}:{number}: forbidden identifier {name}")


def check_report_format() -> None:
    global checks
    checks += 1
    reports = sorted((ROOT / "spec/report").rglob("*.md"))
    if not reports:
        fail("spec/report: no reports found")
        return
    for path in reports:
        rel = path.relative_to(ROOT).as_posix()
        if not REPORT_NAME.match(path.name):
            fail(f"{rel}: filename does not match report-<epoch>-<slug>.md")
        first = path.read_text().splitlines()[0] if path.read_text() else ""
        if not first.startswith(REPORT_TITLE):
            fail(f"{rel}:1: first line must start with {REPORT_TITLE!r}")
        for number, line in enumerate(path.read_text().splitlines(), 1):
            match = MARKER.match(line)
            if match and match.group(1) not in ALLOWED_MARKERS:
                fail(f"{rel}:{number}: unknown marker [{match.group(1)}]")


def check_plan_format() -> None:
    global checks
    checks += 1
    plan = ROOT / "spec/docs/plan.md"
    text = plan.read_text()
    records = re.split(r"^## ", text, flags=re.M)[1:]
    if not records:
        return
    for record in records:
        title = record.splitlines()[0].strip()
        if not re.search(r"^Status: ", record, flags=re.M):
            fail(f"spec/docs/plan.md: record {title!r} has no Status line")
        if not re.search(r"^Priority: ", record, flags=re.M):
            fail(f"spec/docs/plan.md: record {title!r} has no Priority line")
        match = re.search(r"^Status: (\S+)", record, flags=re.M)
        if match and match.group(1) not in ALLOWED_STATUSES:
            fail(f"spec/docs/plan.md: record {title!r} has status {match.group(1)!r}")


def check_versions_match() -> None:
    global checks
    checks += 1
    crate = re.search(r'^version = "([^"]+)"', (ROOT / "Cargo.toml").read_text(), re.M)
    wheel = re.search(r'^version = "([^"]+)"', (ROOT / "pyproject.toml").read_text(), re.M)
    if not crate or not wheel:
        fail("Cargo.toml and pyproject.toml must both declare a version")
        return
    if crate.group(1) != wheel.group(1):
        fail(
            f"Cargo.toml version {crate.group(1)} != pyproject.toml version {wheel.group(1)}"
        )


def main() -> int:
    for check in (
        check_links,
        check_index_covers_docs,
        check_forbidden_identifiers,
        check_report_format,
        check_plan_format,
        check_versions_match,
    ):
        check()
    if failures:
        print(f"docs check failed: {len(failures)} problem(s) in {checks} checks")
        for failure in failures:
            print(f"  {failure}")
        return 1
    print(f"docs check ok: {checks} checks")
    return 0


if __name__ == "__main__":
    sys.exit(main())
