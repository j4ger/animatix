#!/usr/bin/env bash
# Fresh absolute health samples for the extension abstraction (no historical comparison).
#
# Usage:
#   tools/quality/extension-bench.sh [--quick] [--max-plan-ns 10000]
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

command -v python3 >/dev/null || { echo "extension-bench requires python3" >&2; exit 2; }
# Preserve Cargo's compilation cache; isolate only Criterion's result history.
# A new directory prevents a partially cached base/sample.json from entering
# Criterion's automatic comparison. Never read the shared target/criterion tree.
mkdir -p "$ROOT/target/ci"
RUN_DIR="$(mktemp -d "$ROOT/target/ci/ext-bench.XXXXXXXX")"
export CRITERION_HOME="$RUN_DIR/criterion"
finish_evidence() {
    local status=$?
    trap - EXIT
    if ! python3 tools/quality/extension-bench-results.py "$RUN_DIR" --stage-evidence "$status"; then
        echo "extension-bench: could not stage bounded evidence" >&2
        [[ "$status" -ne 0 ]] || status=2
    fi
    exit "$status"
}
trap finish_evidence EXIT
if [[ -n "${GITHUB_ENV:-}" ]]; then
    printf 'ANIMATIX_EXT_BENCH_EVIDENCE=%s\n' "$RUN_DIR/evidence" >> "$GITHUB_ENV"
fi
# cargo-criterion redirects reporting over a socket and suppresses these JSON
# files. This gate explicitly owns its on-disk results instead.
unset CARGO_CRITERION_PORT
ARGS+=(--noplot --save-baseline health)
echo "extension-bench: fresh health run; no historical comparison; evidence: $RUN_DIR"

cargo bench -p animatix --bench vm --bench frame --no-run 2>&1 | tee "$RUN_DIR/build.log"
cargo bench -p animatix --bench vm -- '^property_plan_lookup_and_sample$' "${ARGS[@]}" \
    2>&1 | tee "$RUN_DIR/property-plan.log"
cargo bench -p animatix --bench frame -- '^timeline_evaluate_(0s|1s|2s)$' "${ARGS[@]}" \
    2>&1 | tee "$RUN_DIR/timeline.log"

CHECK_ARGS=()
if [[ -n "$MAX_PLAN_NS" ]]; then
    CHECK_ARGS+=(--max-plan-ns "$MAX_PLAN_NS")
fi
# Validate all four samples even when no threshold was requested. Criterion
# can log JSON write errors without returning a failing process exit code.
python3 tools/quality/extension-bench-results.py "$CRITERION_HOME" "${CHECK_ARGS[@]}" \
    | tee "$RUN_DIR/summary.json"
