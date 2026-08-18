#!/usr/bin/env bash
# Baseline benchmarks for the extension abstraction.
#
# Usage:
#   scripts/extension-bench.sh [--quick] [--max-plan-ns 10]
#
# --quick uses Criterion's quick mode, which is enough for local regression
# checks but should not be treated as a release benchmark.
# --max-plan-ns fails if property_plan_lookup_and_sample exceeds the threshold.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ARGS=()
MAX_PLAN_NS=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --quick)
            ARGS+=(-- --quick)
            shift
            ;;
        --max-plan-ns)
            MAX_PLAN_NS="$2"
            shift 2
            ;;
        *)
            echo "unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

OUT="$(cargo bench -p animatix --bench property_interpolation "${ARGS[@]}" 2>&1)"
echo "$OUT"
cargo bench -p animatix --bench timeline_eval "${ARGS[@]}"

if [[ -n "$MAX_PLAN_NS" ]]; then
    PLAN_NS="$(printf '%s\n' "$OUT" \
        | grep -A1 'property_plan_lookup_and_sample' \
        | grep -o '[0-9.]* ns' \
        | head -1 \
        | awk '{print $1}')"
    if [[ -z "$PLAN_NS" ]]; then
        echo "could not parse property_plan_lookup_and_sample" >&2
        exit 2
    fi
    if awk "BEGIN { exit !($PLAN_NS > $MAX_PLAN_NS) }"; then
        echo "property_plan_lookup_and_sample ${PLAN_NS}ns exceeds ${MAX_PLAN_NS}ns" >&2
        exit 1
    fi
    echo "property_plan_lookup_and_sample ${PLAN_NS}ns <= ${MAX_PLAN_NS}ns"
fi
