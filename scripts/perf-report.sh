#!/usr/bin/env bash
# Render Criterion results as a small, machine-readable performance ledger.
#
# Usage:
#   scripts/perf-report.sh [INPUT_DIR] [--output PATH] [--layer3 PATH]
#
# INPUT_DIR defaults to target/perf/latest. Every JSON file in the directory
# must be a Criterion estimates.json copy produced by perf-bench.sh. --layer3
# accepts a JSON array of additional measurements, allowing GPU/export tools to
# contribute records without coupling them to Criterion's schema.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

INPUT_DIR="target/perf/latest"
OUTPUT="target/perf/latest.json"
LAYER3=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --output)
            OUTPUT="$2"
            shift 2
            ;;
        --layer3)
            LAYER3="$2"
            shift 2
            ;;
        --help|-h)
            sed -n '2,12p' "$0"
            exit 0
            ;;
        *)
            if [[ "$INPUT_DIR" != "target/perf/latest" ]]; then
                echo "perf-report: unexpected argument '$1'" >&2
                exit 2
            fi
            INPUT_DIR="$1"
            shift
            ;;
    esac
done

python3 - "$INPUT_DIR" "$OUTPUT" "$LAYER3" <<'PY'
import json
import pathlib
import sys
from datetime import datetime, timezone

input_dir = pathlib.Path(sys.argv[1])
output = pathlib.Path(sys.argv[2])
layer3_path = pathlib.Path(sys.argv[3]) if sys.argv[3] else None

if not input_dir.is_dir():
    raise SystemExit(f"perf-report: input directory does not exist: {input_dir}")

records = []
def surface_for(name):
    if any(token in name for token in ("build", "rebuild", "pipeline", "text_rebuild")):
        return "rebuild"
    if any(token in name for token in ("scrub", "playback")):
        return "scrub"
    if any(token in name for token in ("property", "interpolate")):
        return "interp"
    if "modifier" in name or name.startswith("vm_"):
        return "modifier"
    if any(token in name for token in ("raster", "offscreen", "export")):
        return "export"
    return "frame"

for path in sorted(input_dir.glob("*.json")):
    data = json.loads(path.read_text())
    try:
        mean = float(data["mean"]["point_estimate"])
        std_dev = float(data["std_dev"]["point_estimate"])
    except (KeyError, TypeError, ValueError) as exc:
        raise SystemExit(f"perf-report: invalid Criterion estimates: {path}: {exc}")
    name = path.stem
    records.append({
        "surface": surface_for(name),
        "bench": name,
        "unit": "ns",
        "mean": mean,
        "std_dev": std_dev,
        "source": "criterion",
    })

if not records:
    raise SystemExit(f"perf-report: no Criterion JSON files found in {input_dir}")

if layer3_path:
    extra = json.loads(layer3_path.read_text())
    if not isinstance(extra, list):
        raise SystemExit("perf-report: --layer3 input must be a JSON array")
    for record in extra:
        if not isinstance(record, dict) or not {"surface", "bench", "mean", "unit"} <= record.keys():
            raise SystemExit("perf-report: every layer-3 record needs surface, bench, mean, and unit")
        record = dict(record)
        record.setdefault("source", "layer3")
        records.append(record)

output.parent.mkdir(parents=True, exist_ok=True)
ledger = {
    "schema": 1,
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "input_dir": str(input_dir),
    "measurements": records,
}
output.write_text(json.dumps(ledger, indent=2, sort_keys=True) + "\n")

print(f"{'surface':<12} {'bench':<46} {'mean':>14} {'unit':<6} {'source'}")
for record in records:
    print(
        f"{record['surface']:<12} {record['bench']:<46} "
        f"{float(record['mean']):>14.3f} {record['unit']:<6} {record['source']}"
    )
print(f"\nperf-report: wrote {len(records)} measurements to {output}")
PY
