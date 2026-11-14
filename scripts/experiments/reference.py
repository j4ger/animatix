#!/usr/bin/env python3
"""Compare archived handwritten doc checks with the generated reference check."""

import argparse
import json
from pathlib import Path
import statistics
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from scripts.experiments.common import ExperimentFailure, ExperimentOutput, host_snapshot, run_timed, source_fingerprint


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline-ref", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--warm-rounds", type=int, default=5)
    parser.add_argument("--cold-rounds", type=int, default=3)
    args = parser.parse_args()
    if args.warm_rounds < 5 or args.cold_rounds < 3:
        parser.error("at least five warm pairs and three cold pairs are required")
    revision = subprocess.check_output(
        ["git", "rev-parse", "--verify", f"{args.baseline_ref}^{{commit}}"], cwd=ROOT, text=True).strip()
    source = source_fingerprint(ROOT)
    evidence = ExperimentOutput(ROOT, args.output, "property-reference", {
        "source": source, "baseline_revision": revision, "host": host_snapshot(),
        "warm_rounds": args.warm_rounds, "cold_rounds": args.cold_rounds,
        "rustc": subprocess.check_output(["rustc", "-vV"], text=True),
        "scope": "old complete docs script versus current lightweight docs script plus full generated tables",
        "cold_definition": "fresh candidate Cargo target; baseline handwritten checks have no compiler target",
    })
    with evidence.machine_lock():
        evidence.write_metadata()
        baseline = evidence.owned_target("baseline-source", reset=True)
        archive = subprocess.Popen(["git", "archive", revision], cwd=ROOT, stdout=subprocess.PIPE)
        extracted = subprocess.run(["tar", "-xf", "-", "-C", str(baseline)], stdin=archive.stdout)
        archive.stdout.close()
        if archive.wait() or extracted.returncode:
            raise ExperimentFailure("could not create immutable baseline archive")
        candidate = ["bash", "-euc", "bash scripts/check-docs.sh; cargo run --locked -p animatix --example property_reference -- --check"]
        warm = evidence.owned_target("warm-target")
        prime = run_timed("reference-warmup", candidate, ROOT,
                          {"CARGO_TARGET_DIR": str(warm), "CARGO_INCREMENTAL": "0"},
                          evidence.logs_dir / "warmup.log", timeout_seconds=7200)
        evidence.append_record({"phase": "warmup", **prime})
        if prime["exit_code"]:
            raise ExperimentFailure(f"reference warmup failed: {prime['log_path']}")
        records = []
        for phase, rounds in (("cold", args.cold_rounds), ("warm", args.warm_rounds)):
            for index in range(rounds):
                order = ("handwritten", "generated") if index % 2 == 0 else ("generated", "handwritten")
                for arm in order:
                    target = warm if phase == "warm" else evidence.owned_target("cold-target", reset=True)
                    document = ROOT / "docs/properties.md"
                    before = (document.read_bytes(), document.stat().st_mtime_ns)
                    result = run_timed(f"reference-{phase}-{arm}-{index+1}",
                        ["bash", "scripts/check-docs.sh"] if arm == "handwritten" else candidate,
                        baseline if arm == "handwritten" else ROOT,
                        {"CARGO_TARGET_DIR": str(target), "CARGO_INCREMENTAL": "0"},
                        evidence.logs_dir / f"{phase}-{arm}-{index+1}.log", timeout_seconds=7200)
                    result.update(phase=phase, arm=arm, round=index+1)
                    evidence.append_record(result)
                    if result["exit_code"] or result["timed_out"]:
                        raise ExperimentFailure(f"reference check failed: {result['log_path']}")
                    if before != (document.read_bytes(), document.stat().st_mtime_ns):
                        raise ExperimentFailure("reference check modified the document")
                    records.append(result)
                    print(f"{phase}/{arm} round {index+1}: {result['elapsed_seconds']:.3f}s", flush=True)
        if source_fingerprint(ROOT) != source:
            raise ExperimentFailure("source changed during measurements")
        summary = {phase: {arm: {
            "median_seconds": statistics.median(row["elapsed_seconds"] for row in records
                                                if row["phase"] == phase and row["arm"] == arm),
            "peak_tree_rss_bytes": max(row["peak_tree_rss_bytes"] for row in records
                                       if row["phase"] == phase and row["arm"] == arm),
        } for arm in ("handwritten", "generated")} for phase in ("cold", "warm")}
        evidence.write_json("summary.json", summary)
        print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    sys.exit(main())
