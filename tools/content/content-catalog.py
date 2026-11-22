#!/usr/bin/env python3
"""Inventory AMX content without moving it or changing publication.

Astro owns publication. This report inventories source files, import edges,
media metadata, and literal references for maintenance.
"""
from __future__ import annotations

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import posixpath
import re
import subprocess

ROOT = Path(__file__).resolve().parents[2]
IMPORT = re.compile(r'^\s*import\s+"([^"]+)"', re.M)


def paths() -> list[str]:
    result = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
    ).decode().split("\0")
    return sorted({p for p in result if p and (ROOT / p).is_file()})


def inferred_kind(path: str) -> str:
    if "/lib/" in path:
        return "library"
    if "/benches/" in path or path.startswith(("web/tools/fixtures/web-runtime-perf/", "web/_perf_ab/")):
        return "benchmark"
    if "/fixtures/" in path or "/broken_corpus/" in path:
        return "fixture"
    if path.startswith("web/dev/"):
        return "experiment"
    return "example"


def build() -> dict:
    all_paths = paths()
    sources = [p for p in all_paths if p.endswith(".amx")]
    current_previews = json.loads((ROOT / "web/content/preview-manifest.json").read_text())["works"]
    current_sources = {"web/content/" + w["source"]: w["id"] for w in current_previews}
    # Literal full-path references are useful evidence, not a complete dependency
    # graph: shell discovery, aliases and constructed URLs need manual review.
    reference_texts = {}
    for path in all_paths:
        if Path(path).suffix in {".rs", ".py", ".sh", ".html", ".js", ".md"}:
            reference_texts[path] = (ROOT / path).read_text(errors="replace")
    inventory = []
    for source in sources:
        text = (ROOT / source).read_text()
        imports = []
        for raw in IMPORT.findall(text):
            resolved = posixpath.normpath(posixpath.join(posixpath.dirname(source), raw))
            imports.append({"specifier": raw, "source": resolved, "exists": resolved in sources})
        source_hash = hashlib.sha256((ROOT / source).read_bytes()).hexdigest()
        kind = "showcase" if source in current_sources else inferred_kind(source)
        module = "/lib/" in source
        sibling_main = (ROOT / source).with_name("main.amx")
        fragment = sibling_main.exists() and Path(source).name != "main.amx"
        record = {
            "source": source,
            "sha256": source_hash,
            "kind": kind,
            "role": "module" if module else "fragment" if fragment else "entry-candidate",
            "current_work": current_sources.get(source),
            "imports": imports,
            "literal_references": [p for p, body in reference_texts.items() if source in body and p != source],
        }
        inventory.append(record)
    return {
        "version": 1,
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip(),
        "scope": "tracked and non-ignored untracked AMX; working tree content",
        "counts": dict(sorted(Counter(x["kind"] for x in inventory).items())),
        "total": len(inventory),
        "current_content_registry": "web/app/src/lib/content.ts",
        "current_technique_registry": "web/app/src/lib/techniques.ts",
        "items": inventory,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="check the inventory and report counts")
    parser.add_argument("--output", type=Path, help="write JSON inventory instead of stdout")
    args = parser.parse_args()
    try:
        report = build()
    except (ValueError, KeyError) as error:
        parser.error(str(error))
    output = json.dumps(report, indent=2, ensure_ascii=False) + "\n"
    if args.output:
        args.output.write_text(output)
    elif not args.check:
        print(output, end="")
    if args.check:
        print(f"Catalog valid: {report['total']} AMX sources; {report['counts']}")


if __name__ == "__main__":
    main()
