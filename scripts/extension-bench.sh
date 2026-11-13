#!/usr/bin/env bash
# Baseline benchmarks for the extension abstraction.
#
# Usage:
#   scripts/extension-bench.sh [--quick] [--max-plan-ns 10000]
#
# --quick uses Criterion's quick mode, which is enough for local regression
# checks but should not be treated as a release benchmark.
# --max-plan-ns is an emergency health sentinel for the specifically named
# property-plan lookup leaf. It is not a general performance-drift threshold.
#
# The two benches live in the merged `vm` and `frame` targets, so this runs
# them by Criterion filter rather than by target name.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ARGS=()
MAX_PLAN_NS=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --quick)
            ARGS+=(--quick)
            shift
            ;;
        --max-plan-ns)
            if [[ $# -lt 2 || ! "$2" =~ ^[0-9]+([.][0-9]+)?$ ]]; then
                echo "--max-plan-ns requires a finite nonnegative number" >&2
                exit 2
            fi
            MAX_PLAN_NS="$2"
            shift 2
            ;;
        *)
            echo "unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

cargo bench -p animatix --bench vm --bench frame --no-run
OUT="$(cargo bench -p animatix --bench vm -- property_plan_lookup_and_sample "${ARGS[@]}" 2>&1)"
echo "$OUT"
cargo bench -p animatix --bench frame -- timeline_evaluate "${ARGS[@]}"

if [[ -n "$MAX_PLAN_NS" ]]; then
    PLAN_NS="$(printf '%s\n' "$OUT" \
        | grep -A1 'property_plan_lookup_and_sample' \
        | grep -o '[0-9.]* ns' \
        | head -1 \
        | awk '{print $1}')"
    if [[ -z "$PLAN_NS" ]]; then
        echo "no fresh property_plan_lookup_and_sample result in this command output" >&2
        echo "refusing to apply the sentinel to a stale Criterion estimate" >&2
        exit 2
    fi
    if awk "BEGIN { exit !($PLAN_NS > $MAX_PLAN_NS) }"; then
        echo "property_plan_lookup_and_sample ${PLAN_NS}ns exceeds ${MAX_PLAN_NS}ns" >&2
        exit 1
    fi
    echo "property_plan_lookup_and_sample ${PLAN_NS}ns <= ${MAX_PLAN_NS}ns"
fi
