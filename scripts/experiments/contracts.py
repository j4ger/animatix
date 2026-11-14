"""Compare separate and consolidated public-contract harnesses without rebuilding."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from scripts.experiments.common import ExperimentFailure, ExperimentOutput, host_snapshot, run_timed, source_fingerprint


def targets(build_log):
    found = {}
    for line in Path(build_log).read_text().splitlines():
        try:
            record = json.loads(line)
        except ValueError:
            continue
        if (record.get("reason") == "compiler-artifact"
                and record.get("manifest_path", "").endswith("/crates/animatix/Cargo.toml")
                and "test" in record.get("target", {}).get("kind", [])
                and record.get("executable")):
            found[record["target"]["name"]] = record["executable"]
    if not found:
        raise ExperimentFailure("no engine integration targets in Cargo build log")
    return found


def inventory(executables, consolidate):
    cases = set()
    for name, binary in executables.items():
        result = subprocess.run([binary, "--list"], check=True, text=True, capture_output=True)
        for line in result.stdout.splitlines():
            if line.endswith(": test"):
                test = line.removesuffix(": test")
                canonical = test if consolidate else f"{name}::{test}"
                if canonical in cases:
                    raise ExperimentFailure(f"duplicate contract case: {canonical}")
                cases.add(canonical)
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before-json")
    parser.add_argument("--after-json")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--rounds", type=int, default=5)
    parser.add_argument("--worker", type=Path)
    args = parser.parse_args()
    if args.worker:
        for binary in json.loads(args.worker.read_text()):
            result = subprocess.run([binary, "--test-threads=1"], check=False)
            if result.returncode:
                return result.returncode
        return 0
    before, after = targets(args.before_json), targets(args.after_json)
    if len(before) != 7 or set(after) != {"engine_contracts"}:
        raise ExperimentFailure("expected seven original targets and one engine_contracts target")
    old_cases, new_cases = inventory(before, False), inventory(after, True)
    if old_cases != new_cases:
        raise ExperimentFailure(f"coverage differs: missing={old_cases-new_cases}, added={new_cases-old_cases}")
    if args.rounds < 5:
        raise ExperimentFailure("contract-layout acceptance requires at least five paired rounds")
    hashes = {binary: hashlib.sha256(Path(binary).read_bytes()).hexdigest()
              for binary in [*before.values(), *after.values()]}
    source = source_fingerprint(ROOT)
    evidence = ExperimentOutput(ROOT, args.output, "contract-layout", {
        "source": source, "host": host_snapshot(),
        "case_ids": sorted(old_cases), "binary_sha256": hashes,
        "rounds": args.rounds, "source_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True).strip(),
        "environment": {key: os.environ.get(key) for key in
                        ("VK_ICD_FILENAMES", "RUSTFLAGS", "IN_NIX_SHELL")},
    })
    records = []
    with evidence.machine_lock():
        evidence.write_metadata()
        for arm, binaries in (("separate", before), ("consolidated", after)):
            (evidence.directory / f"{arm}.json").write_text(json.dumps(list(binaries.values())))
        for index in range(args.rounds):
            order = ("separate", "consolidated") if index % 2 == 0 else ("consolidated", "separate")
            for arm in order:
                record = run_timed(
                    f"{arm}-{index+1}", [sys.executable, __file__, "--worker",
                    str(evidence.directory / f"{arm}.json")], ROOT,
                    {"ANIMATIX_REQUIRE_GPU": "1"}, evidence.logs_dir / f"{arm}-{index+1}.log",
                    timeout_seconds=1800)
                record.update(arm=arm, round=index+1)
                evidence.append_record(record)
                if record["exit_code"] or record["timed_out"]:
                    raise ExperimentFailure(f"contract-layout run failed: {record['log_path']}")
                records.append(record)
                print(f"{arm} round {index+1}: {record['elapsed_seconds']:.3f}s", flush=True)
        for binary, digest in hashes.items():
            if hashlib.sha256(Path(binary).read_bytes()).hexdigest() != digest:
                raise ExperimentFailure(f"binary changed during measurement: {binary}")
        if source_fingerprint(ROOT) != source:
            raise ExperimentFailure("source changed during measurement")
        median = {arm: statistics.median(r["elapsed_seconds"] for r in records if r["arm"] == arm)
                  for arm in ("separate", "consolidated")}
        peak = {arm: max(r["peak_tree_rss_bytes"] for r in records if r["arm"] == arm)
                for arm in median}
        accepted = (median["consolidated"] <= median["separate"] * .9
                    and peak["consolidated"] <= peak["separate"] * 1.1)
        summary = {"case_count": len(old_cases), "median_seconds": median,
                   "peak_tree_rss_bytes": peak, "adopt_consolidation": accepted}
        (evidence.directory / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
        print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    sys.exit(main())
