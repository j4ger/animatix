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
| Equation frames | `equation_frame.rs` | per-frame Typst recompilation of an `Equation`/`Fragment` subtree (dynamic scene, so the static-subtree cache does not apply) |

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
| `examples/gallery/fft_explain.amx` | feature density, mixed primitives |
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

  > **Status (PF-9, 2026-08-31): implemented** as
  > `crates/animatix-gui/src/app/perf_log.rs` (`PerfLogSink`). Each line also
  > carries a `stages` map drained from the shared stage tracer
  > (`animatix::perf::take_measurements()`), so collected data uses the exact
  > stage names the bench suite gates on. Threading note: stage durations are
  > UI-thread only (the `rebuild` stage runs on the worker thread and is
  > covered by the top-level `rebuild_ms` field instead). The sink disables
  > itself with a single warning after the first I/O error.

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

> **Status (PF-8, 2026-08-31): implemented** as
> `crates/animatix/src/perf.rs` behind the default-on `perf-tracing` feature.
> Instrumented seams today: `rebuild` (`Timeline::build_impl`),
> `build_frame_env`, `modifier_exec`, `sample` (the root-node evaluation loop —
> scene encoding is interleaved, so `encode_scene` is not yet split out),
> `layout` (taffy linear layout), and `rasterize`
> (`Renderer::render_vello_scene_with_background`). `encode_scene` and `export`
> seams remain reserved until PF-7 splits them. Bench/GUI consumers drain via
> `perf::take_measurements()` per frame/iteration.

> **Bench consumer (2026-08-31):** `crates/animatix/benches/stage_breakdown.rs`
> gates the shared stage names: for a generated 60-actor dynamic scene
> (modifiers + container) it reports the per-frame miss cost of
> `build_frame_env`, `modifier_exec`, and `sample`, plus `rebuild`, and the
> wall-time miss total via `iter_custom`. Measured split (60 actors,
> 1080p, every frame a cache miss): `sample` ≈ 47 µs (~88% of the 53 µs miss
> budget), `build_frame_env` ≈ 7.4 µs, `modifier_exec` ≈ 1.2 µs — so per-node
> evaluation + scene encoding (`sample`) is the dominant PF-4 remaining cost.
> `layout` fires only on the first frame for this scene (static layout is
> cached), so it is skipped by the multi-frame warmup guard. 2026-09-03 note:
> a per-node `encode_scene` `ScopedStage` was **tried and rejected** — it adds
> ~2 µs to every `evaluate`-based bench and violates the §7 "a per-actor
> tracing event is not acceptable" rule, so the `encode_scene` seam stays
> reserved for a collect-then-encode split (PF-7). `stage/sample` also drifts
> ±12% run-to-run on this fixture (31 µs → 35 µs on unchanged code), so
> treat its `change:` line as noise, not a gate.
>
> **Attribution caveat (2026-09-03):** the `layout` skip means the
> dynamic-layout *hit* path (`compute_animated_layout`: admission lookup +
> key build + positions clone per container per frame) is invisible to every
> stage name — probes put it at ~14.6 µs/frame, ~40% of `sample`, and the
> "property sampling ≈14 µs" figure above was actually dominated by it. After
> the allocation-free layout cache (PF-4, 2026-09-03) `stage/sample` measured
> 34.7 → 26.2 µs (−24%, reproduced 3×, `build_frame_env` control flat). When a
> stage bench reports an unexplained residue, probe the paths OUTSIDE the
> instrumented stage names before trusting a sub-stage attribution.
>
> **Steady-state profiling driver (2026-09-03):** Criterion profiles proved
> unreliable for hot-path attribution twice — one-time setup (fontdb scans,
> roxmltree/chumsky parse) and suite neighbours contaminate the capture, and
> per-node probe push/pops (~55–90 ns each) swamp sub-5 µs stages.
> `crates/animatix/examples/perf_driver.rs` runs the stage_breakdown scenario
> in a tight loop with a settle phase:
> `perf record -e cycles:u -c 50001 -- taskset -c 0 target/release/examples/perf_driver`
> yields a clean function-level ranking (604k samples). With the layout cache
> fixed (PF-4), the closed attribution for the evaluate loop is: string-keyed
> map machinery ≈30%, allocator ≈18%, scene_eval inlined bodies ≈13%,
> `resolve_property` 3.6% (→2.0% after the pre-resolved transform reads),
> property-track sampling ≈6%, layout 2.6%, `evaluate_vector_paths` 1.4%
> (→ early-out), vello encode ≈2.8%. After round 3 (pre-resolved
> `size`/`transform` reads + pooled `precise_bounds_cache` label keys),
> `resolve_property` no longer appears in the profile at all and
> `stage/sample` sits at 22.2 µs (cumulative 34.7 → 22.2 = −36% over three
> rounds).

> **Steady-state allocation driver (2026-09-04, PF-6):**
> `crates/animatix/examples/alloc_driver.rs` runs the same scenario under the
> dhat allocator and installs the profiler *after* the settle phase, so
> `dhat-heap.json` ranks exactly the steady-state `evaluate` loop by bytes /
> allocation count. Build with `RUSTFLAGS="-C force-frame-pointers=yes"`
> (frame pointers keep the backtraces resolvable in a release build) and
> `--profile profiling`. Allocation counts are deterministic — unlike the
> ±5–12% timing drift documented in §4 — so this is the preferred ranking
> metric for allocation-class work; timing A/Bs remain the regression gate.
>
> First capture (60-actor dynamic scenario, 10k frames): **600 allocations
> and ≈500 KB churn per frame, live set ≈ 0** (pure churn), peak 443 KB.
> Ranked by first animatix frame in the backtrace:
>
> 1. **`build_frame_env_internal` overrides `reserve` — one ≈430 KB table
>    allocation per frame (86% of churn bytes).** The estimate sized from
>    `self.env.len()`, but `Environment::with_base` shares the base layer and
>    `inject_runtime_lookup_values` only injects referenced-roots-filtered
>    tracks: the map held 131 entries while being reserved for ~2600.
>    **Fixed (2026-09-04):** the estimate now sizes from the injected-actor
>    count × 120 (measured inserts per Rect track = 117; the ×35 constant
>    previously assumed per actor was itself too small — under-reserving
>    triggered SipHash rehashes that cost +60% on `stage/build_frame_env`,
>    which is why the multiplier must err generous). Two implementation
>    lessons landed with it: (a) a first version added a *second*
>    `has_procedural_plots()` call — an O(tracks) scan — and that alone
>    regressed the `env_50`/`env_200` micro-benches +13/+25%; computing
>    `has_runtime_injection` once and reusing it in the fast-path condition
>    restored them (env_50 −8.0% vs baseline). (b) Result: churn
>    500 → 96.6 KB/frame (−81%), peak 443 → 39.8 KB (−91%), allocation count
>    flat at 600; `stage/build_frame_env` unchanged (6.13 → 6.22 µs vs an
>    untouched-code baseline, within drift), `stage/eval_total` −2.5% on the
>    miss-frame spot check, and the full 64-bench gate passed with **0
>    regressions** — the time gain itself stays *not* gateable (kept per the
>    "strictly removes work" precedent, §4).
> 2. **`KurboShape::to_path` → `BezPath::from_iter`** — ~70 blocks,
>    23.5 KB/frame: every shape actor re-converts its geometry to a bezpath
>    every frame even when size is frame-constant (candidate: share/cache the
>    path and let the transform vary).
> 3. **`Environment::set` `String` keys + `format!`-built env keys** —
>    ~208 `to_string` blocks/frame across property injection, env-key
>    helpers, and `apply_override_incremental` (candidate: pooled/reused
>    keys, or pooling the whole frame env per §5 P1).
> 4. **`morph::evaluate_paths_with_options` via `evaluate_vector_paths`** —
>    ~61 blocks, 4.9 KB/frame cloning the path `Vec` per actor per frame even
>    when the track is at a constant value (candidate: `Arc<[VelloPath]>`
>    sharing when no morph/keyframes).
> 5. **Dynamic-layout residues** — `compute_animated_layout` Box/Table
>    allocations ~60 blocks + ~10 KB/frame despite the allocation-free hit
>    path (probe the key-build and `compute_layout_for_time` internals).
> 6. **`precise_bounds_cache` clone** — one ≈3.7 KB table clone/frame at
>    `scene_eval.rs` `evaluate_program_inner` (candidate: `Arc`-share like
>    the layout positions).

> **Round 2 (2026-09-04, later the same day): items 4/5 fixed + a real leak
> found.** (a) **Vector-path Arc memo**: `evaluate_vector_paths` returns
> `Arc<Vec<VelloPath>>` and memoizes the time-independent region (no
> keyframes, or past the last keyframe where the evaluator returns the frozen
> `last_value`) per track, guarded by a `vector_paths_epoch` bumped by
> `invalidate_frame_cache` — the funnel every mutation goes through. Build
> callers use the unmemoized `evaluate_vector_paths_value`. (b) **Shared
> empty `Arc<LayoutPositions>`** replaces the per-node `Arc::new(empty map)`
> in `compute_animated_layout`'s missing-metadata early return and the
> `actor_world_affine` reset sites. Combined: 600 → **420 blocks** and
> 96.6 → **71.0 KB** per frame (cumulative −86% bytes vs the pre-PF-6
> 500 KB); `stage/sample` 22.2 → **20.34 µs (−8.3%)** and `eval_total`
> −5.6% — a real time win this round, because the 60 per-frame deep clones
> were CPU work, not just allocator churn.
>
> **The leak (found by the same instrument, pre-existing):** running the
> `scene_costs` benches ballooned to ~21 GB RSS with dhat showing **15M live
> blocks at exit — 50 per frame, one per actor — all retained inside
> `restore_frame_cache`'s `bounds_key_pool` extend**. Root cause: every cache
> hit drains the bounds map into the pool and re-extends, but a pure
> cache-hit workload never *pops*, so the pool grew by one key per node per
> hit forever (introduced with the pool itself, `01cb616c`, 2026-09-03).
> Fix: `EvalCaches::recycle_bounds_keys` caps the pool at 512 keys. Same
> workload after the fix: `curr_bytes` flat at 0.12 MB, RSS flat at ~15 MB,
> bench-binary peak 22.8 GB → **60 MB**. Corollaries recorded here because
> they revise earlier entries: (1) the "22 GB" observed during suites was
> **the leak, not rustc's debuginfo linker spike** (that spike exists but is
> transient and smaller); (2) the §4 note that `many_actors_evaluate` /
> `many_actors_cache_hit` "contradict each other run-to-run (±9–17%)" was
> almost certainly **leak-driven memory pressure**, not bench instability —
> after the fix those benches measured 1.39/1.40 µs with
> `timeline_evaluate_1s/2s` collapsing −43% (176.5 → 100.0 ns) in the full
> gate. Treat those two as reliable again; re-baseline them.
>
> `examples/leak_probe.rs` is the diagnostic template for "leak vs
> fragmentation": sample `dhat::HeapStats` (`curr_bytes` = live set) and
> process RSS over a fixed-time evaluate loop — live growing with total is a
> leak, total growing with live flat is churn/fragmentation.

> **Round 5 (2026-09-04): `precise_bounds` Arc sharing — measured and
> rejected.** The Round-4 list's last cheap candidate: handing the frame-end
> bounds table to the `SceneProgram`/`FrameCacheEntry` as an `Arc` snapshot
> (frame-start reclaim via `try_unwrap`, hit-path pointer swap, pool-fed key
> rebuild). Allocation verdict as predicted: 18.2 → 14.6 KB/frame churn
> (the 60-key table clone vanished). **Time verdict: a net loss** —
> adjacent A/B on the un-committed change measured `full_200` 16.4 →
> 17.0 µs (+3.3%) and `offscreen_100_actors` +8.2%; per-node
> `Arc::make_mut` atomic refcount traffic and the reclaim pass cost more
> than the allocator saved, even though every "removed work" argument held.
> Reverted; the round adds the item to the §5 do-not-re-attempt list
> alongside the `PrimitiveRegistry::find` index and the viewport-check
> resampling: **bounds-table sharing needs a design that avoids per-node
> Arc indirection in the render path** (e.g. batching bounds into a flat
> `Vec` keyed by a dense per-frame slot id instead of string-keyed maps)
> — do not re-attempt the string-keyed `Arc` variant without new evidence.
> Session note for harness users: this is the third flag-then-isolate
> save this session (two of them flagged build-path benches that failed
> isolation; this one flagged evaluate-path benches that reproduced).

> **Round 3 (2026-09-04, same day): the frame env is pooled — the §5 P1
> "allocation-free hot path" candidate.** The override-layer key set is
> identical frame-to-frame (labels × registry properties are build-time
> fixed), so `evaluate_program_inner` hands the finished environment back to
> a one-slot `Timeline::env_pool` and `build_frame_env_internal` takes it
> back for the next frame. Two supporting changes: `Environment::set` now
> overwrites in place via `get_mut`-first (a pooled env's keys already
> exist — no key copy at all; `set_owned` moves caller-built keys), and
> `reset_for_reuse` re-stamps the memoization identity and clears plot
> bindings on take. Invariants: (1) `invalidate_frame_cache` drops the pool
> — every mutation funnels there, so fresh-build semantics return whenever
> the injection key set may change; (2) the only cross-frame key-set
> variance is a variable track evaluating to `None` before its first
> keyframe, handled by removing the stale key on the pooled path; (3) only
> the scene-eval path pools (the public `build_frame_env` hands ownership to
> callers the pool cannot see; the plot path's `local_env` clones are
> unaffected). Measured: 403.8 → **271.8 blocks**, 70.8 → **41.8 KB** per
> frame, peak 39.8 → **10.7 KB** (−73%); cumulative vs pre-PF-6:
> **−55% blocks, −92% bytes**. Time follows: `stage/build_frame_env`
> 6.15 → **5.23 µs (−15%)**, `stage/sample` 19.6 µs, and the full gate's
> `evaluate_25/50_actors` **−94/−92%**, `evaluate_100_actors` −61%,
> `reactive_playback_100frames` −52%, `mixed_scene_evaluate` −50%.
> Three benches flagged (`reactive_parse_only` +93%, `reactive_build_only`
> +84%, `rebuild` +11.5%) — all three **failed to reproduce in isolation**
> (68.7 µs / 265.0 µs / 2.007 ms vs baselines 68.2 / 261.5 / 2003.8), the
> §4 process-state contamination again; none of these stages is touched by
> pooling. The flag-then-isolate protocol from §4 is what kept this round
> honest.

> **Round 4 (2026-09-04, same day): shared shape bezpaths + a render-path
> memo.** `VelloPath.path` is now `Arc<BezPath>` (morph/path-list clones
> become refcount bumps; `Arc::make_mut` preserves the two in-place affine
> edit sites), and the shape→path conversion is memoized per track:
> `AnimationTrack::shape_path_memoized(&KurboShape)` keys on the sampled
> shape (self-validating — geometry change misses and rebuilds; the default
> tolerance is fixed), with the memo living on `ShapeTracks` and reached
> through a new `RenderCtx.track` field. rect/ellipse/line/polygon use it;
> the build-time helper (`build_vector_shape_vello_path`) uses a
> thread-local scratch track — a throwaway `AnimationTrack::new` per call
> showed up in the gate as a rebuild regression. Measured: 271.8 →
> **204.2 blocks**, 41.8 → **18.2 KB** per frame (cumulative vs pre-PF-6:
> **−66% blocks, −96% bytes**); `stage/sample` 19.6 → **18.0 µs**.
> Gate flags `rebuild`/`modules_full`/`components_full` (all build-path)
> failed to reproduce in isolation — adjacent A/B with/without the change
> measured 4.98 vs 4.98 ms on `modules_full`. Round-trip note: an unforced
> `git checkout HEAD -- src` between the flag and the re-measure discarded
> the first copy of this change; it was replayed from the recorded edits
> and verified against the pre-loss alloc numbers (204.2 blocks) before
> committing. A second full-suite run flagged five more build-path benches
> (`rebuild` +12%, `simple_build_only` +7.7%, `modules_full` +6.9%,
> `frame_env_only` +6.1%, `components_full` +5.8%); isolation results:
> `rebuild` 2.026 ms (+1.1%), `frame_env_only` 299.9 ns (**bit-identical**),
> `modules_full`/`components_full` matching the adjacent A/B (session
> drift) — only `simple_build_only` +2.8% may be a real per-declaration
> memo/RefCell overhead on the declaration path, accepted against the
> evaluate-path wins and far below any keystroke-latency concern. Note:
> `Environment::set`'s get_mut-first ordering double-hashes keys on a
> *fresh* environment (pool-cold paths) — measured invisible at this
> scale; revisit only if a build-path regression reproduces.

> **Steady-state profiling driver (2026-09-03):** Criterion profiles proved
> unreliable for hot-path attribution twice — one-time setup (fontdb scans,
> roxmltree/chumsky parse) and suite neighbours contaminate the capture, and
> per-node probe push/pops (~55–90 ns each) swamp sub-5 µs stages.
> `crates/animatix/examples/perf_driver.rs` runs the stage_breakdown scenario
> in a tight loop with a settle phase:
> `perf record -e cycles:u -c 50001 -- taskset -c 0 target/release/examples/perf_driver`
> yields a clean function-level ranking (604k samples). With the layout cache
> fixed (PF-4), the closed attribution for the evaluate loop is: string-keyed
> map machinery ≈30%, allocator ≈18%, scene_eval inlined bodies ≈13%,
> `resolve_property` 3.6% (→2.0% after the pre-resolved transform reads),
> property-track sampling ≈6%, layout 2.6%, `evaluate_vector_paths` 1.4%
> (→ early-out), vello encode ≈2.8%. After round 3 (pre-resolved
> `size`/`transform` reads + pooled `precise_bounds_cache` label keys),
> `resolve_property` no longer appears in the profile at all and
> `stage/sample` sits at 22.2 µs (cumulative 34.7 → 22.2 = −36% over three
> rounds).

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

**Known limitation (observed 2026-08-31):** sub-100ns leaf benches
(`timeline_evaluate_*`) can drift **+5–8% between benchmark sessions** on the
same machine while staying internally consistent within a session (all three
`timeline_evaluate_*` moved from ~90.5 ns to ~97 ns together — CPU frequency /
code-layout shift, not a code change). Protocol when the gate flags such a leaf:
(1) re-run the bench *filtered* — if the whole session level shifted, siblings
move too and the flag is environmental; (2) only treat it as real if the
filtered re-run reproduces the delta against a same-session comparison. A
verified optimization lands with a re-saved baseline, which also resets the
session level.

**Known limitation (observed 2026-09-01):** that drift is not confined to
sub-100ns leaves — on 45–55 µs benches a whole session can shift **2–3.5%**,
the same order as a worthwhile optimization. Two techniques made A/Bs
trustworthy. (1) Run the comparison **adjacently** (`git stash` → bench →
`git stash pop` → bench) instead of against a baseline saved earlier in the day.
(2) Include an **untouched stage in the same process** as a control and
normalize against it: `stage/build_frame_env` and `stage/rebuild` are ideal
because no frame-evaluation candidate touches them, so their delta *is* the
session drift. Applying both, per-node allocation cleanups in `sample` measured
−1.2% (i.e. noise) even though they strictly remove work — they were kept, but
not claimed as a win. Separately, `many_actors_evaluate` and
`many_actors_cache_hit` (both `scene_costs.rs`) exercise what should be
identical frame-cache-hit work, yet moved ±9–17% in *opposite* directions
across identical A/Bs; do not read a regression off either one in isolation
until PF-3/PF-6 makes the harness robust.
>
> **Update (2026-09-04, resolved):** the contradiction was
> **leak-driven memory pressure**, not bench instability — see the Round-2
> note in §3.5. `restore_frame_cache`'s unbounded `bounds_key_pool` grew on
> every cache hit, and the swap thrash it caused explains the ±9–17%
> inversions. With the pool capped, the pair measured 1.39/1.40 µs on
> back-to-back runs and `timeline_evaluate_1s/2s` collapsed −43% in the full
> gate. Read those benches normally again; re-baseline before gate use.

**Known limitation (observed 2026-09-03, `stage_breakdown`):** the drift is
**process-state / order-dependent**, and two common mitigations were tested and
rejected: (a) CPU pinning via `taskset -c 0` did **not** change the mean (pinned
spread was actually worse than unpinned); (b) raising Criterion's
`--warm-up-time` to 8 s did **not** close the gap (full-suite `stage/sample`
stayed ~35 µs vs the ~30.4 µs filtered value at any warm-up). The same code
measures differently because of allocator/code-layout/CPU state left behind by
the *preceding* benches running in the same process — `eval_total` runs ~24 s of
sustained `evaluate` before `stage/sample`. Within-run variance is genuinely tiny
(`stage/sample` std ≈ 0.7%), so more samples / longer measurement does not help.
The only reliable handling remains the **control-normalization** and
**adjacent-A/B** techniques above (measure an untouched control like
`build_frame_env` in the same run and diff it away), plus re-saving the baseline
after each verified optimization. Do not attempt to "fix" this with pinning,
longer warm-ups, or an auto-re-run-in-isolation gate recheck: the last is biased
because an isolated re-run is compared against a full-suite baseline and will
dismiss genuine regressions.

---

## 5. Optimization plan (first targets, evidence-backed)

Ordered by expected authoring-impact ÷ effort. Each item lists the **metric**
it moves and the **gate** that protects it.

### P1 — Frame-evaluation hot path (biggest authoring win)
- **Target:** `frame.*`, `scrub.*` — get steady-state eval well under the 16.7 ms
  frame budget at 1080p for typical scenes; get first-frame eval (no-cache) into
  single-digit ms.
- **Suspects:** per-frame `Vec`/`SceneItem` clones, allocation churn in the
  scene-encode path (currently interleaved with sampling in
  `Timeline::evaluate_node`), layout re-computation, and
  `property_plan_lookup_and_sample`. (2026-08-31 refresh: the earlier suspect
  "cache-hit restore clones the whole `SceneProgram`" is resolved — PF-4 made
  `restore_frame_cache` return a thin program that deep-copies only the scene;
  remaining cost there is the no-cache miss path, ~33 µs.) Use the
  `perf-tracing` stages (`crate::perf::stage`) plus `perf record` on a
  `[profile.bench]` (debug symbols) build to rank these with evidence.
- **Gate:** `frame.*`, `scrub.*` baselines in CI.

### P2 — Rebuild latency (keystroke-to-preview)
- **Target:** `rebuild.*`, `full_pipeline.*` — target sub-10 ms for small scenes,
  bounded for large/generated scenes.
- **Done so far:** process-wide font DB sharing (PF-5, commit `5b12b015`);
  process-wide memoization of Text/Code/Typst compilation keyed on all inputs
  (font-environment epoch guards staleness); O(1) environment-stamp key for the
  build-time expression cache; and referenced-root filtering of build-env
  injection (`build::referenced_roots` pre-scans the expanded AST, injecting
  only actor labels that appear in expressions — over-injection safe,
  under-injection fails loudly). Cumulative `text_rebuild/mixed_48_warm`:
  49.6 ms → 0.41 ms; `components_full` −58%; lib test suite 58 s → 9 s.
- **Remaining suspects:** `expand_components` recursion on generated scenes;
  per-rebuild glyph `Vec` clones for text actors (store `Arc<[TextPath]>` in
  tracks); cold-build Typst compilation cost is now the dominant term.
- **Gate:** already partially gated; extend the `extension-bench.sh` pattern to
  the whole `rebuild.*` group.

### P3 — Allocation / memory profile
- **Target:** peak RSS + allocation count on P1/P2 scenarios; find per-frame
  `Vec` clones (cache-hit restore, `SceneItem` collection, hit regions).
- **Approach:** add `perf` memory capture, profile with DHAT/tracy on the
  scenario suite, eliminate steady-state allocation in the hot path.
- **Status (2026-09-04, five rounds done):** `alloc_driver` + dhat capture
  is the memory instrument (see the driver note in §3.5). Ledger: round 1
  frame-env reserve (−81% churn bytes) + the unbounded `bounds_key_pool`
  leak fix (21 GB → 60 MB on `scene_costs`); round 2 vector-path Arc memo +
  shared empty layout maps; round 3 pooled frame env; round 4 shared shape
  bezpaths — cumulative **−66% blocks / −96% bytes**, `stage/sample`
  −48% (34.7 → 18.0 µs). Round 5 (`precise_bounds` Arc sharing) was
  measured and REVERTED — see the §3.5 Round-5 note; allocation savings do
  not pay for per-node Arc indirection in the render path.
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
| Result merge/report | `scripts/perf-report.sh` | **added (PF-3 foundation)**; emits `target/perf/latest.json` |
| Persistent cross-run baselines + hard relative gate | `PERF_BASELINE_DIR` + `perf-bench compare` | **added (PF-3 foundation)**; CI artifact upload/download remains PF-2 |
| Shared stage tracing | `crates/animatix/src/perf.rs` + `ScopedStage` | **added** (PF-8, 2026-08-31; `perf-tracing` default-on feature) |
| Scenario suite benches | `crates/animatix/benches/` | add |
| Steady-state time driver | `crates/animatix/examples/perf_driver.rs` | **added** (2026-09-03; §3.5 note) |
| Steady-state allocation driver (dhat) | `crates/animatix/examples/alloc_driver.rs` + `examples/scenario_60actors.rs` | **added** (PF-6, 2026-09-04; §3.5 note) |
| GPU/export + memory capture | `animatix-cli perf` (or bench under `nix develop`) | add (PF-7) |
| GUI JSONL perf sink | `animatix-gui` `--perf-log` | **added** (PF-9, 2026-08-31; `crates/animatix-gui/src/app/perf_log.rs`) |
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
- **Profiling profiles (added 2026-08-31):** the workspace root `Cargo.toml`
  now sets `[profile.bench] debug = true` (so `perf record`/flamegraph resolve
  frames on bench binaries without changing optimization) and defines
  `[profile.profiling]` (inherits `release`, `debug = true`) for profiling the
  GUI/CLI via `cargo build --profile profiling`.
- **Measured tracer cost (2026-08-31, `cost_breakdown`):** the enabled tracer
  adds ~35 ns absolute on `frame_env_only` (~240 → ~276 ns, the finest seam)
  and is invisible within noise on `full_evaluate` (~940–1000 ns). Per frame
  this is one push/pop per stage — negligible against the 16.7 ms budget.
  Baselines are saved with the tracer on, so gate comparisons remain
  like-for-like.

---

## 8. Workflow for a performance task

1. Read this doc and `docs/architecture.md` (§3 runtime, §renderer).
2. Ensure a baseline exists: `bash scripts/perf-bench.sh save`.
3. Make the change. Re-run `bash scripts/perf-bench.sh compare` — it fails on
   regressions and prints the delta table.
4. If GPU/export affected, run the Layer-3 perf binary in `nix develop`.
5. Update `docs/roadmap.md`; commit via `cog commit perf "<summary>" <scope>`
   (scope `renderer` / `timeline` / `gui` as appropriate).
