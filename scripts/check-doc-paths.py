#!/usr/bin/env python3
"""Check documented repo paths, allowing only named generated artifact roots."""
from pathlib import Path
import re
import sys

# web/runtime is produced by the SDK build, absent in a clean source checkout.
# Do not exempt its children or all of web/: misspellings must still fail.
GENERATED_ROOTS = {"web/runtime"}


def stale_paths(root):
    for document in sorted((root / "docs").rglob("*.md")):
        if "history" in document.relative_to(root / "docs").parts[:-1]:
            continue
        for span in re.findall(r"`([^`\n]+)`", document.read_text()):
            if any(token in span for token in ("<", ">", "*", "{", "}", "?", "://", " ", "...")):
                continue
            path = span.rsplit(":", 1)[0]
            if not path.startswith(("crates/", "scripts/", "web/", "examples/", "docs/")):
                continue
            if path.removesuffix("/") in GENERATED_ROOTS:
                continue
            if not (root / path).exists():
                yield document, span


if __name__ == "__main__":
    failures = list(stale_paths(Path(sys.argv[1])))
    for document, span in failures:
        print(f"  STALE PATH: {document} -> {span}")
    sys.exit(bool(failures))
