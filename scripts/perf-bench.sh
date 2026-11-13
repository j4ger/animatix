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
#   quick  — the default. Every CPU bench target (`build`, `frame`, `stage`,
#            `vm`, `text`) plus the analyzer suite, measured with
#            --warm-up-time 1 --measurement-time 2. A warm tree runs in
#            ~4-6 min. This is the regression guard: it resolved −20%…−50%
#            moves cleanly during the frame-path cache work and is sensitive
#            to everything the CPU path can regress.
#   full   — quick plus the `gpu` target (offscreen + readback over every
#            example demo). Demos measure GPU-pipeline health at the
#            ms level with ±10-20% run-to-run variance; they cannot resolve
#            sub-µs CPU deltas (measured 2026-10-26: a −42% CPU change read
#            as +8.6% median there), so they guard a different failure class
#            and only need running around releases or renderer overhauls.
#
# Usage:
#   scripts/perf-bench.sh save [quick|full] [--thresh PCT]   # record the tier's baseline
#   scripts/perf-bench.sh compare [quick|full] [--thresh PCT] # rerun; material regressions fail
#   scripts/perf-bench.sh evaluate [quick|full]               # classify saved JSON only
#   scripts/perf-bench.sh run [quick|full]                    # just run, no guard
#
#   PERF_BASELINE_DIR=/path scripts/perf-bench.sh save full   # baselines live in
#   PERF_BASELINE_DIR=/path scripts/perf-bench.sh compare     #   $DIR/<tier>/
#     Use an explicit directory to persist baselines between CI runs. A save also
#     writes <tier>/META.json (commit, rustc, CPU count, time, load, taskset);
#     `compare` prints it for context but never uses it in the verdict.
#
# Load preflight:
#   Before any suite runs, the script reads /proc/loadavg and refuses to measure
#   when the 1-minute load exceeds PERF_MAX_LOAD_RATIO (default 0.5) x CPUs.
#   A loaded host moves every bench, so the numbers are noise, not a verdict; the
#   run exits 2 (environment, not regression). Use PERF_ALLOW_LOAD=1 to measure
#   anyway; the override is printed and recorded in META.json.
#
#   PERF_TASKSET_CPUS=0-3  optional `taskset -c` CPU set wrapping cargo bench.
#   PERF_MAX_LOAD_RATIO=0.5  load-per-CPU preflight ceiling.
#   PERF_ALLOW_LOAD=1        override the preflight (explicit, logged).
#   PERF_MIN_BASELINE_COVERAGE=90
#                            percent of saved baseline benches that must match
#                            a current benchmark. A compare that matches zero
#                            rows, or only a minority of a stale/renamed
#                            baseline, is a broken verdict, not a pass.
#
# Thresholds:
#   --thresh PCT  minimum regression ratio beyond the Criterion mean-CI screen,
#                 e.g. 5 => +5%; impact policy decides whether it blocks.
#
# Exit codes: 0 pass, 1 material regression/policy/coverage failure,
#             2 usage/preflight/environment or inconclusive interval, 3 no baseline.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

# The regression verdict is computed by a Python script (see compare_estimates).
# If python3 is missing the function produces no output, its caller reads an
# empty line, and the run reports success with zero benches compared — a green
# result that means nothing. Refuse instead.
if ! command -v python3 >/dev/null 2>&1; then
    echo "perf-bench: python3 not found — cannot compute regression verdicts." >&2
    echo "           Run this inside a shell that provides python3, e.g. 'nix develop'." >&2
    echo "           Refusing to report a green run without it." >&2
    exit 1
fi

# Criterion budget: 1 s warm-up + 2 s measurement per bench proved stable
# enough to resolve the frame-path changes of 2026-10-25 (−20%…−50% moves);
# the old defaults (3 s + 5 s) tripled the wall time for no extra verdict
# quality on benches this small.
BENCH_ARGS=(--warm-up-time 1 --measurement-time 2)

# The quick tier: every engine CPU bench target plus the analyzer suite.
QUICK_BENCHES=(
    build
    frame
    stage
    vm
    text
)
# The full tier adds the GPU demo suite.
FULL_BENCHES=("${QUICK_BENCHES[@]}" gpu)

BASELINE_DIR="${PERF_BASELINE_DIR:-target/perf/baseline}"
LOG_DIR="${PERF_LOG_DIR:-target/perf/latest}"
RUN_MARKER="target/perf/.run-start"

MODE="${1:-compare}"
shift || true

THRESH=5
TIER=quick
MIN_BASELINE_COVERAGE="${PERF_MIN_BASELINE_COVERAGE:-90}"
if ! [[ "$MIN_BASELINE_COVERAGE" =~ ^(100|[1-9][0-9]?)$ ]]; then
    echo "perf-bench: PERF_MIN_BASELINE_COVERAGE='$MIN_BASELINE_COVERAGE' must be an integer 1-100." >&2
    exit 2
fi
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

if [[ "$MODE" != "save" && "$MODE" != "run" && "$MODE" != "compare" && "$MODE" != "evaluate" ]]; then
    echo "perf-bench: unknown mode '$MODE' (expected save|run|compare|evaluate)" >&2
    exit 2
fi

mkdir -p "$BASELINE_DIR/$TIER" "$LOG_DIR"

# ── environment guards ────────────────────────────────────────────────────────
# The regression verdict assumes the machine is otherwise idle. That is the one
# assumption the harness cannot check from within Criterion: on a loaded host
# every bench shifts together, and the noise-adaptive bound then reports a pile
# of false regressions. /proc/loadavg is the cheap, non-self-matching test (a
# `pgrep` for the bench process would match the shell running this script).

detect_cpu_count() {
    if command -v nproc >/dev/null 2>&1; then
        nproc
    elif command -v getconf >/dev/null 2>&1; then
        getconf _NPROCESSORS_ONLN
    else
        echo 1
    fi
}

PREFLIGHT_LOAD1=""   # empty means "not observed" (non-Linux, or no /proc)
PREFLIGHT_CORES=1
PREFLIGHT_RATIO="0.5"

preflight() {
    PREFLIGHT_CORES="$(detect_cpu_count)"
    if ! [[ "$PREFLIGHT_CORES" =~ ^[0-9]+$ ]] || [[ "$PREFLIGHT_CORES" -lt 1 ]]; then
        PREFLIGHT_CORES=1
    fi
    PREFLIGHT_RATIO="${PERF_MAX_LOAD_RATIO:-0.5}"
    if ! awk -v r="$PREFLIGHT_RATIO" 'BEGIN { exit !(r ~ /^[0-9]+([.][0-9]+)?$/ && r + 0 >= 0) }'; then
        echo "perf-bench: PERF_MAX_LOAD_RATIO='$PREFLIGHT_RATIO' is not a non-negative number." >&2
        exit 2
    fi

    if [[ ! -r /proc/loadavg ]]; then
        echo "perf-bench: /proc/loadavg unavailable — skipping the load preflight (not Linux?)." >&2
        return 0
    fi

    local load1 limit
    read -r load1 _ < /proc/loadavg
    PREFLIGHT_LOAD1="$load1"
    limit="$(awk -v r="$PREFLIGHT_RATIO" -v c="$PREFLIGHT_CORES" 'BEGIN { printf "%.2f", r * c }')"

    if awk -v l="$load1" -v r="$PREFLIGHT_RATIO" -v c="$PREFLIGHT_CORES" 'BEGIN { exit !(l > r * c) }'; then
        if [[ "${PERF_ALLOW_LOAD:-0}" == "1" ]]; then
            echo "perf-bench: WARNING — 1-min load $load1 exceeds ${PREFLIGHT_RATIO} x ${PREFLIGHT_CORES} CPUs (limit $limit); measuring anyway because PERF_ALLOW_LOAD=1." >&2
        else
            cat >&2 <<EOF
perf-bench: refusing to measure: the machine is loaded, so the numbers would be noise.
  /proc/loadavg 1-min load : $load1
  CPUs                     : $PREFLIGHT_CORES
  PERF_MAX_LOAD_RATIO      : $PREFLIGHT_RATIO (limit $limit)

  Wait for an idle window (finish builds, tests, browser/editor indexing), or
  rerun with PERF_ALLOW_LOAD=1 if you knowingly accept a noisy measurement.
  A load-driven compare failure is an environment result, not a code regression.
EOF
            exit 2
        fi
    else
        echo "perf-bench: load preflight ok (1-min load $load1 / $PREFLIGHT_CORES CPUs, limit $limit)."
    fi
}

# Optional CPU pinning. Never hard-code a CPU list here: the right set differs
# per machine, and pinning does not fix a genuinely busy host (see the
# 2026-09-03 note in docs/performance_evaluation.md §4).
TASKSET_PREFIX=()
if [[ -n "${PERF_TASKSET_CPUS:-}" ]]; then
    if ! command -v taskset >/dev/null 2>&1; then
        echo "perf-bench: PERF_TASKSET_CPUS='$PERF_TASKSET_CPUS' is set but 'taskset' was not found." >&2
        echo "           Install util-linux or unset PERF_TASKSET_CPUS." >&2
        exit 2
    fi
    TASKSET_PREFIX=(taskset -c "$PERF_TASKSET_CPUS")
fi

run_cargo() {
    if [[ ${#TASKSET_PREFIX[@]} -gt 0 ]]; then
        "${TASKSET_PREFIX[@]}" "$@"
    else
        "$@"
    fi
}

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
    local cargo_args=()
    for bench in "${bench_args[@]}"; do
        cargo_args+=(--bench "$bench")
    done
    run_cargo cargo bench -p animatix "${cargo_args[@]}" -- "${BENCH_ARGS[@]}"
    run_cargo cargo bench -p animatix-analyzer --bench editor_intelligence -- "${BENCH_ARGS[@]}"
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

# Baseline metadata is context, not a verdict. A save records which build and
# machine produced the estimates; compare prints it so a stale, dirty or
# differently-pinned baseline is visible. META.json never enters the comparison
# (only the estimates pairs do), and a missing/malformed one cannot fail a run.
write_baseline_meta() {
    local bench_count="$1"
    local path="$BASELINE_DIR/$TIER/META.json"
    local commit dirty rustc cpus created allow taskset
    commit="$(git rev-parse HEAD 2>/dev/null || echo unknown)"
    if [[ -n "$(git status --porcelain 2>/dev/null)" ]]; then dirty=true; else dirty=false; fi
    rustc="$(rustc --version 2>/dev/null || echo unavailable)"
    cpus="$PREFLIGHT_CORES"
    created="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    allow=false
    [[ "${PERF_ALLOW_LOAD:-0}" == "1" ]] && allow=true
    taskset="${PERF_TASKSET_CPUS:-}"
    python3 - "$path" "$TIER" "$commit" "$dirty" "$rustc" "$cpus" "$created" \
        "$PREFLIGHT_LOAD1" "$PREFLIGHT_RATIO" "$allow" "$taskset" "$bench_count" <<'PY'
import json, sys
(path, tier, commit, dirty, rustc, cpus, created, load1, ratio,
 allow, taskset, bench_count) = sys.argv[1:13]
with open(path, "w", encoding="utf-8") as f:
    json.dump({
        "tier": tier,
        "git_commit": commit,
        "git_dirty": dirty == "true",
        "rustc": rustc,
        "cpu_count": int(cpus),
        "created_utc": created,
        "bench_count": int(bench_count),
        "load_1m_at_start": float(load1) if load1 else None,
        "max_load_ratio": float(ratio),
        "allow_load": allow == "true",
        "taskset_cpus": taskset or None,
    }, f, indent=2, sort_keys=True)
    f.write("\n")
PY
}

show_baseline_meta() {
    local meta="$BASELINE_DIR/$TIER/META.json"
    if [[ ! -f "$meta" ]]; then
        echo "perf-bench: baseline has no META.json (saved before metadata was recorded)."
        return 0
    fi
    python3 - "$meta" <<'PY' || echo "perf-bench: could not read baseline metadata from $meta" >&2
import json, sys
with open(sys.argv[1], encoding="utf-8") as f:
    m = json.load(f)
commit = str(m.get("git_commit", "unknown"))[:12]
dirty = " (dirty tree)" if m.get("git_dirty") else ""
load1 = m.get("load_1m_at_start")
load = "unknown" if load1 is None else str(load1)
taskset = ", taskset " + str(m["taskset_cpus"]) if m.get("taskset_cpus") else ""
allow = ", PERF_ALLOW_LOAD=1" if m.get("allow_load") else ""
print("perf-bench: baseline metadata: " + str(m.get("tier", "?")) + " saved "
      + str(m.get("created_utc", "unknown")) + " at " + commit + dirty
      + " with " + str(m.get("rustc", "rustc unknown")) + ", "
      + str(m.get("cpu_count", "?")) + " CPUs, load " + load + taskset + allow)
PY
}

POLICY_FILE="${PERF_POLICY_FILE:-scripts/perf-policy.json}"
POLICY_TOOL="scripts/perf-policy.py"

# collect_new removes its destination before copying fresh estimates. Refuse
# overlapping baseline/log paths before creating either path or starting cargo.
BASELINE_PATH="$(realpath -m -- "$BASELINE_DIR")"
LOG_PATH="$(realpath -m -- "$LOG_DIR")"
case "$BASELINE_PATH/" in
    "$LOG_PATH/"*) echo "perf-bench: baseline path must not be equal to or inside the log path." >&2; exit 2 ;;
esac
case "$LOG_PATH/" in
    "$BASELINE_PATH/"*) echo "perf-bench: log path must not be equal to or inside the baseline path." >&2; exit 2 ;;
esac

case "$MODE" in
    save)
        preflight
        echo "perf-bench: running the '$TIER' tier and recording its baseline..."
        run_suite
        collect_new "$BASELINE_DIR/$TIER"
        bench_count="$(find "$BASELINE_DIR/$TIER" -maxdepth 1 -name '*.json' ! -name 'META.json' | wc -l | tr -d ' ')"
        write_baseline_meta "$bench_count"
        echo "perf-bench: '$TIER' baseline saved to $BASELINE_DIR/$TIER ($bench_count benches; metadata: $BASELINE_DIR/$TIER/META.json)"
        ;;

    run)
        preflight
        echo "perf-bench: running the '$TIER' tier without guard..."
        run_suite
        ;;

    compare|evaluate)
        baseline_count="$(find "$BASELINE_DIR/$TIER" -maxdepth 1 -name '*.json' ! -name 'META.json' | wc -l | tr -d ' ')"
        if [[ "$baseline_count" -eq 0 ]]; then
            echo "perf-bench: no '$TIER' baseline found under $BASELINE_DIR/$TIER." >&2
            echo "  Run 'scripts/perf-bench.sh save $TIER' first (ideally on a known-good commit)." >&2
            exit 3
        fi
        show_baseline_meta
        if [[ "$MODE" == "compare" ]]; then
            preflight
            echo "perf-bench: running the '$TIER' tier; statistical signals are screened by workload materiality..."
            run_suite
            collect_new "$LOG_DIR"
        elif [[ ! -d "$LOG_DIR" ]]; then
            echo "perf-bench: evaluate-only current results not found under $LOG_DIR." >&2
            exit 3
        fi

        current_count="$(find "$LOG_DIR" -maxdepth 1 -name '*.json' ! -name 'META.json' | wc -l | tr -d ' ')"
        if [[ "$current_count" -eq 0 ]]; then
            echo "perf-bench: no saved current estimates found under $LOG_DIR." >&2
            exit 3
        fi
        if ! python3 "$POLICY_TOOL" validate --policy "$POLICY_FILE" \
            --baseline-dir "$BASELINE_DIR/$TIER" --current-dir "$LOG_DIR"; then
            echo "perf-bench: FAIL — every baseline/current row must have exactly one policy classification." >&2
            exit 1
        fi

        REPORT_FILE="${PERF_POLICY_REPORT_FILE:-${LOG_DIR}.policy-report.txt}"
        if [[ "$MODE" == "evaluate" ]]; then
            if ! python3 "$POLICY_TOOL" validate-report --report "$REPORT_FILE" \
                --baseline-dir "$BASELINE_DIR/$TIER" --current-dir "$LOG_DIR"; then
                echo "perf-bench: FAIL — policy report must not alias a raw baseline/current JSON file." >&2
                exit 1
            fi
            mkdir -p "$(dirname "$REPORT_FILE")"
            : > "$REPORT_FILE"
            POLICY_VERSION="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$POLICY_FILE")"
            POLICY_SHA256="$(sha256sum "$POLICY_FILE" | awk '{print $1}')"
            printf 'Policy-only evaluation: %s\nBaseline: %s\nCurrent estimates: %s\nPolicy: %s\n\n' \
                "$(date -Is)" "$BASELINE_DIR/$TIER" "$LOG_DIR" "$POLICY_FILE" >> "$REPORT_FILE"
            printf 'Policy version: %s\nPolicy SHA256: %s\nInterval scope: Criterion per-run mean confidence-interval screen; excludes cross-process/session variance.\n\n' \
                "$POLICY_VERSION" "$POLICY_SHA256" >> "$REPORT_FILE"
            echo "perf-bench: policy-only evaluation; raw baseline/current estimates are unchanged. Report: $REPORT_FILE"
        fi

        failures=0
        total=0
        new_benches=0
        policy_failures=0
        inconclusive=0
        header="$(printf "%-46s %12s %12s %11s %9s %16s %18s %-14s %-12s %16s %12s %s" "bench" "baseline(ns)" "current(ns)" "delta(ns)" "mean%" "ratio-CI(%)" "delta-CI(ns)" "role" "screen" "impact-CI(ns)" "budget(ns)" "verdict")"
        printf '%s\n' "$header"
        if [[ "$MODE" == "evaluate" ]]; then printf '%s\n' "$header" >> "$REPORT_FILE"; fi
        for cur in "$LOG_DIR"/*.json; do
            [[ -f "$cur" ]] || continue
            id="$(basename "$cur" .json)"
            base="$BASELINE_DIR/$TIER/${id}.json"
            if [[ ! -f "$base" ]]; then
                new_benches=$((new_benches + 1))
                policy_failures=$((policy_failures + 1))
                printf -v line "%-46s %12s %12s %11s %9s %16s %18s %-14s %-12s %16s %12s %s" "$id" "-" "-" "-" "-" "-" "-" "unbaselined" "-" "-" "-" "FAIL"
                printf '%s\n' "$line"
                if [[ "$MODE" == "evaluate" ]]; then printf '%s\n' "$line" >> "$REPORT_FILE"; fi
                continue
            fi
            if ! row="$(python3 "$POLICY_TOOL" classify --policy "$POLICY_FILE" --id "$id" \
                --baseline "$base" --current "$cur" --thresh "$THRESH")"; then
                policy_failures=$((policy_failures + 1))
                printf "%-46s %13s %13s %11s %9s %8s %-14s %12s %12s %s\n" "$id" "-" "-" "-" "-" "-" "invalid" "-" "-" "FAIL" >&2
                continue
            fi
            read -r base_ns cur_ns delta_ns chg ratio_lower ratio_upper delta_lower delta_upper stat_status role impact_lower impact_upper budget verdict <<< "$row"
            total=$((total+1))
            if [[ "$verdict" == "BLOCK" ]]; then
                failures=$((failures+1))
            elif [[ "$verdict" == "INCONCLUSIVE" ]]; then
                inconclusive=$((inconclusive+1))
            fi
            printf -v line "%-46s %12.1f %12.1f %11.1f %8.2f%% [%7.2f,%7.2f]%% [%8.1f,%8.1f] %-14s %-12s [%7s,%7s] %12s %s" \
                "$id" "$base_ns" "$cur_ns" "$delta_ns" "$chg" "$ratio_lower" "$ratio_upper" "$delta_lower" "$delta_upper" "$role" "$stat_status" "$impact_lower" "$impact_upper" "$budget" "$verdict"
            printf '%s\n' "$line"
            if [[ "$MODE" == "evaluate" ]]; then printf '%s\n' "$line" >> "$REPORT_FILE"; fi
        done

        echo ""
        if [[ "$total" -eq 0 ]]; then
            echo "perf-bench: FAIL — zero '$TIER' baselines matched the current run." >&2
            echo "           ${baseline_count} saved baseline(s), ${current_count} current estimate(s)," >&2
            echo "           ${new_benches} current bench(es) have no baseline entry." >&2
            echo "           A compare that matched nothing cannot prove anything; re-save the baseline." >&2
            if [[ "$MODE" == "evaluate" ]]; then printf '\nFAIL: zero current rows matched baseline.\n' >> "$REPORT_FILE"; fi
            exit 2
        fi
        coverage=$((total * 100 / baseline_count))
        echo "perf-bench: $total/$baseline_count '$TIER' baseline benches compared (${coverage}% coverage), $failures material regressions, $inconclusive inconclusive rows, $policy_failures policy/baseline failures."
        if [[ "$coverage" -lt "$MIN_BASELINE_COVERAGE" ]]; then
            echo "perf-bench: FAIL — baseline coverage ${coverage}% is below the required ${MIN_BASELINE_COVERAGE}%." >&2
            echo "           ${baseline_count} saved baseline(s), only ${total} matched. The baseline is" >&2
            echo "           stale or the benchmark names changed; re-save it on a known-good commit." >&2
            if [[ "$MODE" == "evaluate" ]]; then printf '\nFAIL: baseline coverage %s%% is below %s%%.\n' "$coverage" "$MIN_BASELINE_COVERAGE" >> "$REPORT_FILE"; fi
            exit 1
        fi
        if [[ "$failures" -gt 0 ]]; then
            echo "perf-bench: FAIL — Criterion mean-CI regression screen and provisional workload-impact budget both exceeded." >&2
            if [[ "$MODE" == "evaluate" ]]; then printf '\nFAIL: %s material regressions.\n' "$failures" >> "$REPORT_FILE"; fi
            exit 1
        fi
        if [[ "$policy_failures" -gt 0 ]]; then
            echo "perf-bench: FAIL — unbaselined, unknown, or malformed benchmark rows cannot be classified." >&2
            if [[ "$MODE" == "evaluate" ]]; then printf '\nFAIL: %s policy/baseline failures.\n' "$policy_failures" >> "$REPORT_FILE"; fi
            exit 1
        fi
        if [[ "$inconclusive" -gt 0 ]]; then
            echo "perf-bench: INCONCLUSIVE — intervals overlap the statistical or impact boundary; repeat in a clean window." >&2
            if [[ "$MODE" == "evaluate" ]]; then printf '\nINCONCLUSIVE: repeat in a clean window.\n' >> "$REPORT_FILE"; fi
            exit 2
        fi
        echo "perf-bench: PASS (statistical signals below impact budgets are informational)."
        if [[ "$MODE" == "evaluate" ]]; then printf '\nPASS: no blocking material regressions.\n' >> "$REPORT_FILE"; fi
        ;;

    *)
        echo "usage: $0 {save|compare|evaluate|run} [quick|full] [--thresh PCT]" >&2
        exit 2
        ;;
esac
