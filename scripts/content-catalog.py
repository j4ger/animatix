#!/usr/bin/env python3
"""Inventory AMX content without moving it or changing publication.

The optional review registry carries stable IDs and decisions. works.json describes
the historical collection; Astro content.ts owns current pages. Defaults are inferred
purpose, never visual-quality approval.
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

ROOT = Path(__file__).resolve().parent.parent
IMPORT = re.compile(r'^\s*import\s+"([^"]+)"', re.M)
KINDS = {"example", "showcase", "fixture", "benchmark", "experiment", "library", "brand"}
DECISIONS = {"review", "keep", "merge", "rework", "fixture", "retire"}


def paths() -> list[str]:
    result = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
    ).decode().split("\0")
    return sorted({p for p in result if p and (ROOT / p).is_file()})


def inferred_kind(path: str) -> str:
    if "/lib/" in path:
        return "library"
    if path.startswith(("testfixtures/", "web/_perf_ab/")):
        return "benchmark"
    if "/fixtures/" in path or "/broken_corpus/" in path:
        return "fixture"
    if path.startswith(("dogfood/", "web/dev/")):
        return "experiment"
    if path == "web/content/scenes/hero.amx":
        return "brand"
    if path.startswith("web/scenes/"):
        return "brand"
    if path.startswith(("examples/gallery/", "examples/projects/", "web/demos/")):
        return "showcase"
    return "example"


def build() -> dict:
    all_paths = paths()
    sources = [p for p in all_paths if p.endswith(".amx")]
    registry = json.loads((ROOT / "content/catalog.json").read_text())
    entries = registry["entries"]
    if registry["version"] != 1:
        raise ValueError("unsupported registry version")
    overrides = {}
    ids = set()
    for item in entries:
        if item["source"] not in sources:
            raise ValueError(f"missing registered source: {item['source']}")
        if item["source"] in overrides or item["id"] in ids:
            raise ValueError(f"duplicate registry source or id: {item['id']}")
        ids.add(item["id"])
        overrides[item["source"]] = item
        if item.get("decision", "review") not in DECISIONS:
            raise ValueError(f"invalid decision: {item['id']}")
        if item.get("kind", "example") not in KINDS:
            raise ValueError(f"invalid kind: {item['id']}")
        for dependency in item.get("generated_from", []) + item.get("related_sources", []):
            if dependency not in sources:
                raise ValueError(f"missing related source: {dependency}")
        if item.get("generator") and not (ROOT / item["generator"]).is_file():
            raise ValueError(f"missing generator: {item['generator']}")
    works = json.loads((ROOT / "web/works.json").read_text())["works"]
    current_previews = json.loads((ROOT / "web/content/preview-manifest.json").read_text())["works"]
    current_sources = {"web/content/" + w["source"]: w["id"] for w in current_previews}
    work_ids = [w["id"] for w in works]
    if len(work_ids) != len(set(work_ids)):
        raise ValueError("duplicate works.json ID")
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
        memberships = []
        for work in works:
            explicit = {"web/demos/" + work["poster"]}
            explicit.update("web/demos/" + f["scene"] for f in work.get("figures", []))
            deck = "web/demos/" + work["deck"]
            if source in explicit or (deck.endswith("/") and source.startswith(deck)):
                memberships.append(work["id"])
        override = overrides.get(source, {})
        source_hash = hashlib.sha256((ROOT / source).read_bytes()).hexdigest()
        reviewed_hash = override.get("reviewed_sha256")
        review_stale = bool(reviewed_hash and reviewed_hash != source_hash)
        kind = override.get("kind", "showcase" if source in current_sources else inferred_kind(source))
        module = "/lib/" in source or "/scenes/" in source and source.startswith("examples/")
        sibling_main = (ROOT / source).with_name("main.amx")
        fragment = sibling_main.exists() and Path(source).name != "main.amx"
        record = {
            "id": override.get("id"),
            "source": source,
            "sha256": source_hash,
            "visual_review_stale": review_stale,
            "kind": kind,
            "kind_basis": "override" if "kind" in override else "path-inference",
            "role": "module" if module else "fragment" if fragment else "entry-candidate",
            "decision": override.get("decision", "review"),
            "status": override.get("status", "unreviewed"),
            "visual_review": override.get("visual_review", "not-reviewed"),
            "legacy_works": memberships,
            "current_work": current_sources.get(source),
            "imports": imports,
            "literal_references": [p for p, body in reference_texts.items() if source in body and p != source],
        }
        record.update({k: v for k, v in override.items() if k not in {"id", "source", "kind"}})
        if review_stale:
            record["visual_review"] = "stale-source-changed"
        inventory.append(record)
    return {
        "version": 1,
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT).decode().strip(),
        "scope": "tracked and non-ignored untracked AMX; working tree content",
        "counts": dict(sorted(Counter(x["kind"] for x in inventory).items())),
        "total": len(inventory),
        "legacy_works_manifest": "web/works.json",
        "current_content_registry": "web/app/src/lib/content.ts",
        "current_technique_registry": "web/app/src/lib/techniques.ts",
        "items": inventory,
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="validate registry and report counts")
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
