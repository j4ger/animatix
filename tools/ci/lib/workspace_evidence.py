"""Best-effort public evidence: never export raw logs, commands or environment."""
import json
from collections import Counter
import re
from pathlib import Path


def identifier(value):
    return value if isinstance(value, str) and re.fullmatch(r'[A-Za-z0-9_:.-]{1,512}', value) else None


def number(value):
    return value if type(value) in (int, float) and 0 <= value < 10**10 else None


def public_report(inventory, results, summary):
    cases = [{"id": identifier(c.get("id")), "ignored": c.get("ignored") is True}
             for c in inventory.get("cases", [])]
    harnesses = []
    for h in results.get("harnesses", []):
        harnesses.append({"package": identifier(h.get("package")),
                          "target": identifier(h.get("binary")),
                          "exit_code": number(h.get("exit_code")),
                          "timed_out": h.get("timed_out") is True,
                          "elapsed_seconds": number(h.get("elapsed_seconds")),
                          "tests": [{"name": identifier(t.get("name")),
                                     "status": t.get("status") if t.get("status") in ("ok", "FAILED", "ignored") else None}
                                    for t in h.get("observed_test_results", [])]})
    coverage = next((s.get("coverage") for s in summary.get("stages", [])
                     if s.get("name") == "test-harnesses"), None)
    safe_coverage = None
    if isinstance(coverage, dict):
        safe_coverage = {"equal": coverage.get("equal") is True,
                         "expected_count": number(coverage.get("expected_count")),
                         "actual_count": number(coverage.get("actual_count")),
                         "missing": [identifier(x) for x in coverage.get("missing", [])],
                         "unexpected": [identifier(x) for x in coverage.get("unexpected", [])]}
    counts = Counter(x for h in results.get("harnesses", []) for x in h.get("executed_ids", []))
    duplicates = [identifier(x) for x, count in counts.items() if count > 1]
    return {"duplicate_test_ids": sorted(duplicates, key=lambda x: x or ""), "schema_version": 1, "complete": "finished_at_utc" in summary and coverage is not None,
            "exit_code": number(summary.get("exit_code")), "cases": cases,
            "harnesses": harnesses, "coverage": safe_coverage}


def export_evidence(run_dir):
    def read(name):
        path = run_dir / name
        return json.loads(path.read_text()) if path.exists() else {}
    metadata = read('metadata.json')
    repo = Path(metadata['repo'])
    report = public_report(read('inventory.json'), read('harness-results.json'), read('summary.json'))
    # Only a local Git object identifier, never a remote URL or shell output.
    import subprocess
    sha = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=repo, capture_output=True,
                         text=True, timeout=5, check=True).stdout.strip()
    report['commit'] = sha if re.fullmatch(r'[0-9a-f]{40,64}', sha) else None
    report['run_id'] = identifier(run_dir.name)
    destination = repo / 'target/ci/workspace-test-evidence'
    destination.mkdir(parents=True, exist_ok=True)
    path = destination / (run_dir.name + '.json')
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    temporary.replace(path)
