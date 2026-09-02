#!/usr/bin/env bash
# Animatix performance benchmark harness (Layer 1, see docs/performance_evaluation.md).
#
# Provides a regression guard over the full Criterion micro-suite by diffing the
# current run's `estimates.json` means against a saved baseline. Uses only
# Criterion's own JSON output (no cargo-criterion dependency).
#
# Usage:
#   scripts/perf-bench.sh save [FILTER]            # run the suite (optionally filtered) and record the baseline
#   scripts/perf-bench.sh compare [FILTER] [--thresh PCT]  # rerun and fail if any bench regressed >PCT% (default 5)
#   scripts/perf-bench.sh run [FILTER]             # just run the suite (no guard); FILTER is a Criterion filter
#
#   PERF_BASELINE_DIR=/path/to/artifact scripts/perf-bench.sh save
#   PERF_BASELINE_DIR=/path/to/artifact scripts/perf-bench.sh compare
#     Use an explicit directory to persist baselines between CI runs.
#
# Thresholds:
#   --thresh PCT  percentage (mean) a bench may regress before `compare` fails, e.g. 5 => +5%.
#   Absolute guardrails are handled separately by scripts/extension-bench.sh.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

PKG="animatix"
BASELINE_DIR="${PERF_BASELINE_DIR:-target/perf/baseline}"
LOG_DIR="${PERF_LOG_DIR:-target/perf/latest}"
RUN_MARKER="target/perf/.run-start"

MODE="${1:-compare}"
shift || true

THRESH=5
K=3   # combined std-dev multiples before a real regression is declared
FILTER=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --thresh)
            THRESH="$2"
            shift 2
            ;;
        *)
            FILTER="$1"
            shift
            ;;
    esac
done

mkdir -p "$BASELINE_DIR" "$LOG_DIR"

# We run ALL benches in one `cargo bench` invocation (more efficient).
run_suite() {
    local filter="${1:-}"
    mkdir -p "$LOG_DIR"
    touch "$RUN_MARKER"
    if [[ -n "$filter" ]]; then
        cargo bench -p "$PKG" -- "$filter"
    else
        cargo bench -p "$PKG"
    fi
}

# Collect fresh `new/estimates.json` -> a flat directory of <bench>__<func>.json
collect_new() {
    local dest="$1"
    local filter="${2:-}"
    rm -rf "$dest"
    mkdir -p "$dest"
    # Criterion writes target/criterion/<bench-id>/new/estimates.json; bench-id
    # is the benchmark function name (e.g. timeline_evaluate_0s).
    find target/criterion -name estimates.json -path "*/new/*" -newer "$RUN_MARKER" 2>/dev/null | while read -r f; do
        local id
        id="$(basename "$(dirname "$(dirname "$f")")")"
        if [[ -n "$filter" && "$id" != *"$filter"* ]]; then
            continue
        fi
        cp "$f" "$dest/${id}.json"
    done
    # Guard: did anything get collected?
    if [[ -z "$(find "$dest" -name "*.json" -print -quit)" ]]; then
        echo "perf-bench: no new estimates collected under target/criterion — did the suite run?" >&2
        return 1
    fi
}

# Python3 helper: compare two Criterion estimates files (baseline vs current).
# Regression rule (see docs/performance_evaluation.md §4) is statistically
# principled and noise-adaptive: fail only when the current mean exceeds the
# baseline mean by more than K standard deviations of the *combined* run
# variance (a conservative t-style bound), with an absolute floor of THRESH%.
# This keeps noisy build/planner benches (wide std_dev) from false-flagging,
# while tight leaf benches (tiny std_dev) stay sensitive. Independent of any
# Criterion subcommand; reads each run's own `mean`/`std_dev` point estimates.
#
# Prints: "mean_baseline mean_current chg_pct limit_pct verdict" (space-separated).
compare_estimates() {
    python3 -c '
import json,sys
K=float(sys.argv[1])
FLOOR=float(sys.argv[2])
b=json.load(open(sys.argv[3]))
c=json.load(open(sys.argv[4]))
bm=b["mean"]["point_estimate"]; bs=b["std_dev"]["point_estimate"]
cm=c["mean"]["point_estimate"]; cs=c["std_dev"]["point_estimate"]
chg=(cm-bm)/bm*100
noise=K*(bs+cs)/bm*100      # combined-std bound, expressed in % of baseline
limit=max(noise, FLOOR)
verdict="ok"
if cm > bm and chg > limit:
    verdict="REGRESSION"
print(f"{bm} {cm} {chg:.2f} {limit:.2f} {verdict}")
' "$K" "$THRESH" "$1" "$2"
}

case "$MODE" in
    save)
        echo "perf-bench: running full suite and recording baseline..."
        run_suite "$FILTER"
        collect_new "$BASELINE_DIR" "$FILTER"
        echo "perf-bench: baseline saved to $BASELINE_DIR ($(find "$BASELINE_DIR" -name '*.json' | wc -l) benches)"
        ;;

    run)
        echo "perf-bench: running suite without guard..."
        run_suite "$FILTER"
        ;;

    compare)
        if [[ -z "$(find "$BASELINE_DIR" -name "*.json" -print -quit)" ]]; then
            echo "perf-bench: no baseline found under $BASELINE_DIR." >&2
            echo "  Run 'scripts/perf-bench.sh save' first (ideally on a known-good commit)." >&2
            exit 3
        fi
        echo "perf-bench: running suite and comparing against baseline (regression beyond combined-noise bound or +${THRESH}% floor fails)..."
        run_suite "$FILTER"
        collect_new "$LOG_DIR" "$FILTER"

        failures=0
        total=0
        # print header
        printf "%-46s %13s %13s %9s %8s  %s\n" "bench" "baseline(ns)" "current(ns)" "%chg" "limit%" "verdict"
        for cur in "$LOG_DIR"/*.json; do
            id="$(basename "$cur" .json)"
            base="$BASELINE_DIR/${id}.json"
            [[ -f "$base" ]] || continue   # new bench not in baseline: skip
            read -r base_ns cur_ns chg limit_pct verdict < <(compare_estimates "$base" "$cur")
            total=$((total+1))
            if [[ "$verdict" == "REGRESSION" ]]; then
                failures=$((failures+1))
            fi
            printf "%-46s %13.1f %13.1f %9s %8s  %s\n" "$id" "$base_ns" "$cur_ns" "$chg%" "$limit_pct%" "$verdict"
        done

        echo ""
        echo "perf-bench: $total benches compared, $failures real regressions detected."
        if [[ "$failures" -gt 0 ]]; then
            echo "perf-bench: FAIL — benchmark regressions detected." >&2
            exit 1
        fi
        echo "perf-bench: PASS."
        ;;

    *)
        echo "usage: $0 {save|compare|run} [FILTER] [--thresh PCT]" >&2
        exit 2
        ;;
esac
