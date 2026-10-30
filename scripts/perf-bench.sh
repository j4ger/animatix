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
#   scripts/perf-bench.sh compare [quick|full] [--thresh PCT] # rerun; regressions fail
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
#   --thresh PCT  percentage (mean) a bench may regress before `compare` fails, e.g. 5 => +5%.
#   Absolute guardrails are handled separately by scripts/extension-bench.sh.
#
# Exit codes: 0 pass, 1 regressions/insufficient baseline coverage,
#             2 usage/preflight/environment, 3 no baseline to compare against.

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
K=3   # combined std-dev multiples before a real regression is declared
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

    compare)
        baseline_count="$(find "$BASELINE_DIR/$TIER" -maxdepth 1 -name '*.json' ! -name 'META.json' | wc -l | tr -d ' ')"
        if [[ "$baseline_count" -eq 0 ]]; then
            echo "perf-bench: no '$TIER' baseline found under $BASELINE_DIR/$TIER." >&2
            echo "  Run 'scripts/perf-bench.sh save $TIER' first (ideally on a known-good commit)." >&2
            exit 3
        fi
        show_baseline_meta
        preflight
        echo "perf-bench: running the '$TIER' tier and comparing against its baseline (regression beyond combined-noise bound or +${THRESH}% floor fails)..."
        run_suite
        collect_new "$LOG_DIR"

        failures=0
        total=0
        new_benches=0
        current_count="$(find "$LOG_DIR" -maxdepth 1 -name '*.json' | wc -l | tr -d ' ')"
        # print header
        printf "%-46s %13s %13s %9s %8s  %s\n" "bench" "baseline(ns)" "current(ns)" "%chg" "limit%" "verdict"
        for cur in "$LOG_DIR"/*.json; do
            id="$(basename "$cur" .json)"
            base="$BASELINE_DIR/$TIER/${id}.json"
            if [[ ! -f "$base" ]]; then
                new_benches=$((new_benches + 1))   # new bench not in baseline: skip
                continue
            fi
            read -r base_ns cur_ns chg limit_pct verdict < <(compare_estimates "$base" "$cur")
            total=$((total+1))
            if [[ "$verdict" == "REGRESSION" ]]; then
                failures=$((failures+1))
            fi
            printf "%-46s %13.1f %13.1f %9s %8s  %s\n" "$id" "$base_ns" "$cur_ns" "$chg%" "$limit_pct%" "$verdict"
        done

        echo ""
        if [[ "$total" -eq 0 ]]; then
            echo "perf-bench: FAIL — zero '$TIER' baselines matched the current run." >&2
            echo "           ${baseline_count} saved baseline(s), ${current_count} current estimate(s)," >&2
            echo "           ${new_benches} current bench(es) have no baseline entry." >&2
            echo "           A compare that matched nothing cannot prove anything; re-save the baseline." >&2
            exit 2
        fi
        coverage=$((total * 100 / baseline_count))
        echo "perf-bench: $total/$baseline_count '$TIER' baseline benches compared (${coverage}% coverage; ${new_benches} new bench(es) skipped), $failures real regressions detected."
        if [[ "$coverage" -lt "$MIN_BASELINE_COVERAGE" ]]; then
            echo "perf-bench: FAIL — baseline coverage ${coverage}% is below the required ${MIN_BASELINE_COVERAGE}%." >&2
            echo "           ${baseline_count} saved baseline(s), only ${total} matched. The baseline is" >&2
            echo "           stale or the benchmark names changed; re-save it on a known-good commit." >&2
            exit 1
        fi
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
