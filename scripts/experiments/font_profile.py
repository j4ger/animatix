#!/usr/bin/env python3
"""Compare frozen binaries differing only in sha2 debug optimization."""

import argparse
import hashlib
import json
from pathlib import Path
import statistics
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
from scripts.experiments.common import ExperimentFailure, ExperimentOutput, host_snapshot, run_timed, source_fingerprint

CASE = "tests::caching::non_latin_text_uses_literal_fast_path"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--cli-before", type=Path)
    parser.add_argument("--cli-after", type=Path)
    parser.add_argument("--rounds", type=int, default=5)
    args = parser.parse_args()
    if args.rounds < 5:
        parser.error("at least five paired rounds are required")
    binaries = {"unoptimized": args.before.resolve(), "optimized": args.after.resolve()}
    for binary in binaries.values():
        listing = subprocess.check_output([str(binary), "--list"], text=True)
        if CASE + ": test" not in listing.splitlines():
            raise ExperimentFailure(f"the same test is missing in {binary}")
    workloads = {"text-test": binaries}
    if bool(args.cli_before) != bool(args.cli_after):
        parser.error("both CLI executables are required for a CLI comparison")
    if args.cli_before:
        workloads["cli-check"] = {"unoptimized": args.cli_before.resolve(), "optimized": args.cli_after.resolve()}
    hashes = {task: {arm: digest(binary) for arm, binary in arms.items()}
              for task, arms in workloads.items()}
    source = source_fingerprint(ROOT)
    evidence = ExperimentOutput(ROOT, args.output, "font-profile", {
        "source": source, "host": host_snapshot(), "case": CASE, "rounds": args.rounds,
        "binary_sha256": hashes,
        "binaries": {task: {arm: str(path) for arm, path in arms.items()} for task, arms in workloads.items()},
        "rustc": subprocess.check_output(["rustc", "-vV"], text=True),
        "difference": "profile.dev.package.sha2.opt-level: default -> 3 (inherited by test); release/dist unchanged",
        "measurement": "fresh test process each round; filesystem/font caches may be warm",
    })
    records = []
    with evidence.machine_lock():
        evidence.write_metadata()
        for index in range(args.rounds):
            order = list(binaries) if index % 2 == 0 else list(reversed(binaries))
            for task, arms in workloads.items():
                for arm in order:
                    command = ([str(arms[arm]), CASE, "--exact", "--test-threads=1"] if task == "text-test"
                               else [str(arms[arm]), "check", str(ROOT / "examples/templates/starter.amx")])
                    record = run_timed(f"{task}-{arm}-{index+1}", command,
                        ROOT / "crates/animatix-text", {},
                        evidence.logs_dir / f"{task}-{arm}-{index+1}.log", timeout_seconds=300)
                    record.update(task=task, arm=arm, round=index+1)
                    evidence.append_record(record)
                    if record["exit_code"] or record["timed_out"]:
                        raise ExperimentFailure(f"test failed: {record['log_path']}")
                    records.append(record)
                    print(f"{task}/{arm} round {index+1}: {record['elapsed_seconds']:.3f}s", flush=True)
        if source_fingerprint(ROOT) != source or any(digest(path) != hashes[task][arm]
                for task, arms in workloads.items() for arm, path in arms.items()):
            raise ExperimentFailure("source or executable changed during measurements")
        summary = {}
        for task in workloads:
            samples = [row for row in records if row["task"] == task]
            median = {arm: statistics.median(row["elapsed_seconds"] for row in samples if row["arm"] == arm)
                      for arm in binaries}
            peak = {arm: max(row["peak_tree_rss_bytes"] for row in samples if row["arm"] == arm)
                    for arm in binaries}
            accepted = (median["optimized"] <= median["unoptimized"] * .9
                        and median["unoptimized"] - median["optimized"] >= 5
                        and peak["optimized"] <= peak["unoptimized"] * 1.1)
            summary[task] = {"median_seconds": median, "peak_tree_rss_bytes": peak,
                           "passes_initialization_threshold": accepted,
                           "adoption": "requires full workspace validation"}
        evidence.write_json("summary.json", summary)
        print(json.dumps(summary))
    return 0


if __name__ == "__main__":
    sys.exit(main())
