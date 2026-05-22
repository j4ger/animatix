#!/usr/bin/env python3
"""
lint_theme.py — Scan for hard-coded color values in Animatix GUI sources.

Usage:
    python3 scripts/lint_theme.py

Exit code:
    0 — no violations found
    1 — one or more hard-coded color values detected
"""

import os
import re
import sys

GUI_SRC = os.path.join(
    os.path.dirname(__file__), "..", "crates", "animatix-gui", "src"
)

# Patterns that represent hard-coded color values we want to flag.
PATTERNS = [
    re.compile(r"Color32::from_rgb\("),
    re.compile(r"Color32::from_rgba_premultiplied\("),
    re.compile(r"Color32::from_rgba_unmultiplied\("),
]

# Files/patterns to exclude.
EXCLUDED_FILES = {"design_tokens.rs"}
EXCLUDED_DIRS = {"target"}


def lint_file(filepath: str) -> list[str]:
    """Return a list of violation strings for a single file."""
    rel = os.path.relpath(filepath, start=os.path.join(os.path.dirname(__file__), ".."))
    violations: list[str] = []
    with open(filepath, encoding="utf-8", errors="replace") as fh:
        for lineno, line in enumerate(fh, start=1):
            for pat in PATTERNS:
                if pat.search(line):
                    violations.append(f"{rel}:{lineno}: {line.rstrip()}")
    return violations


def main() -> int:
    root = os.path.abspath(GUI_SRC)
    if not os.path.isdir(root):
        print(f"ERROR: source directory not found — {root}", file=sys.stderr)
        return 1

    all_violations: list[str] = []

    for dirpath, dirnames, filenames in os.walk(root):
        # Skip excluded directories
        dirnames[:] = [d for d in dirnames if d not in EXCLUDED_DIRS]

        for fn in filenames:
            if not fn.endswith(".rs"):
                continue
            if fn in EXCLUDED_FILES:
                continue
            filepath = os.path.join(dirpath, fn)
            all_violations.extend(lint_file(filepath))

    if all_violations:
        print("Hard-coded color violations found (use design_tokens.rs instead!):")
        for v in all_violations:
            print(v)
        return 1

    print("No hard-coded color violations found — all colors use design tokens.")
    return 0


if __name__ == "__main__":
    sys.exit(main())