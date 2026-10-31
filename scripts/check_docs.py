#!/usr/bin/env python3
"""Documentation consistency checks run in CI.

1. Every relative link in a Markdown file points to a file that exists.
2. The ADR index (docs/adr/README.md) lists every ADR file exactly once,
   lists no missing file, and its status column matches each ADR's
   "Status:" line.
3. "Current ABI" and "current save format" claims in Markdown under core/
   and apps/ match ABI_VERSION and FORMAT_VERSION in the code, and the
   ws-ffi, ws-persist and Unity READMEs each state the current value.

Uses only the Python standard library. Exits non-zero on any failure.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)\)")
FENCE = re.compile(r"^\s*(```|~~~)")
SKIP_DIRS = {".git", "target", "node_modules", "Library", "Temp"}


def markdown_files():
    for path in sorted(ROOT.rglob("*.md")):
        if not SKIP_DIRS.intersection(path.relative_to(ROOT).parts):
            yield path


def check_links(errors):
    for path in markdown_files():
        in_fence = False
        for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
            if FENCE.match(line):
                in_fence = not in_fence
                continue
            if in_fence:
                continue
            for target in LINK.findall(line):
                if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("#"):
                    continue  # external URL, mailto, or same-file anchor
                file_part = target.split("#", 1)[0]
                if not file_part:
                    continue
                resolved = (path.parent / file_part).resolve()
                if not resolved.exists():
                    rel = path.relative_to(ROOT)
                    errors.append(f"{rel}:{lineno}: broken link to {target}")


def adr_status(path):
    for line in path.read_text(encoding="utf-8").splitlines():
        m = re.match(r"^- Status:\s*(.+)$", line)
        if m:
            return m.group(1).strip()
    return None


def check_adr_index(errors):
    adr_dir = ROOT / "docs" / "adr"
    files = {p.name: p for p in adr_dir.glob("[0-9][0-9][0-9][0-9]-*.md") if not p.name.startswith("0000-")}
    index = (adr_dir / "README.md").read_text(encoding="utf-8")
    rows = re.findall(r"^\|\s*\[(\d{4})\]\(([^)]+)\)\s*\|[^|]*\|\s*([^|]+?)\s*\|\s*$", index, re.M)
    seen = {}
    for number, target, status in rows:
        if target in seen:
            errors.append(f"docs/adr/README.md: ADR {number} listed twice")
        seen[target] = status
        if not target.startswith(number + "-"):
            errors.append(f"docs/adr/README.md: row {number} links to {target}")
        if target not in files:
            errors.append(f"docs/adr/README.md: lists missing file {target}")
            continue
        file_status = adr_status(files[target])
        if file_status is None:
            errors.append(f"docs/adr/{target}: no '- Status:' line")
        elif not file_status.startswith(status):
            errors.append(f"docs/adr/{target}: status '{file_status}' does not match index '{status}'")
    for name in sorted(set(files) - set(seen)):
        errors.append(f"docs/adr/README.md: {name} is not listed in the index")


ABI_CONST = ("core/crates/ws-ffi/src/lib.rs", r"pub const ABI_VERSION: u32 = (\d+);")
FORMAT_CONST = ("core/crates/ws-persist/src/header.rs", r"pub const FORMAT_VERSION: u32 = (\d+);")
CLAIMS = [
    ("ABI", re.compile(r"current ABI:?\s*\**\s*(\d+)", re.I)),
    ("format", re.compile(r"current save format:?\s*\**\s*(\d+)", re.I)),
    ("format", re.compile(r"writers (?:always )?write format (\d+)", re.I)),
]
REQUIRED = [
    ("core/crates/ws-ffi/README.md", "ABI", "Current ABI: **{}**"),
    ("apps/unity/README.md", "ABI", "Current ABI: **{}**"),
    ("core/crates/ws-persist/README.md", "format", "Current save format: **{}**"),
]


def constant(root, rel, pattern):
    m = re.search(pattern, (root / rel).read_text(encoding="utf-8"))
    if not m:
        raise SystemExit(f"error: {rel}: constant not found ({pattern})")
    return int(m.group(1))


def check_versions(errors, root=ROOT):
    current = {"ABI": constant(root, *ABI_CONST), "format": constant(root, *FORMAT_CONST)}
    for top in ("core", "apps"):
        for path in sorted((root / top).rglob("*.md")):
            rel = path.relative_to(root)
            if SKIP_DIRS.intersection(rel.parts):
                continue
            for lineno, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
                for kind, pattern in CLAIMS:
                    for found in pattern.findall(line):
                        if int(found) != current[kind]:
                            errors.append(f"{rel}:{lineno}: says {kind} {found}, code has {current[kind]}")
    for rel, kind, marker in REQUIRED:
        path = root / rel
        if path.exists() and marker.format(current[kind]) not in path.read_text(encoding="utf-8"):
            errors.append(f"{rel}: missing '{marker.format(current[kind])}'")


def main():
    errors = []
    check_links(errors)
    check_adr_index(errors)
    check_versions(errors)
    for e in errors:
        print(f"error: {e}")
    if errors:
        print(f"{len(errors)} documentation problem(s) found")
        return 1
    print("documentation checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
