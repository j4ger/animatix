#!/usr/bin/env python3
"""Measure `animatix audit` findings across the examples / web demo corpora.

This is the before/after instrument for the toolchain follow-up round. It walks
the same three corpora the docs talk about, runs the CPU-only structural tier,
and reports counts by code, by severity, and by file.

Usage (cargo-built binary expected at target/debug/animatix):

    nix develop --command python3 scripts/audit-corpus-stats.py
    nix develop --command python3 scripts/audit-corpus-stats.py --policy default
    nix develop --command python3 scripts/audit-corpus-stats.py --save /tmp/before.json
    nix develop --command python3 scripts/audit-corpus-stats.py --compare /tmp/before.json
    nix develop --command python3 scripts/audit-corpus-stats.py --json

`--compare` prints a delta against a previously saved JSON. It is a reporting
instrument, not a gate: the toolchain leaves visual judgement to a human looking
at the rendered frames, so no finding-count or error-count target is enforced.
"""

from __future__ import annotations

import argparse
import collections
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BIN = ROOT / "target" / "debug" / "animatix"
CORPORA = ("examples", "web/demos", "web/scenes")

def scene_files() -> list[pathlib.Path]:
    out: list[pathlib.Path] = []
    for corpus in CORPORA:
        for path in sorted((ROOT / corpus).rglob("*.amx")):
            rel = path.relative_to(ROOT).as_posix()
            # Component libraries and generated site copies are not standalone
            # scenes; match the exclusion in scripts/lib-scenes.sh.
            if "/lib/" in rel or "_site/" in rel:
                continue
            # Multi-file projects are driven through main.amx; their fragments
            # reference cross-file context a standalone audit cannot see.
            if path.name != "main.amx" and (path.parent / "main.amx").is_file():
                continue
            # Needs the demo plugin manifest; `gate examples` covers it.
            if path.name == "plugin_pulse.amx":
                continue
            # A pure `pub let` constant module imported by the epicycles
            # scenes, not a playable scene (no actors, zero duration).
            if path.as_posix().endswith("epicycles/fourier.amx"):
                continue
            out.append(path)
    return out


def audit(path: pathlib.Path, policy: str) -> dict | None:
    try:
        proc = subprocess.run(
            [str(BIN), "audit", str(path), "--render", "never",
             "--policy", policy, "--format", "json"],
            capture_output=True, text=True, timeout=120,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    if proc.returncode not in (0, 1):
        return None
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError:
        return None


def measure(policy: str) -> dict:
    by_code: collections.Counter[str] = collections.Counter()
    by_severity: collections.Counter[str] = collections.Counter()
    per_corpus: dict[str, dict] = {
        c: {"files": 0, "findings": 0, "errors": 0} for c in CORPORA
    }
    per_file: list[tuple[int, str]] = []
    unreadable: list[str] = []

    for path in scene_files():
        report = audit(path, policy)
        rel = path.relative_to(ROOT).as_posix()
        if report is None:
            unreadable.append(rel)
            continue
        findings = report.get("findings", [])
        summary = report.get("summary", {})
        errors = int(summary.get("errors", 0))
        corpus = next(c for c in CORPORA if rel.startswith(c + "/"))
        per_corpus[corpus]["files"] += 1
        per_corpus[corpus]["findings"] += len(findings)
        per_corpus[corpus]["errors"] += errors
        for finding in findings:
            by_code[finding.get("code", "?")] += 1
            by_severity[finding.get("severity", "?")] += 1
        per_file.append((len(findings), rel))

    per_file.sort(reverse=True)
    return {
        "policy": policy,
        "files": sum(c["files"] for c in per_corpus.values()),
        "findings": sum(c["findings"] for c in per_corpus.values()),
        "errors": sum(c["errors"] for c in per_corpus.values()),
        "by_code": dict(by_code.most_common()),
        "by_severity": dict(by_severity),
        "per_corpus": per_corpus,
        "max_per_file": per_file[0][0] if per_file else 0,
        "top_files": [{"file": f, "findings": n} for n, f in per_file[:10]],
        "unreadable": unreadable,
    }


def print_human(result: dict) -> None:
    print(f"policy={result['policy']}  files={result['files']}  "
          f"findings={result['findings']}  errors={result['errors']}  "
          f"max_per_file={result['max_per_file']}")
    print("  by corpus:")
    for name, counts in result["per_corpus"].items():
        print(f"    {name:12} files={counts['files']:3} "
              f"findings={counts['findings']:6} errors={counts['errors']:4}")
    print("  by severity:", result["by_severity"])
    print("  by code:", dict(list(result["by_code"].items())[:12]))
    print("  top files:", [(f["file"], f["findings"]) for f in result["top_files"][:6]])
    if result["unreadable"]:
        print("  UNREADABLE:", result["unreadable"][:10])


def print_compare(before: dict, after: dict) -> int:
    print(f"findings {before['findings']} -> {after['findings']} "
          f"(delta {after['findings'] - before['findings']:+d})")
    print(f"errors   {before['errors']} -> {after['errors']}")
    print(f"max/file {before['max_per_file']} -> {after['max_per_file']}")
    for code in sorted(set(before["by_code"]) | set(after["by_code"])):
        b, a = before["by_code"].get(code, 0), after["by_code"].get(code, 0)
        if b or a:
            print(f"  {code:28} {b:6} -> {a:6}  ({a - b:+d})")
    print("  (reporting only — no finding-count or error-count target is enforced)")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--policy", default="default",
                        choices=("default", "site"))
    parser.add_argument("--save", type=pathlib.Path, help="write the result JSON here")
    parser.add_argument("--compare", type=pathlib.Path,
                        help="compare against a previously saved JSON")
    parser.add_argument("--json", action="store_true", help="print result JSON to stdout")
    args = parser.parse_args()

    if not BIN.is_file():
        print(f"error: {BIN} not found — build it first", file=sys.stderr)
        return 2

    result = measure(args.policy)
    if args.save:
        args.save.write_text(json.dumps(result, indent=2) + "\n")
        print(f"saved {args.save}")
    if args.compare and args.compare.is_file():
        before = json.loads(args.compare.read_text())
        print_human(result)
        return print_compare(before, result)
    if args.json:
        print(json.dumps(result, indent=2))
    else:
        print_human(result)
    return 0


if __name__ == "__main__":
    sys.exit(main())
