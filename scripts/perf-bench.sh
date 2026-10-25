#!/usr/bin/env bash
# Animatix performance benchmark harness (Layer 1, see docs/performance_evaluation.md).
#
# Provides a regression guard over the Criterion micro-suite by diffing the
# current run's `estimates.json` means against a saved baseline. Uses only
# Criterion's own JSON output (no cargo-criterion dependency).
#
# Two tiers (2026-10-26 restructure; the flat 132-bench run took 40-50 min
# per side, which made the guard too expensive to actually run):
#
#   quick  — the default. Every CPU bench target (frame walk, sampling, env,
#            VM, build, text, analyzer intelligence), measured with
#            --warm-up-time 1 --measurement-time 2. A warm tree runs in
#            ~4-6 min. This is the regression guard: it resolved −20%…−50%
#            moves cleanly during the frame-path cache work and is sensitive
#            to everything the CPU path can regress.
#   full   — quick plus `demos_frame_cost` (GPU offscreen + readback over
#            every example demo). Demos measure GPU-pipeline health at the
#            ms level with ±10-20% run-to-run variance; they cannot resolve
#            sub-µs CPU deltas (measured 2026-10-26: a −42% CPU change read
#            as +8.6% median there), so they guard a different failure class
#            and only need running around releases or renderer overhauls.
#
# Usage:
#   scripts/perf-bench.sh save [quick|full] [--thresh PCT]   # record the tier's baseline
#   scripts/perf-bench.sh compare [quick|full] [--thresh PCT] # rerun; regressions fail
#   scripts/perf-bench.sh run [quick|full]                    # just run, no guard
#
#   PERF_BASELINE_DIR=/path scripts/perf-bench.sh save full   # baselines live in
#   PERF_BASELINE_DIR=/path scripts/perf-bench.sh compare     #   $DIR/<tier>/
#     Use an explicit directory to persist baselines between CI runs.
#
# Thresholds:
#   --thresh PCT  percentage (mean) a bench may regress before `compare` fails, e.g. 5 => +5%.
#   Absolute guardrails are handled separately by scripts/extension-bench.sh.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# The regression verdict is computed by a Python script (see compare_estimates).
# If python3 is missing the function produces no output, its caller reads an
# empty line, and the run reports success with zero benches compared — a green
# result that means nothing. Refuse instead.
if ! command -v python3 >/dev/null 2>&1; then
    echo "perf-bench: python3 not found — cannot compute regression verdicts." >&2
    echo "           Run this inside a shell that provides python3 (the repo's dev" >&2
    echo "           shell does not); refusing to report a green run without it." >&2
    exit 1
fi

# Criterion budget: 1 s warm-up + 2 s measurement per bench proved stable
# enough to resolve the frame-path changes of 2026-10-25 (−20%…−50% moves);
# the old defaults (3 s + 5 s) tripled the wall time for no extra verdict
# quality on benches this small.
BENCH_ARGS=(--warm-up-time 1 --measurement-time 2)

# The quick tier: every engine CPU bench target plus the analyzer suite.
QUICK_BENCHES=(
    build_time
    code_highlight
    cost_breakdown
    equation_frame
    full_pipeline
    modifier_runtime
    property_interpolation
    scaling
    scaling_breakdown
    scaling_no_cache
    scene_costs
    scrubbing
    stage_breakdown
    static_scene
    static_scene_item_collection
    text_rebuild
    timeline_eval
    visibility_culling
    vm_regression
    vm_vs_ir
)
# The full tier adds the GPU demo suite.
FULL_BENCHES=("${QUICK_BENCHES[@]}" demos_frame_cost)

BASELINE_DIR="${PERF_BASELINE_DIR:-target/perf/baseline}"
LOG_DIR="${PERF_LOG_DIR:-target/perf/latest}"
RUN_MARKER="target/perf/.run-start"

MODE="${1:-compare}"
shift || true

THRESH=5
K=3   # combined std-dev multiples before a real regression is declared
TIER=quick
while [[ $# -gt 0 ]]; do
    case "$1" in
        --thresh)
            THRESH="$2"
            shift 2
            ;;
        quick|full)
            TIER="$1"
            shift
            ;;
        *)
            echo "perf-bench: unknown argument '$1' (expected quick|full or --thresh PCT)" >&2
            exit 2
            ;;
    esac
done

mkdir -p "$BASELINE_DIR/$TIER" "$LOG_DIR"

# One cargo invocation per package: `--bench` target selection is per package
# (the analyzer suite lives only there, the engine suites only here), and one
# invocation compiles/benches the whole tier without re-linking per target.
run_suite() {
    mkdir -p "$LOG_DIR"
    touch "$RUN_MARKER"
    local bench_args=()
    if [[ "$TIER" == "full" ]]; then
        bench_args=("${FULL_BENCHES[@]}")
    else
        bench_args=("${QUICK_BENCHES[@]}")
    fi
    local engine_args=()
    for bench in "${bench_args[@]}"; do
        engine_args+=(--bench "$bench")
    done
    cargo bench -p animatix "${engine_args[@]}" -- "${BENCH_ARGS[@]}"
    cargo bench -p animatix-analyzer --bench editor_intelligence -- "${BENCH_ARGS[@]}"
}

# Collect fresh `new/estimates.json` -> a flat directory of <id>.json
collect_new() {
    local dest="$1"
    rm -rf "$dest"
    mkdir -p "$dest"
    # Criterion writes target/criterion/<group>/<function>/new/estimates.json,
    # or <function>/new/estimates.json when a bench declares no group. Use the
    # whole path below target/criterion as the id so grouped benches keep their
    # group name: taking only the parent directory collapsed `analyzer_update/
    # small` to `small`, which then failed the filter and was silently dropped.
    # The `-newer $RUN_MARKER` freshness scope is what makes per-tier
    # collection correct: a previous run of the *other* tier (or of single
    # benches by hand) leaves older estimates in target/criterion, and only
    # this run's outputs are picked up.
    find target/criterion -name estimates.json -path "*/new/*" -newer "$RUN_MARKER" 2>/dev/null | while read -r f; do
        local id
        id="${f#target/criterion/}"
        id="${id%/new/estimates.json}"
        id="${id//\//__}"
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
# Sub-`TINY_NS` benches get a second gate: a tight leaf bench has a tiny
# within-run std_dev, so the percentage floor alone flagged a 0.9ns -> 1.1ns
# shift (a 0.2ns absolute move, indistinguishable from code-layout/inlining
# drift) as a 23% regression on every run. For those, an absolute increase of
# more than TINY_NS_DELTA is also required, so a genuine slowdown (1ns -> 3ns)
# still fails while layout noise does not.
#
# Prints: "mean_baseline mean_current chg_pct limit_pct verdict" (space-separated).
compare_estimates() {
    python3 -c '
import json,sys
K=float(sys.argv[1])
FLOOR=float(sys.argv[2])
TINY_NS=float(sys.argv[5]) if len(sys.argv) > 5 else 10.0
TINY_NS_DELTA=float(sys.argv[6]) if len(sys.argv) > 6 else 1.0
b=json.load(open(sys.argv[3]))
c=json.load(open(sys.argv[4]))
bm=b["mean"]["point_estimate"]; bs=b["std_dev"]["point_estimate"]
cm=c["mean"]["point_estimate"]; cs=c["std_dev"]["point_estimate"]
chg=(cm-bm)/bm*100
noise=K*(bs+cs)/bm*100      # combined-std bound, expressed in % of baseline
limit=max(noise, FLOOR)
verdict="ok"
if cm > bm and chg > limit:
    tiny = bm < TINY_NS
    if tiny and (cm - bm) <= TINY_NS_DELTA:
        # Layout/inlining drift on a sub-nanosecond bench, not a real change.
        verdict="ok"
    else:
        verdict="REGRESSION"
print(f"{bm} {cm} {chg:.2f} {limit:.2f} {verdict}")
' "$K" "$THRESH" "$1" "$2" "${PERF_TINY_NS:-10}" "${PERF_TINY_NS_DELTA:-1}"
}

case "$MODE" in
    save)
        echo "perf-bench: running the '$TIER' tier and recording its baseline..."
        run_suite
        collect_new "$BASELINE_DIR/$TIER"
        echo "perf-bench: '$TIER' baseline saved to $BASELINE_DIR/$TIER ($(find "$BASELINE_DIR/$TIER" -name '*.json' | wc -l) benches)"
        ;;

    run)
        echo "perf-bench: running the '$TIER' tier without guard..."
        run_suite
        ;;

    compare)
        if [[ -z "$(find "$BASELINE_DIR/$TIER" -name "*.json" -print -quit)" ]]; then
            echo "perf-bench: no '$TIER' baseline found under $BASELINE_DIR/$TIER." >&2
            echo "  Run 'scripts/perf-bench.sh save $TIER' first (ideally on a known-good commit)." >&2
            exit 3
        fi
        echo "perf-bench: running the '$TIER' tier and comparing against its baseline (regression beyond combined-noise bound or +${THRESH}% floor fails)..."
        run_suite
        collect_new "$LOG_DIR"

        failures=0
        total=0
        # print header
        printf "%-46s %13s %13s %9s %8s  %s\n" "bench" "baseline(ns)" "current(ns)" "%chg" "limit%" "verdict"
        for cur in "$LOG_DIR"/*.json; do
            id="$(basename "$cur" .json)"
            base="$BASELINE_DIR/$TIER/${id}.json"
            [[ -f "$base" ]] || continue   # new bench not in baseline: skip
            read -r base_ns cur_ns chg limit_pct verdict < <(compare_estimates "$base" "$cur")
            total=$((total+1))
            if [[ "$verdict" == "REGRESSION" ]]; then
                failures=$((failures+1))
            fi
            printf "%-46s %13.1f %13.1f %9s %8s  %s\n" "$id" "$base_ns" "$cur_ns" "$chg%" "$limit_pct%" "$verdict"
        done

        echo ""
        echo "perf-bench: $total '$TIER' benches compared, $failures real regressions detected."
        if [[ "$failures" -gt 0 ]]; then
            echo "perf-bench: FAIL — benchmark regressions detected." >&2
            exit 1
        fi
        echo "perf-bench: PASS."
        ;;

    *)
        echo "usage: $0 {save|compare|run} [quick|full] [--thresh PCT]" >&2
        exit 2
        ;;
esac
