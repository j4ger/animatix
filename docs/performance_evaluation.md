# Animatix Performance Evaluation Framework & Plan

> Canonical design doc for how Animatix measures, tracks, and drives performance
> work. Read this before starting any performance optimization task: it defines
> *what* is measured, *how* it is measured reproducibly, *where* the results are
> stored, and *what* the first optimization targets should be.

> **Current status:** CI integration is deliberately **paused** while the harness
> is proven in local optimization rounds. Everything in this doc runs locally via
> `scripts/perf-bench.sh` (`save` → optimize → `compare`). Re-enable CI only after
> the gate is stable and non-flaky (roadmap PF-2).

## 1. Why this exists

Animatix has three latency- and throughput-sensitive surfaces:

1. **Interactive authoring (GUI)** — every keystroke / value edit can trigger a
   full **rebuild** (parse → typecheck → expand → `Timeline::build`) plus one or
   more **frame evaluations** and a **GPU render**. The perceived "snappiness" of
   the IDE is dominated by these two costs.
2. **Real-time preview / scrubbing** — per-frame CPU evaluation into a
   `vello::Scene`, then rasterization. Must sustain interactive frame budgets.
3. **Export** — offline rendering of many frames (PNG/WebP/video/GIF). Here
   **throughput** (frames/sec) matters more than single-frame latency, and the
   GPU/FFmpeg path is the dominant cost.

The goal of the framework is to make each of these **measurable, regression
guardable, and comparable** across commits, so optimization work can be justified
by evidence and not intuition.

---

## 2. Current state (review findings)

### 2.1 What is already measured

The project already ships a mature Criterion suite under `crates/animatix/benches/`
(17 benches, all compile against the current API):

| Area | Bench | What it measures |
|---|---|---|
| Build/compile | `build_time.rs` | `Timeline::build` over synthetic workloads |
| Text-heavy rebuild | `text_rebuild.rs` | `Timeline::build` for a 48-actor mixed Text/Code/Typst scene, warm (cache hits) vs cold (`clear_text_compile_cache` per iter) |
| Full editor path | `full_pipeline.rs` | parse → module load → typecheck → expand → build (incl. real examples) |
| Parse-only / build-only | `full_pipeline.rs` | isolation of parse vs. build stage cost |
| Frame evaluation | `timeline_eval.rs` | `Timeline::evaluate` at several times (cache hit) |
| Frame evaluation (no cache) | `scene_costs.rs`, `scaling_no_cache.rs` | full re-evaluation cost |
| Scene scaling | `scaling.rs` | `evaluate` vs. actor count (10→200) |
| Cost breakdown | `cost_breakdown.rs` | frame-env setup vs. property sampling vs. full evaluate |
| Property interpolation | `property_interpolation.rs` | track/existing keyframe sampling + easing lerp |
| Modifier runtime | `modifier_runtime.rs`, `vm_vs_ir.rs`, `vm_regression.rs` | IR executor vs. VM, regression vs. reference |
| Visibility | `visibility_culling.rs` | culling passes |
| Static scene | `static_scene.rs`, `static_scene_item_collection.rs` | scene-only eval + item collection |
| Scrubbing | `scrubbing.rs` | interactive scrub pattern (GUI-adjacent) |

### 2.2 CI gate today

`scripts/extension-bench.sh` runs only **two** benches
(`property_interpolation`, `timeline_eval`) in Criterion *quick* mode and asserts
**one** absolute threshold (`property_plan_lookup_and_sample ≤ 10_000 ns`) in
`.github/workflows/ci.yml`.

### 2.3 Gaps identified

Despite the breadth above, several things are missing for a *trustworthy*
optimization loop:

1. **No long-lived baseline / regression tracking.** Only one bench has any
   threshold; every other bench regresses silently. There is no saved baseline to
   diff against, and no notion of "% change vs. last known-good".
2. **No label grouping by UX surface.** Benches are organized by *implementation
   area*, not by *user-visible property* (rebuild latency, scrub frame latency,
   export throughput). It is hard to answer "is authoring faster?"
3. **GPU / render / export path is unmeasured.** Criterion benches stop at the
   `vello::Scene`; rasterization (offscreen renderer), transitions, and video/GIF
   export throughput are not in the automated suite (they need a GPU, so they
   can't run in the headless CI job).
4. **GUI rebuild latency & preview FPS have no automated numbers.** The GUI has a
   live HUD (`animatix-gui/src/app/preview/performance.rs`:
   `PerformanceMetrics` tracks FPS / rebuild / render time), but that is a
   *display* instrument, not an *automated* suite. `set_gpu_memory` is even
   marked `#[allow(dead_code)] // Reserved for GPU memory tracking integration`.
5. **No memory profiling.** Peak RSS / allocations / GPU texture memory are not
   captured, so allocation storms (e.g. per-frame `Vec` clones, `SceneItem`
   collection) are invisible.
6. **No hierarchical stage tracing in production code.** `cost_breakdown` is
   hand-rolled for one scene shape; there is no uniform, always-available
   per-stage timer (rebuild / build-frame-env / sample / encode / rasterize) that
   both the benchmark suite and the GUI HUD could share.
7. **Narrow scenario corpus.** Most benches use small synthetic scenes; real
   dogfood/sorting-visualizer/generated scenes are only lightly exercised
   (`full_pipeline` loads a handful of examples). Large / reactive / generated
   scenes — the worst case for the planner, modifier runtime, and layout — are
   underrepresented.

---

## 3. Evaluation framework design

### 3.1 Core principle: three layers, one ledger

Layering gives fast, cheap signals at the bottom and slow, high-fidelity signals
at the top. Every layer writes into the **same result ledger** so a single
`scripts/perf-report.sh` can summarize everything.

```
Layer 1  Micro/Criterion benches (CPU, deterministic, CI-friendly)
Layer 2  Scenario suite (CPU+GPU, real .amx workloads, run on demand/GPU CI)
Layer 3  In-app GPU/export throughput + GUI HUD telemetry
         └── all feed → result ledger (JSON) → perf-report summary
```

### 3.2 Layer 1 — Criterion micro-suite (already exists, needs baselines)

Policy changes over the current state:

- **Every bench declares a name-spaced group** prefixed by the UX surface so
  reports read as *user-visible* costs:
  - `rebuild.*` (parse/typecheck/expand/build, per scene size)
  - `frame.*` (evaluate happy path + `frame.no_cache`)
  - `scrub.*` (consecutive distinct-time evaluates, GUI scrubbing)
  - `interp.*` (sampling/lerp leaf costs)
  - `modifier.*` (IR executor, fn calls)
- **Baselines are saved to `criterion/<branch>/*.json`** (gitignored) via
  Criterion's `--save-baseline` / `--load-baseline` pairing. A dedicated
  `perf-bench.sh` runs the full suite, compares to the last saved baseline, and
  exits non-zero on any bench that regressed beyond a threshold.

### 3.3 Layer 2 — Scenario suite (new)

A set of realistic `.amx` workloads drawn from `examples/` and `dogfood/`,
parameterized to stress the expensive paths:

| Scenario | Stresses |
|---|---|
| `sorting-visualizer/entry.amx` | generated actors, many tracks, sequence/stagger actions |
| `examples/animation/16_showcase.amx` | feature density, mixed primitives |
| `examples/data/23_plot_kinds.amx` | plot path generation (vector paths) |
| `examples/generation/*` | reactive `always` blocks → frame env + modifier runtime |
| a synthetic `N`-actor generated scene (scaling sweep) | planner, layout, scene flatten |

Each scenario reports **rebuild time**, **first-frame eval**, **steady-state eval
(cache hit)**, and — when a GPU is available — **raster time** and **export
throughput**. Scenarios live under `crates/animatix/benches/` as Criterion benches
reusing `common.rs`, so they get baselines for free.

### 3.4 Layer 3 — GPU & in-app telemetry (new, where a GPU is available)

- A **non-CI** binary/feature (e.g. `animatix-cli perf` or a dedicated bench run
  inside `nix develop`) that:
  - renders representative scenes through `OffscreenRenderer` and reports real
    raster ms / FPS;
  - encodes a short clip through the video pipeline and reports throughput
    (frames/s) and encode wall time;
  - reports peak RSS and (where exposed) GPU texture memory — this is the home
    for the currently-unused `PerformanceMetrics::set_gpu_memory`.
- **GUI HUD telemetry export**: add a `--perf-log <path>.jsonl` flag to the GUI
  that, during a normal interactive session, appends one JSON line per frame
  (`ts, fps, rebuild_ms, render_ms, scene_size, actors`) so real-authoring data
  can be collected without a synthetic driver. `PerformanceMetrics` already has
  the fields; this only adds a sink.

### 3.5 Shared stage tracing (recommended, cross-cutting)

Introduce a lightweight, always-on (in debug/bench) instrumentation layer so the
bench suite and the GUI HUD measure the *same* stages rather than re-deriving
timings:

```rust
// crates/animatix/src/perf.rs (new)
pub struct StageTimers;                      // thread-local, cheap
pub struct ScopedStage<'a>(&'a str, Instant);
impl ScopedStage<'_> {
    pub fn new(name: &str) -> Self;          // pushes name + start
}
impl Drop for ScopedStage<'_> { /* records to thread-local ring */ }

pub fn take_measurements() -> Vec<(String, Duration)>;  // drained by callers
```

Stages to cover (reuse the exact same names in benches and HUD):
`rebuild`, `build_frame_env`, `sample`, `layout`, `modifier_exec`, `encode_scene`,
`rasterize`, `export`. Gating: enabled by a compile-time default-on flag so the
production hot path pays only a thread-local push/pop unless compiled out (see
`7. Cost of instrumentation`).

### 3.6 Result ledger & reporting

Criterion already writes `target/criterion/*/estimates.json`. `perf-report.sh`
merges those with Layer-3 GPU/export numbers into a single
`target/perf/latest.json` and prints a human table:

```
surface       bench                         mean        vs baseline
rebuild       sorting-visualizer_full       1.42 ms     +3.2%  (ok)
frame         mixed_scene_evaluate          0.93 ms     -1.4%  (ok)
scrub         scrub_10_actors_500ms         2.1 ms      +12%   ⚠ REGRESSION
export        showcase_1080p_fps            412 fps     (gpu, latest only)
```

---

## 4. How regressions are caught (gates)

1. **Noise-adaptive relative gate** (`scripts/perf-bench.sh compare`): loads the
   last saved baseline and flags a bench as a regression only when its mean rose
   by more than **K combined standard deviations** of both runs (default `K=3`,
   so `limit = max(3·(σ_base+σ_cur)/μ_base, THRESH)`) with an absolute floor of
   `THRESH` (default **+5%**, `--thresh`). This is deliberate: stable leaf
   benches (tiny σ) get a tight ~5% bound, while noisy build/planner benches
   (large σ) get a wide bound, so real slowdowns are caught and run-to-run noise
   is not. `compare` exits non-zero on any flagged regression. `extension-bench.sh`
   wraps the same idea for the existing absolute guardrail.
2. **Absolute guardrail** (existing, kept): `extension-bench.sh --max-plan-ns`
   for the one fixed hot leaf (`property_plan_lookup_and_sample`).
3. **GPU/export** (merged into the result ledger, not a gate in headless CI;
   becomes a gate in the GPU CI runner once one exists).

Rationale: a *relative* gate catches regressions regardless of machine speed; an
*absolute* gate catches catastrophic slow-downs. A pure fixed-percentage gate
would either false-flag noisy build benches (seen empirically: `build_time` sways
±15–20% run-to-run on the same machine) or go blind on tight leaves — hence the
standard-deviation rule, which adapts to each bench's own noise.

---

## 5. Optimization plan (first targets, evidence-backed)

Ordered by expected authoring-impact ÷ effort. Each item lists the **metric**
it moves and the **gate** that protects it.

### P1 — Frame-evaluation hot path (biggest authoring win)
- **Target:** `frame.*`, `scrub.*` — get steady-state eval well under the 16.7 ms
  frame budget at 1080p for typical scenes; get first-frame eval (no-cache) into
  single-digit ms.
- **Suspects:** per-frame `Vec`/`SceneItem` clones, allocation churn in
  `encode_scene`, redundant re-encoding on cache-hit restore
  (`restore_frame_cache` currently `clone()`s the whole `SceneProgram`), layout
  re-computation, and `property_plan_lookup_and_sample`.
- **Gate:** `frame.*`, `scrub.*` baselines in CI.

### P2 — Rebuild latency (keystroke-to-preview)
- **Target:** `rebuild.*`, `full_pipeline.*` — target sub-10 ms for small scenes,
  bounded for large/generated scenes.
- **Suspects:** O(n·m) work in `Timeline::build` (per-track / per-keyframe),
  `expand_components` recursion on generated scenes, planner
  (`property_plan_lookup_and_sample`), keyframe consolidation across scenes.
- **Gate:** already partially gated; extend the `extension-bench.sh` pattern to
  the whole `rebuild.*` group.

### P3 — Allocation / memory profile
- **Target:** peak RSS + allocation count on P1/P2 scenarios; find per-frame
  `Vec` clones (cache-hit restore, `SceneItem` collection, hit regions).
- **Approach:** add `perf` memory capture, profile with DHAT/tracy on the
  scenario suite, eliminate steady-state allocation in the hot path.
- **Gate:** `frame.*` (allocation count is a strong proxy for eval time).

### P4 — GPU / export throughput
- **Target:** raster ms and video/GIF encode FPS on showcase scenes.
- **Approach:** Layer-3 `perf` binary inside `nix develop`; sanity-check the
  renderer paths (`OffscreenRenderer`, `fullscreen_blit`, filter backend)
  surfaced during export; revisit `PerformanceMetrics::set_gpu_memory`.
- **Gate:** GPU CI runner (future) or on-demand `perf-report` only.

---

## 6. Deliverables / file map

| Item | Location | Status |
|---|---|---|
| This design doc | `docs/performance_evaluation.md` | **added** |
| Bench harness + baseline/regression script | `scripts/perf-bench.sh` | **added** |
| CI perf-report job (runs suite, uploads Criterion report + baseline artifacts) | `.github/workflows/ci.yml` | **paused** — CI integration deliberately deferred; prove the harness in local optimization rounds first, then re-enable (PF-2) |
| Result merge/report | `scripts/perf-report.sh` | add |
| Persistent cross-run baselines + hard relative gate | artifacts / `perf-bench compare` in CI | add (PF-3) |
| Shared stage tracing | `crates/animatix/src/perf.rs` + `ScopedStage` | add (PF-8) |
| Scenario suite benches | `crates/animatix/benches/` | add |
| GPU/export + memory capture | `animatix-cli perf` (or bench under `nix develop`) | add (PF-7) |
| GUI JSONL perf sink | `animatix-gui` `--perf-log` | add (PF-9) |
| Roadmap backlog | `docs/roadmap.md` | **added** (PF-1…PF-9) |

---

## 7. Cost of instrumentation (must-read before adding tracing)

The always-on stage tracer in `3.5` should be measurement-only and cheap, but do
not let it perturb the very numbers it reports:

- Use a **thread-local ring** of `(name, Instant)`; no allocation in the hot
  `Drop` path (reuse a fixed-size buffer) unless drained.
- Keep `Duration` captures behind `#[inline]` push/pop; a `tracing::debug!` per
  frame is acceptable, a per-actor `tracing::event` is not.
- Make it **compile-time switchable** (a `perf-tracing` default-on feature) so a
  non-optimized instrumentation build can be excluded from optimized benches if
  it ever shows up in the numbers. Benchmark the benchmarks: compare
  `evaluate` with and without the tracer on one scene in `cost_breakdown` before
  trusting timings.

---

## 8. Workflow for a performance task

1. Read this doc and `docs/architecture.md` (§3 runtime, §renderer).
2. Ensure a baseline exists: `bash scripts/perf-bench.sh save`.
3. Make the change. Re-run `bash scripts/perf-bench.sh compare` — it fails on
   regressions and prints the delta table.
4. If GPU/export affected, run the Layer-3 perf binary in `nix develop`.
5. Update `docs/roadmap.md`; commit via `cog commit perf "<summary>" <scope>`
   (scope `renderer` / `timeline` / `gui` as appropriate).
