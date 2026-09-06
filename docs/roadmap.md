# Animatix Roadmap

Canonical source of truth for remaining work. When a segment is fully done,
remove the completed items from this file.

---

## Completed Tracks

Historical tracks are kept as evidence. New implementation work is tracked in
[Backlog & Prioritization](#backlog--prioritization).

### Architecture Follow-Ups (2026-08-12)

| ID | Track | Status | Benefit | Feasibility | Necessity |
|----|-------|--------|---------|-------------|-----------|
| A | Scene-qualified selection and keyframe diff | Done 2026-08-12 | Prevent actors/keyframes from another scene preserving stale selections; makes rebuild behavior consistent across compositions | High | Medium-high |
| A1 | Scene-qualified keyframe source edits | Done 2026-08-13 | Keyframe insert/merge/delete/move/easing edits are scoped to the named scene body and no longer cross-scene match on actor/property/time | Medium | Low |
| B | Static subtree item collection cost | Done 2026-08-12 | Scene-only evaluation no longer collects/clones unused `SceneItem`s; static cache key now includes dimensions/collect_items | Medium | Low |

### Presenterm-Inspired Design Tracks (evaluated 2026-08-12)

| ID | Track | Status | Benefit | Feasibility | Necessity |
|----|-------|--------|---------|-------------|-----------|
| P1 | Render overlay / observable scene program | Done 2026-08-12 (structured op IR deferred) | Unify preview/export/offscreen paths; testable overlays | Medium | Medium-high |
| P2 | Hot-reload diff + preserve time/scene/selection | Done 2026-08-12 (property-precise keyframes) | Editing no longer disturbs current view; actionable removed-actor feedback | High | High |
| P3 | Command layer convergence (configurable keybindings, external command queue) | Done 2026-08-12 (app-owned registry) | Completes existing command architecture; presenterm key matcher patterns | High | Low-medium |
| P4 | Theme inheritance + raw/resolved runtime theme | Framework capability done 2026-08-12; GUI integration deferred | Theme deltas, dependency validation, full-closure hot reload | High | Medium |
| P5 | Unified asset store + usage tracking | Done 2026-08-12 (usage re-derived on rebuild) | Inspector asset usage and a clear rebuild lifecycle | Medium | Medium |
| P6 | Async file-backed asset loading | Closed by design 2026-08-12 | No current consumer; P5 fallback seam remains documented | High if scoped | Low |

### Dogfood Follow-Ups (2026-08-13)

| ID | Track | Status | Evidence | Next Action |
|----|-------|--------|----------|-------------|
| D1 | Indexed target source highlighting | Done 2026-08-13 | `dogfood/runs/002` pass 5: `card[i]` targets are uncolored while named targets are colored | Added tokenizer/AST label-base detection plus GUI regression tests |
| D2 | Rect default stroke asymmetry | Done 2026-08-13 | `dogfood/probes/007-rect-default-stroke-asymmetric-edge` | Filled shapes now default to no stroke; stroke-only actors keep a default outline and draw/reveal actions add a fill-colored outline for reveal effects |
| D3 | Structural container `unused-label` | Done 2026-08-13 | `dogfood/runs/002` authoring findings; sorting visualizer needs `lint-disable` | Built-in containers (`Row`/`Col`/`Grid`/`Stack`/`Group`/`Filter`/`Mask`) with children no longer trigger `unused-label`; empty containers and non-container actors still warn |
| D4 | Spec/runtime syntax drift | Done 2026-08-13 | Spec examples used `Circle`, square-bracket transform values, `duration:`, `Button`, and `gold`; parser/checker disagreed | Aligned spec examples with the implemented surface and registered `transform` as a known actor property with shorthand type support |

`D4` is documentation and checker/registry work, not a runtime language change.
The concrete drift: `Circle` is rejected, `transform` is expressed as a tuple
but omitted from the known-property registry, `duration:` is rejected on
actions while the modifier section calls it shared vocabulary, and `Button` /
`gold` are not built-in primitives/colors.

### Dogfood Content Backlog

| Content | Status | Blocked By | Next Step |
|---|---|---|---|
| Array/group `fade-in` target A/B run | Done 2026-08-14 | None | `dogfood/runs/003` accepted group-target `fade-in cards` as idiomatic; document container group-targets for entrance actions |

### Performance Evaluation Framework & Backlog

Design source of truth: `docs/performance_evaluation.md`. The framework is
layered (Criterion micro-suite → scenario suite → GPU/export + GUI telemetry)
and all performance work should be justified by a moved metric in that doc.

| ID | Track | Status | Next Step |
|----|-------|--------|-----------|
| PF-1 | `scripts/perf-bench.sh` baseline/regression harness | Done 2026-08-21 | Statistical (combined-std) regression gate over the full Criterion suite, run locally during optimization rounds |
| PF-2 | CI integration (`perf-report` job / persistent baselines) | Paused | Prove the harness in local optimization rounds first; re-enable CI only after the gate is stable and non-flaky |
| PF-3 | Persist/de-dup benchmark baselines across CI runs | Partly done | `perf-bench.sh` now accepts `PERF_BASELINE_DIR` for artifact-backed baselines and `perf-report.sh` emits a JSON ledger; wire artifact upload/download when PF-2 resumes |
| PF-4 | P1 frame-evaluation hot path (cache-hit restore clone, per-frame `Vec`/`SceneItem` churn, allocation in `encode_scene`) | Partly done | Frame-cache-hit no longer clones items/bounds/diagnostics — only the scene (commit `94873806`; `many_actors_cache_hit` 2408→1201ns). The redundant `Arc<vello::Scene>` copy in `FrameCacheEntry` was removed (2026-08-31: `evaluate_200_actors` −6.8%, `scrub_text_scene_100frames` −20%); the replaced cache entry's scene is now moved out as the next frame's encode buffer, and `invalidate_frame_cache` recycles the entry's scene into the buffer — miss frames deep-copy the scene once instead of twice (`many_actors_cache_hit` −4.4% more, `scrub_text_scene_100frames` −7.3% more, small no-cache leaves flat within noise). Stage-tracer evidence (`stage_breakdown` bench, 2026-08-31): a 60-actor dynamic miss frame is 53 µs, of which `sample` (per-node evaluation + scene encoding) is ≈47 µs (~88%) — the next hot-path target; `build_frame_env` ≈7.4 µs, `modifier_exec` ≈1.2 µs. Equation/Fragment frames no longer recompile Typst every frame: the `ChildProcessing::Equation` branch now goes through a process-wide grouped memo (`compile_typst_grouped_cached`, 2026-09-01), so a dynamic equation scene drops from 445 µs to 12.0 µs per frame (~37×); the new `benches/equation_frame.rs` gates it. Per-node allocations removed on the same path: the transform cache refreshes its slot in place instead of re-inserting an owned `String` key, `hit_regions` skips the label clone when picking was not requested, and the `actor_kind` metadata scan is only paid on the fallback path. Temporary sub-stage instrumentation inside `sample` (2026-09-01; note it adds ~8 µs of tracer overhead of its own) attributed ≈14 µs to property sampling, ≈7 µs to the transform-cache wrapper, ≈4 µs to primitive dispatch, ≈3.6 µs to scene encode, ≈1 µs to affine math — property sampling is the next target. Resolved child layout positions now use a
`HashMap` (`timeline::layout::LayoutPositions`) instead of a `BTreeMap`
(2026-09-02): every child paid an O(log n) string-comparison lookup per frame,
and a lesion that disabled the lookup entirely measured −4.9% on `sample`. The
hash map recovers all of it — `sample` −4.6%, `scrub_text_scene_100frames`
−5.1%, `scrub_many_actors_100frames` −3.5%, `reactive_evaluate_100frames`
−3.7%, full 64-bench gate 0 regressions; drift-corrected `sample` ≈ −3.4%,
reproduced across two adjacent A/Bs. Consumers only ever do point lookups, so
no ordered iteration was lost. Property reads now resolve once per process:
`property_registry::resolve_property` returns the schema *and* the runtime
plan-slot id from one table, where `effective_*` previously paid a binary
search over the sorted registry **plus** a separate hash into the id map — at
least five times per actor per frame (2026-09-02). `sample` −18.0%,
`eval_total` −14.7% (−20.6% / −16.7% once normalised against the untouched
`build_frame_env` control, which moved +3.7% *against* the change), reproduced
across three runs. `perf-bench.sh compare` flagged `modules_full`,
`full_50/100/200` and `evaluate_25_actors`; none reproduce under adjacent
high-power A/B (all ≤1.7%, and `evaluate_25_actors` is −1.7%) — those benches
drift 2–4% between runs against a 5% gate threshold, so **a full-suite FAIL
needs targeted re-measurement before it is believed**. `is_static_subtree` is
now memoized in `EvalCaches::static_subtree_flags`, cleared by
`invalidate_frame_cache` (2026-09-02): the uncached computation walks the whole
`PROPERTY_REGISTRY` per track (`has_any_keyframes`) plus the subtree, and the
frame path asked once per root *per frame* — lesioning the scan measured −72%
to −81% on the scrub benches, and the memo recovers all of it: 
`scrub_many_actors_100frames` −86.0%, `scrub_layout_scene_100frames` −82.5%,
`scrub_text_scene_100frames` −72.1%, `static_50/100/200_actors` −86/−88/−89%,
`visible_100_actors` −87.9%, `many_actors_evaluate_no_cache` −86.5%. The only
flagged cache-hit bench (`many_actors_cache_hit` +17.5%) does not reproduce as a
real cost: its sibling `many_actors_evaluate` runs the identical operation at
+1.1%, and `is_static_subtree` is unreachable on the frame-cache-hit path
(`restore_frame_cache` returns before it) — codegen/layout noise on the known
unstable pair. Measured and **rejected as not bottlenecks** (adjacent high-power A/B normalized against an untouched control
stage, 2026-09-01): (a) indexing `PrimitiveRegistry::find` — an O(1) hash
instead of a 31-entry linear scan with string compares, hit at least twice per
actor per frame — and (b) moving per-frame vector-path / procedural-plot
re-sampling behind the viewport check. Both landed within session drift. Do not
re-attempt without new evidence. Remaining: profile and gate `frame.*`/`scrub.*`; note that `many_actors_evaluate` and `many_actors_cache_hit` contradict each other run-to-run (both are cache-hit paths, yet they moved ±9–17% in opposite directions across identical-work A/Bs) — **resolved 2026-09-04: the "contradiction" was leak-driven memory pressure from the unbounded `bounds_key_pool` (see PF-6); the pair is reliable again after the cap, and `timeline_evaluate_1s/2s` dropped −43% once the hit path stopped paying pool-churn costs.** 2026-09-03: `EvalCaches::background_color` now samples the scene background once per frame instead of once per node (`scene_eval.rs` builds the primitive `EvaluateCtx` from the frame value; strictly removes N−1 redundant `PropertyTrack` samples of a frame-constant — correct by construction, but `stage/sample` drifts ±12% between runs so the exact gain is *not* gateable; kept per the "strictly removes work" precedent). Infrastructure findings that blocked a clean measurement split: (1) a per-node `encode_scene` `ScopedStage` was **rejected** — it adds ~2 µs to every `evaluate`-based bench and violates PF-8 §7's "a per-actor tracing event is not acceptable", so the seam stays reserved for a future collect-then-encode split (PF-7); (2) the `stage_breakdown` fixture cannot fire `layout` every frame because `layout_size` is build-seeded (an `always { size }` override does not re-seed it) and the animated child is `at`-excluded from layout admission — making it measurable needs a per-frame `layout_size` keyframe via new public API or a text scene that re-measures. 2026-09-03: scene-only misses now clone the per-frame `precise_bounds` table **once** instead of twice — the returned `SceneProgram` on the `evaluate()` path stays thin (`precise_bounds` empty, consistent with the existing cache-hit thin restore) and the bounds are stashed on the `FrameCacheEntry` so a later hit still restores `precise_bounds_cache` for callout tooling; the observable (`collect_items`) path is unchanged. `SceneProgram::precise_bounds` has exactly one production reader (`restore_frame_cache`), so no external consumer is affected; 739 lib tests pass, `stage/sample` −0.9% (within the noise floor, unchanged). 2026-09-03 (evidence-driven pass): temporary sub-stage probes (same throwaway method as the 2026-09-01 note, measured then reverted) found the real dominant cost inside `sample` was **NOT** property sampling — `evaluate_node_transform` ≈8.5 µs, primitive dispatch ≈4.3 µs, vello encode ≈2 µs, vector paths ≈0 — but **`compute_animated_layout` ≈14.6 µs/frame**, which sits entirely OUTSIDE the `layout` stage (the taffy compute it wraps is cache-skipped) and had therefore been invisible to every stage attribution. Root cause: the dynamic-layout cache hit path allocated ~3N+3 `String`s per container per frame to build a structured key (`child_extents` labels, an intermediate label `Vec`, `child_labels.to_vec()` re-clone, `container.to_string()`, `align`/`vertical_align` clones) plus a full `positions.clone()` of the N-key map on hit. Fix: per-container cache buckets keyed by label (bucket = the static identity — metadata, membership, order are build-time-fixed between invalidations, and every public mutable accessor funnels through `invalidate_frame_cache`), entries keyed only by the exact-Eq per-frame dynamic inputs (layout_size fingerprints + baselines — no hash collisions possible), `layout_children_for` memoized as an `Arc<Vec<ContainerLayoutChild>>`, and cached positions shared as `Arc<LayoutPositions>` (hit = refcount bump). Public signatures preserved via deref (`compute_animated_layout`/`compute_layout_for_time` now return `Arc<LayoutPositions>`). Measured: `stage/sample` 34.7→26.2 µs (−24%, reproduced 3×; `build_frame_env` control flat) — the largest single hot-path win recorded in PF-4. Also fixed: `cross_file_slot_fill_applies_at_any_depth` depended on an uncommitted scratch file under `/tmp/amxrepro/` (failed on every clean machine since `7ba09832`); the fixture is now an in-memory source. 2026-09-03 (attribution closed): a steady-state driver (`crates/animatix/examples/perf_driver.rs` — tight `evaluate` loop, settle phase, `perf record -e cycles:u` on a pinned core; Criterion profiles were unreliable because one-time setup + suite neighbours contaminate the capture and probe push/pops swamp sub-5 µs stages) ranked the remaining evaluate loop: string-keyed map machinery ≈30% (`tracks` BTreeMap, transform_cache, overrides, precise_bounds, env), allocator ≈18%, scene_eval inlined bodies ≈13%, `resolve_property` 3.6%, property-track sampling ≈6%, layout 2.6%, `evaluate_vector_paths` 1.4%, vello encode ≈2.8%. Two provable reductions landed: (a) `evaluate_vector_paths` returns early when `shape.vector_paths` is absent — exactly equivalent to the empty track's `default_value.clone()` and skips two throwaway `PropertyTrack` constructions per node per frame; (b) `evaluate_node_transform`'s three scalar reads (`rotation`/`scale`/`opacity`) use a process-once pre-resolved registry entry (`transform_property_reads`, `effective_f32_resolved`) instead of hashing the property name per node — override-first ordering preserved byte-for-byte. `stage/sample` 26.2 → 23.6 µs (−10%, reproduced 3×; cumulative 34.7 → 23.6 = −32% across the two rounds; `build_frame_env` control flat at 6.0 µs; `extension-bench.sh` absolute guardrail passes at 6.2 ns). Update 2026-09-06: `local_bounds` and env-key churn are DONE (PF-6 rounds 7/12); the hit-region/静态子树 coupling fix (38041c5a) also landed (see PF-6 round 12). Remaining: `tracks` BTreeMap→HashMap — deferred pending fresh evidence (see PF-6 remaining). 2026-09-03 (round 3): the pre-resolution completed for all five transform-path reads (`size`/`transform` joined `rotation`/`scale`/`opacity`; the now-dead `effective_f32`/`effective_vec2` wrappers removed), and the `precise_bounds_cache` label keys are pooled — the per-node insert pops from `bounds_key_pool` instead of allocating a fresh `String`, and all three map-clear/replace sites (frame-start clear, `invalidate_frame_cache`, cache-hit restore) drain keys back; the map is still emptied every frame so semantics are unchanged, only the allocations are reused. `stage/sample` 23.6 → 22.2 µs (−6%, reproduced 3×; cumulative 34.7 → 22.2 = −36% across three rounds; `build_frame_env` control flat at 5.9 µs; `resolve_property` no longer appears in the steady-state profile at all). `perf_driver` is now the documented steady-state attribution tool (see `performance_evaluation.md`). 2026-09-04 (PF-6 allocation profile): the new `alloc_driver` (dhat) measured the steady-state miss frame at 600 allocations / ≈500 KB churn with live ≈ 0 — and 86% of churn bytes was the `build_frame_env` overrides `reserve`, sized from `self.env.len()` although `with_base` shares the base layer and referenced-roots filtering skips 59 of 60 tracks (map holds 131 entries, was reserved for ~2600 → one ≈430 KB table allocation per frame). Right-sized to injected-actors × 120: a first ×35 attempt **regressed** `stage/build_frame_env` +60% because under-reserving triggers SipHash rehashes mid-injection — the multiplier must err generous (measured inserts per Rect track = 117), and a second attempt's extra `has_procedural_plots()` call (an O(tracks) scan) regressed `env_50`/`env_200` +13/+25% until it was computed once and reused in the fast-path condition. Final: churn 500 → 96.6 KB/frame (−81%), peak 443 → 39.8 KB (−91%), allocation count flat at 600; `stage/build_frame_env` flat vs the untouched baseline (6.13 → 6.22 µs), `stage/eval_total` −2.5% on the miss-frame spot check, and the full 64-bench gate **PASS with 0 regressions** (env_50 −8.0%) — the time gain itself is *not* gateable, kept per the strictly-removes-work precedent. The allocation lens re-ranks the remaining hot path deterministically (unlike the ±5–12% timing drift): env-key `String` churn (~208 blocks/frame across `Environment::set`/`env_keys`/`apply_override_incremental`), per-frame `KurboShape::to_path` bezpath rebuilds (~70 blocks, 23.5 KB) for frame-constant shapes, constant-track vector-path clones in `evaluate_vector_paths` (~61 blocks), dynamic-layout key/box residues (~60 blocks), and the `precise_bounds` table clone (1 block) — details in `performance_evaluation.md` §3.5. |
| PF-5 | P2 rebuild latency (font load, expand/typecheck, planner) | Partly done | System font DB shared process-wide (commit `5b12b015`); Text/Code/Typst compilations memoized process-wide keyed on all inputs (font-environment epoch guards staleness); build-time expression cache keys on an O(1) environment stamp; and `build_eval_env` now injects only actor labels referenced by the program (`build::referenced_roots` AST pre-scan), turning environment construction from O(declarations²) into O(declarations × referenced). `text_rebuild/mixed_48_warm`: 49.6ms→0.41ms (~120×); `components_full` −58%, `modules_full` −15%; lib test suite 58s→9s. Update 2026-09-06: `expand_components` is no longer a priority — `components_full` measures 2.64 ms and the heaviest build bench (`modules_full`) 4.98 ms, both far under the 16.7 ms frame budget; re-open only with a real generated scene exceeding ~10 ms build. `rebuild.*` gating remains deferred with PF-2/PF-3. |
| PF-6 | P3 allocation / memory profile (peak RSS, per-frame clones) | Partly done 2026-09-04 | `alloc_driver` (dhat, `examples/`) captures steady-state per-frame allocations on the 60-actor dynamic scenario — deterministic, immune to the documented timing drift. **Round 1**: 600 allocs / ≈500 KB churn per frame, frame-env reserve 86% of bytes (fixed, −81%). **Round 2**: vector-path Arc memo + shared empty layout maps → 420 blocks / 71.0 KB per frame (cumulative −86% bytes), `stage/sample` −8.3%; **and the instrument caught a real pre-existing leak** — `restore_frame_cache`'s `bounds_key_pool` grew one key per node per cache hit forever (`scene_costs` ballooned to ~21 GB RSS; fixed by `recycle_bounds_keys` cap at 512; same workload now flat at 0.12 MB live / 60 MB peak). The leak also explains the §4 "many_actors pair contradicts itself ±9–17%" note (leak-driven swap pressure) and the earlier misattribution of 22 GB suite spikes to rustc. `leak_probe.rs` = leak-vs-fragmentation diagnostic template. **Round 3 (same day): the frame env is pooled** — `evaluate_program_inner` returns the env to a one-slot `Timeline::env_pool`, `build_frame_env_internal` takes it back, `Environment::set` overwrites in place (`get_mut`-first + `set_owned`), `invalidate_frame_cache` drops the pool as the single invalidation funnel: 272 blocks / 41.8 KB per frame, `stage/build_frame_env` −15%, `evaluate_25/50_actors` −94/−92% in the gate. **Round 4 (same day): shape bezpaths shared + memoized** — `VelloPath.path: Arc<BezPath>`, `AnimationTrack::shape_path_memoized(&KurboShape)` reached via `RenderCtx.track`, thread-local scratch track for the build-time helper: 204 blocks / **18.2 KB per frame (cumulative −66% blocks / −96% bytes vs pre-PF-6)**, `stage/sample` 34.7 (round 1 start) → 18.0 µs. Remaining: DHAT/peak-RSS on real scenarios (GUI/export), string-keyed map structural work — ranked list in `performance_evaluation.md` §3.5. Round-4 gate flags (all build-path) failed isolation re-measurement except a +2.8% `simple_build_only` accepted as per-declaration memo overhead. **Round 5: `precise_bounds` Arc sharing measured and REJECTED** (−3.7 KB/frame churn but +3.3…8.2% time on `full_200`/`offscreen`; per-node Arc indirection in the render path costs more than the allocator saves — needs a slot-id design before re-attempting). **Round 6 (2026-09-05): the slot-id design landed** — `precise_bounds_cache` + `bounds_key_pool` are gone: `AnimationTrack::bounds_slot` (`Cell<u32>`, stamped from a lazily rebuilt sorted-label registry cleared by `invalidate_frame_cache`), a dense `BoundsTable { slots, written }`, flat `(slot, rect)` stash/restore on the frame cache, and one-pass label-map materialization on the observable path only. Adjacent A/B: **204.2 → 168.2 blocks (−17.6%) / 18,233 → 15,832 B (−13.2%) per frame, perf_driver +5.1% (3× reproduced)**; gate wins `timeline_evaluate_*` −50…−53%, `scrub_layout_scene_100frames` −47%, `static_*_actors` −9…−13%. All 7 gate flags dispositioned by isolation: 5 did not reproduce (build-path/sub-ns, §4 contamination), `sample_all_tracks` +4.3% isolated (one extra `Cell` per track; below gate), `static_50_actors_with_items` +12–15% accepted (tooling-only path re-derives the public label map via the slot table; its scene-only twin improved −12.9%; GUI/export never request items). Do NOT stash pairs on the observable path too — building both representations per miss measured +24%. **Round 7 (2026-09-05): one shared key buffer across the frame-env injection chain** — `env_keys::property_into` + `&mut String` threading + a thread-local buffer in `apply_override_incremental` kill every per-frame `format!` env key; on the pooled env the steady-state frame performs zero key allocations: **168.2 → 100.2 blocks (−40%) / 15,832 → 14,580 B per frame, perf_driver +4.4% (3× reproduced)**; gate flags all failed isolation (offscreen/parse benches measured change-faster-or-flat in adjacent A/Bs). **Round 8 (2026-09-05): per-track shape-command memo** — the owned `Vec<RenderCommand>` from `evaluate_shape_render` (extension ABI, signature unchangeable) is memoized on the track keyed by `(vector_paths_epoch, style, state)` with a take/encode/recycle borrow protocol; correctness rests on the audited pure-function property of all six shape primitives' `render()` (anchor refs and overrides are folded into state before the call; `PartialEq` on the state enums pins that contract). **100.2 → 32.2 blocks (−68%) / 14,580 → 9,948 B per frame**, perf_driver +0.8%; the memo is `Box`ed on the track (inline placement regressed `env_200` +4% via cache pressure — fixed, `env_200` now 765–766 ns vs baseline 767–777). All 4 gate flags failed adjacent A/B isolation (offscreen measured change-faster). Cumulative vs pre-PF-6: **−94.6% blocks / −98.0% bytes** (600 blocks / 500 KB → 32.2 / 9.9 KB per frame). **Round 9 (2026-09-05): export path profiled for the first time** — new `export_alloc_driver` (real `.amx` through `OffscreenRenderer`); 93% of the export frame's 3.97 MB churn was the per-frame CPU readback allocation. `RenderedFrame.rgba` is now `Arc<Vec<u8>>` with the renderer parking/reusing the buffer (held frames force fresh allocations — reuse is never correctness-load-bearing; two backend regression tests pin pixels and the park protocol): **3.97 MB → 282 KB per frame (−93%), peak 5.9 → 2.3 MB** on `dashboard_story`. **Round 10 (2026-09-05): plot-closure evaluation chain** — `fft_explain`'s 533 KB/frame was NOT text but the per-sample capture cycle (`merge_missing_into` deep-cloned a captured FFT list per sample point just to test presence). Presence test now borrows, `Value::List` is `Arc<[Value]>` (all list clones are refcount bumps), sampler caches pre-sized: **533.6 → 425.0 KB/frame (−20%), blocks −27%**; accepted cost `simple_build_only` +2.8…3% isolated (extra `Arc` allocation per list construction; build is keystroke latency). **Round 13 (2026-09-06): `tracks` BTreeMap→HashMap resolved by its own evidence gate** — a fresh `perf_driver` profile (374k samples) attributed 5.68% to BTreeMap navigation, of which ~1.3% is vello-internal; the one above-threshold consumer is the layout fingerprint pass (`compute_animated_layout` → per-child `tracks.get`, 1.85%). Fixed WITHOUT the storage refactor: keyframe-free containers (the common case) cache their result in a static slot and skip the fingerprint pass entirely (`0fadf25e`) — perf_driver +1.5% (3× reproduced), `stage/sample` 17.2 µs. The full BTreeMap→HashMap refactor stays closed: the per-node `tracks.get` does not rank top-10. Remaining: GUI `--perf-log` live-session capture (landed 2026-09-06, see PF-9/round-13 note below) |
| PF-7 | P4 GPU / export throughput (raster ms, video/GIF encode FPS) | Baseline landed 2026-09-05 | `export_perf_driver.rs` = the Layer-3 binary (real `.amx`, stage tracer drained per frame). Baseline @720p: 143–413 fps depending on scene; **readback wait is the largest component (~2–3.8 ms/frame, 50–60%)** — `readback_output` blocks on `device.poll(Wait)` for all queued GPU work; `rasterize` 0.26–1.4 ms; `sample` 0.3–2.8 ms. **Pipelining landed 2026-09-06 (`d063df86`)**: `begin_frame_with_debug`/`begin_transition` queue the readback copy without blocking; `wait_frame` polls index-scoped; the streaming pipelines begin N+1 before waiting on N. Measured `dashboard_story` @720p: **169–234 → 501–554 fps (2.4–3×)**, wait+copy 1.3 ms/frame; pipelined-vs-blocking pixel-identity regression test added. Remaining PF-7 surface is wgpu-internal. Machine swings ±40% run-to-run (software rasterizer) — read trends, not absolutes |
| PF-8 | Shared stage tracing (`crates/animatix/src/perf.rs`, `ScopedStage`) so benches + GUI HUD measure the same stages | Done 2026-08-31 | Thread-local ring/ledger tracer behind the default-on `perf-tracing` feature; instrumented `rebuild`, `build_frame_env`, `sample`, `layout`, `modifier_exec`, `rasterize`; `encode_scene`/`export` seams reserved for PF-7. No-op stubs keep the CI `--no-default-features` build identical |
| PF-9 | GUI JSONL perf sink (`--perf-log`) from `PerformanceMetrics` | Done 2026-08-31; live-session capture validated 2026-09-06 | `PerfLogSink` (`crates/animatix-gui/src/app/perf_log.rs`): one JSON line per frame with `ts/fps/rebuild_ms/render_ms/stale/actors/scene_size` plus a `stages` map drained from `animatix::perf::take_measurements()` (PF-8 stage names); self-disables after first I/O error; covered by unit tests. **Live-session capture (2026-09-06, `--demo-script` + `--perf-log`)**: scripted play/pause/5-seek session on `dashboard_story` captured per-frame evidence — `rebuild_ms` 0.0 across all frames (seek/playback never rebuilds), evaluate (`sample`) p50 1.78 ms with no scrub spikes (the 220 ms outlier is a composition scene transition's first render), `stale` always false. The capture ran at ~1 Hz because the occluded Wayland window gets compositor-throttled — paint cadence is an environment artifact; the per-frame data is real. The `--demo-script` flag scripts playback commands through the external command queue for repeatable sessions |

---

## Known Issues (2026-08-26)

| Issue | Detail | Next Step |
|---|---|---|
| BarChart `gap` registry visibility (unverified) | An early-session note flagged the BarChart `gap` property's registry visibility as a known issue; status unknown after the subsequent build refactor | **Verified 2026-08-26**: `gap` is parsed by the shared plot props loop (handles `auto` or numeric), and BarChart delegates to `process_plot_actor_dispatch` — the only signal is an info-level `unknown-property` ("may still be valid"), same class as theme_studio. No fix needed. |
| Track `parent` back-reference is not always back-filled | First-declaration children inside containers can lack the parent pointer (noted during the component Group fix), so parent-chain queries cannot trust the stored field. The children lists ARE authoritative (regression-tested) | **Done 2026-08-26**: `Timeline::parent_of()` (derives child→parent from the children lists) added in `crates/animatix/src/timeline/mod.rs`; the never-revealed diagnostic routes its query through it. |
| ~~`descent_graph` cross-scene modifier warning~~ | ~~Symptom of the graph.map bug: 06_reactive/gradient_descent's `descent_graph.map` call couldn't resolve the receiver and the modifier IR logged `Undefined variable: descent_graph`.~~ | **Resolved by the dotted-NativeFn IR fix (env_keys module + lower.rs CallEnv join) — verified: 0 warnings at t=14 in gradient_descent.** |
| `hidden_by_default` flag goes STALE when reveals bypass lift_hidden_by_default | **Root cause found (probe, 2026-08-26)**: 06_reactive's title/ring carry staggered fade keyframes `[(0,0),(500,0),(1000,1)]` (authored fade-ins beyond the file's 40th line) yet the flag stays `true` — the fade keyframes were added without routing through `lift_hidden_by_default`, so the flag is a stale "never revealed" signal. The SOUND signal is keyframe-based: warn only when the opacity keyframes are all zero AND no ancestor's opacity lifts (parent chain derived from children lists) AND the actor is not a generated sub-actor. The earlier attempt's false positives were generated sub-actors (ticks/labels) whose visibility inherits from parents | **Resolved 2026-08-26**: the diagnostic was re-implemented on the keyframe + parent-chain + generated-sub-actor-exclusion model (`151c02f5`); `Timeline::parent_of()` supplies the parent chain and `FadeIn::execute` now routes its reveal through `lift_hidden_by_default`. Verified against the 42-example corpus. |
| GPU `Filter` `blur`/color effects are not visibly applied in `animatix image` export | A `Filter` with `blur: 10` over a high-contrast checkerboard stayed sharp. Root-caused 2026-08-31: back-to-back compute passes sharing ping-pong textures in one encoder did not synchronize (a color-matrix control proved the machinery works; a two-pass blur returned the untouched copy until split). | **Fixed 2026-08-31**: each blur/color-matrix pass is now submitted in its own encoder (submit boundary = sync point); `gpu_filter_blur_softens_a_hard_boundary` + `color_matrix_actually_desaturates` are the regression guards (backend, content-level); pixel-verified (soft gradients on the checkerboard). See `dogfood/probes/009-filter-gpu-deferred`. The Filter scene-eval silent fallback is still worth surfacing as a diagnostic (separate follow-up). |
| Typst math with implicit multi-letter coefficients fails to compile | `Typst, content: "$mc^2$"` / `"$E = mc^2$"` error because Typst parses `mc` as a single multi-letter *variable*, not `m*c` (a Typst math gotcha). This is correct Typst semantics, not a bug. | **Resolved 2026-08-28**: the compile error now surfaces Typst's real message ("unknown variable: mc" + its hints) instead of an opaque "failed to compile Typst document". For a multi-letter product write `$m c^2$` or `$"mc"$`. |
| Container `fade-in` never reveals Graph-hosted PlotCurve children (2026-09-06, **silent**) — **Resolved 2026-09-06** (`26b4ca46`) | Pre-keyframe declarations are hidden by default; `fade-in <graph>` lifts the container but did not cascade into Graph children, so the hosted curve stayed at seeded opacity 0 forever with **no diagnostic**. Pixel-verified on `examples/data/07_plots.amx` (t=3.0/6.0): its headline sine was invisible the whole scene. Twin quirk: explicit `opacity: 0` on the child bypassed hidden-by-default | `lift_hidden_by_default_subtree` cascades into children on container entrance actions; the `never-revealed` ancestor check now requires a genuine 0→positive ramp (declaration-time constants no longer suppress it) with generated tick/bar labels exempt. Fixed `07_plots.amx` (headline sine back) and surfaced `23_plot_kinds.amx` (fixed with a container fade-in). Tests: `container_fadein_reveals_graph_hosted_children`, `unrevealed_graph_child_still_warns_never_revealed`; probe 010 resolved |
| spec §14 "Runtime parameters" plot pattern never re-samples (2026-09-06) — **Resolved 2026-09-06** (`291ae9ff`) | `let freq = 2` + `always { freq = ... }` + `func: (x) => sin(freq * x)` rendered **static** (pixel-verified identical at t=0.3/5.0). `ProceduralPlot::is_dynamic()` = `references_ident("t") \|\| !param_names.is_empty()`, so capture-only plots reused cached build-time `vector_paths` and the frame-env shadowing path (scene_eval.rs:522+) was unreachable | `Timeline::collect_frame_written_vars` scans lowered always-blocks (bare assignments + `let`s, walking if/for) into `frame_written_vars`; `is_dynamic` also fires when `extra_captures` intersect that set. Spec §14 pattern now animates as written (probe 011 resolved). Test: `plot_capture_of_always_written_var_is_dynamic` |
| High-frequency plot curves under-sample to a straight line (2026-09-06) — **Resolved 2026-09-06** (`aab8ced9`) | Initially attributed to timed plot-param assignment ("wrong curve"), but a plain `func: (x) => sin(5 * x)` reproduced it: the adaptive samplers' subdivision floor was 3 levels (8 samples), nearly collinear for ~16-period functions — the param-track machinery was innocent (probe 012's corrected diagnosis). Timed `curve.freq = 5 [1s]` itself works | `sample_recursive_cartesian`/`_polar`/`_parametric` take a `min_depth` derived from the plot's `resolution` (`resolution.max(8).min(max_depth).ilog2() + 1`); adaptive subdivision continues beyond the floor. Pixel-verified sin(5x) and the probe both render the full wave. Test: `high_frequency_curve_meets_resolution_floor` |
| Unlabeled actor inside a Graph fails the build on an engine-generated name (2026-09-06) | `__anon_sw_graph_1` trips `error[build:reserved-label-prefix]` — the generator's own output violates the reserved rule. Labeled + `// lint-disable: unused-label` is the working spelling; spec §8 now documents the requirement | Exempt engine-generated anon labels from the reserved-prefix check (or generate non-reserved names); documented as required labels in spec §8 for now — open |

### Typst surface fixes (2026-08-27)

The Typst rendering-correctness work landed in four small commits (see the
render-correctness probe `dogfood/probes/008-render-correctness` for the
visual evidence):

- **Uniform `text:` content property** for `Text`/`Code`/`Typst` (was silently
  blank for `Typst, text: "..."`).
- **Bold/italic/weight render** for system fonts: `load_font_emphasis_faces`
  loads regular + bold + italic + bold-italic per family (the Typst world
  previously loaded only one regular face, so emphasis fell back to regular).
- **Default font made full-featured (2026-08-28)**: the single-weight mock
  "Open Sans" was replaced with four real static faces (Regular/Bold/Italic/
  BoldItalic, Apache-2.0) vendored under `crates/animatix/assets/fonts/` with
  SHA-256 provenance and `scripts/refresh-fonts.sh` integrity checks.
  `DEFAULT_FONT_FAMILY` stays "Open Sans", so bold/italic/font_weight now work
  with the default family (no `font_family` needed). Static faces are used
  rather than upstream variable fonts because typst 0.14 does not consume
  variable axes (that landed in typst 0.15).
- **First-class `Math` primitive** (`Math, text: "x^2 + y^2"` compiles Typst
  math without the `$...$` wrapper), registered in the primitive registry,
  analyzer built-in types, and schema; the deprecated `Math`→`Typst` remap was
  removed.
- Also register `Math` in `schema.rs` / `builtins::TYPES` so the analyzer no
  longer flags it as `unknown-type`. No change to the `Text`/`Code` fast paths.

## Language Capability Gaps (2026-09-06, taylor-sin dogfood)

Found while authoring `dogfood/projects/taylor-sin` (render-verified; full
evidence, repro snippets, and workarounds in its `notes.md`). Ranked by
authoring impact on STEM-explainer content. The engine-side defects found by
the same pass are rows in the Known Issues table above.

| ID | Gap | Evidence / Workaround today | Fix direction |
|----|-----|------------------------------|----------------|
| LG-1 | No series construction in expressions: no `sum`-style folder, no `factorial`, no `let` inside closures (parse error), no pure-fn calls from plot closures | Taylor Sₙ(x) needs 7 hand-expanded terms gated by `step()`, with the degree knob inlined 6× into the closure body | Smallest step: `sum(...)`/`factorial(...)` builtins registered in **both** eval paths (eval_shared + IR `BuiltinFn`, per the single-eval-paths pitfall); then closure-local `let`; then user-fn calls via the closure capture mechanism. Needs a short design note before implementation (closure-arg builtins are new territory for the IR) |
| ~~LG-2~~ | ~~`format()` has no precision/width specifiers~~ | **Done 2026-09-06** (`606f8fd7`): `{:.N}` supported in `eval_format` — the single implementation behind both the build-time and IR paths; unrecognized specs stay literal, surplus `{}` keep braces | — |
| ~~LG-3~~ | ~~No reliable frame-driven "knob" vocabulary for plot closures~~ | **Done 2026-09-06** (Stage B, `291ae9ff`): the spec §14 pattern now animates; inline-`t` remains a valid alternative spelling | — |
| ~~LG-4~~ | ~~Modifier keys are not validated against the action's declared signature~~ | **Done 2026-09-06**: `execute_action` validates named modifiers against `ActionSignature.modifiers` and warns `UnsupportedModifierKey` on undeclared keys; `highlight [intensity]` call sites removed from gradient_descent.amx | — |

## Resolved Open Questions (2026-08-26)

The language-revision candidates from the 2026-08 systems review are now
**resolved** (no further decision needed). Booked where they landed:

- **Theme dual-import — closed (option (a))**: the idiom (unaliased import
  registers the colorscheme + aliased import exposes tokens) is the documented
  final choice; `spec.md` documents it. The premise for (b) ("make the unaliased
  import expose tokens directly") was tested and is **FALSE** (2026-08-26): with
  only the unaliased import, `theme.text_lg` does not resolve (falls back with an
  unknown-lookup-path warning). The aliased line is required for token access, so
  the two-import idiom stands.
- **Grid auto-columns — closed (keep the nudge, defer auto-fit)**: a `Grid`
  without `cols` is single-column; auto-fitting columns from child sizes would
  remove a foot-gun. A `missing-grid-cols` build warning now fires when `cols` is
  absent (`crates/animatix/src/primitives/grid.rs`). A corpus census (2026-08-26)
  shows every real `Grid` in `examples/` already sets `cols` (hit rate ≈ 0), so
  the nudge is retained and full auto-fit is **deferred** until a concrete
  foot-gun appears.
- **Comment Directives — closed as a recommendation**: presenterm-style HTML
  comment directives are the wrong mechanism for Animatix, which owns a
  semantic DSL. Valuable commands should map to native `.amx` features; add
  first-class metadata (speaker notes, export presets) only when a concrete user
  story appears.

---

## Audit History

| Item | Resolution |
|------|------------|
| Semantic AST single source | Done. `parse_canonical` is the Chumsky semantic source; analyzer uses the lossless token stream plus AST for positions/completions. |
| Semantic index single source | Done for declarations. `animatix-syntax::builtins` is the single registry; parser records declaration/action-target/play-scene occurrences; `Analyzer` uses them for positions; LSP emits UTF-16 semantic-token columns; `_` and import aliases have roles. Remaining scope-resolution and reference-occurrence items are in [Backlog & Prioritization](#backlog--prioritization). |
| Module/Workspace resolver unification | Done. `Workspace` is now a thin facade over `ModuleGraph` in `SourcesOnly` mode; parsing, symbols, import identity, and namespace resolution are single-source. LSP continues to use per-document `Analyzer` for CST/positions while workspace symbols come from the shared graph. |
| Semantic diagnostics single emitter | Done. `animatix-syntax::semantic_diagnostics` is the canonical emitter; analyzer and LSP convert DTOs instead of re-implementing checks. |
| Path/source-map model | Done. `animatix-syntax::module::source_map` owns normalized path identity, import resolution, and in-memory source overrides. |
| Source override lifecycle | Done. `ModuleGraph::with_source` scopes temporary overrides and restores/removes them on both success and error. `upsert_source` invalidates the changed file and its dependents. |
| GUI mutation/cache/snapshot convergence | Done for the core path. `commit_source`/`replace_text` invalidate caches, and `DocumentStore::with_mutation` scopes snapshot finalize/abort. Remaining handlers can migrate opportunistically. |
| Rebuild worker lifecycle | Done. `RebuildWorker::submit` restarts a dead worker thread. |
| Type model vs annotation grammar | Done. User-facing annotations support `Vec3`, `Tuple<T, U, ...>`, and `Fn(T, U) => R`; `Type::to_annotation` no longer degrades these to `Any`, and tuple/function subtyping, nested alias resolution, closure/call return inference, completion, parser equivalence, and typechecker tests cover the surface. |
| Parser-sync AST equivalence | Done for current syntax. Corpus-level equivalence covers actions, keyframes, scenes, modifiers, shorthand, for loops, reactive bindings, sequence/stagger, component/action definitions, method/if expressions, parameter defaults, match forms, pub/import declarations, multi-scene composition, inline children/for/slots, nested paths, complex patterns, closures, object construction, logical operators, and operator precedence. Expand coverage as new syntax lands. |
| Code style/maintainability pass | Done. Removed production `expect`/`unwrap` panics in frame-cache and LSP URI paths, fixed clippy warnings, moved misplaced keyframe handler tests, and consolidated duplicate keyframe property enumeration into `timeline_diff::collect_actor_keyframes`. |
| Dogfood A/B review demo | Done. `animatix-gui --review dogfood/runs/<slug>` provides Single and Compare review modes, shared-time live preview, read-only highlighted source, diagnostics, and comments persisted to `review.json`; `review.done` and `scripts/dogfood-review.sh` define the agent launch/wait/handoff loop. Run directories stay local and gitignored. Static questionnaire/arena and proposed-syntax review remain deferred until an external-reviewer need appears. |
| Dogfood review hardening | Done. Review passes fixed Compare mode (per-variant columns, render-before-layout, and console click timing), removed misleading comment line anchors, made comment timestamps opt-in, removed manual severity selection, fixed explicit `opacity` on pre-keyframe actor declarations (`probes/006-explicit-opacity-before-keyframe`), added playback speed presets, and consolidated interactive controls into the bottom review console. |
| Dogfood workflow docs | Done 2026-08-13. `dogfood/README.md`, `dogfood/runs/README.md`, and the run/review templates now distinguish projects/probes/runs, document `dogfood-review.sh`, and state that comments are anchored to variant + optional time. |
| Dogfood indexed target highlighting | Done 2026-08-13. Action targets like `fade-in card[0]` and assignment targets like `card[0].scale` now highlight the actor base as a label; GUI regression tests cover indexed targets without turning ordinary index expressions into labels. |
| Dogfood spec/runtime drift | Done 2026-08-13. Spec examples now use implemented actors/colors/modifiers, `transform` is a known actor property accepting 2/4/6-element tuples, and analyzer/runtime regression tests cover the corrected examples. |
| Dogfood filled-shape default stroke | Done 2026-08-13. Filled shapes default to no stroke so plain `Rect`/`Ellipse` renders are clean; `Line`/`Arrow`/`Callout` retain a visible default, and `draw-in`/`reveal-in` add a fill-colored outline only when needed. |
| Dogfood structural container lint | Done 2026-08-13. Built-in containers with children are exempt from `unused-label`, matching their structural use; empty containers and non-container actors still report unused labels. |
| Dogfood sorting visualizer componentization | Done 2026-08-13. Steps and Result scenes use a reusable `Bars` component; component expansion now recurses into scene bodies and callout targets accept namespaced indexed references. |
| Dogfood group entrance A/B | Done 2026-08-14. `fade-in cards [500ms]` on a generated container renders identically to enumerating `card[0..4]`; the group-target form was accepted as idiomatic. |
| Open backlog docs/BarChart pass | Done. BarChart docs now use brace-list `data`/`bar_colors` and document scheme tokens; `graph.map`/`map_inverse` and `_animating_*` docs match implementation; eparts Button theme-slot/variant docs match shipped variants. |
| Open backlog BarChart runtime pass | Done. `bar_colors` registry is build-time-only, `show_labels` renders child Text labels, and `bar_width`/`gap`/`max_value` reject non-numeric values with diagnostics. |
| `always` bare variable assignment | Done. `always { freq = ... }` lowers to a frame-local variable write; plot sampling lets frame values shadow build-time closure captures without leaking captures between plot actors. |
| Open backlog build target | Done by formal decision. Bare `cargo check -p animatix --no-default-features` is intentionally unsupported; README, AGENTS, Cargo.toml, and CI now document `--no-default-features --features render,text,svg` as the supported no-video combination. |
| Gradient-descent example consistency | Done. Descent and learning-rate trails now follow constant-angle radial paths, matching the `x² + y²` loss surface and its radial gradient. |
| Review static/discovery tooling | Done. `scripts/review-report.sh` generates a self-contained HTML questionnaire/arena from a review run and accepts `.proposed`/`.amx.proposed` source-only variants; `scripts/review-discover.sh` emits an agent worklist from all local runs. |
| GUI/theme/commands/assets/callout pass | Done. eparts ColorPicker/TabBar/Alert/Badge/Tag/GroupBox/Tooltip are adopted at natural call sites; GUI gets an external command queue, asset cache preservation/invalidation, and callout guide/edge snapping. |
| Language intelligence/syntax pass | Done. Parser occurrences now include assignment/reactive targets, properties, calls/methods/constructors, and closure parameters with lexical scope ids; analyzer `find_references_at` resolves shadowing, and GUI/LSP semantic tokens consume parser occurrences. |
| Plot/Text transition pass | Done. VectorField/Heatmap/ContourSet support func transitions, `[blend: opacity]` adds opacity cross-fades, and timed Text/Typst content assignments cross-fade glyph paths. |
| Export presets | Done. Named `ExportPreset` values are shared by CLI and GUI; `config { export_preset: "1080p30" }` is honored by CLI video/GIF export. |
| Speaker-notes metadata | Closed by design for now. No concrete presentation/export consumer exists; per the roadmap's metadata policy, first-class notes should be added when that user story appears. |
| AI review evaluator | Design retained in `docs/ai_agent_animation_quality.md`. Implementation would be a new review crate/rule engine/agent loop and remains unscheduled until a product milestone pulls it forward. |
| Complete extension surface | Done. Transactional plugin lifecycle, shared descriptors/types, full manifests, unstable native ABI snapshot 5, capability-based runtime dispatch, GUI/LSP/analyzer integration, native render command completeness, docs, and workspace gates are implemented and committed phase-by-phase. |
| Plugin lifecycle pass | Done 2026-08-19. `GL-01`..`GL-05`: `DocumentPluginManager` owns explicit/document/workspace discovery, atomic last-known-good swaps, plugin error toasts, manual reload, and change polling; the background rebuild worker reuses the shared extension context and rejects stale plugin-epoch rebuilds; extension actions appear in the insertion palette. |
| GUI plugin UX pass | Done 2026-08-19. `GUI-01`..`GUI-07`: plugin status panel with manifests/libraries/capabilities/errors/reload/authoring, shared analyzer discovery reused by LSP, workspace-level priority discovery, manifest-driven Bool/Color/Vec2/Enum/Text editors, in-process fake-plugin test seam, and capability badges. Explicit plugin paths persist in workspace settings. |
| Native ABI/runtime polish pass | Done 2026-08-19. `EXT-01`..`EXT-07`: explicit uncached native image URLs fail instead of falling back, `append_text` supports Text/Code/Typst, `Type::Enum(...)` powers manifest enum editors, `declared_property_names`/`declares_property` replace repeated `Vec<String>` contains, `PrimitiveFamilyDescriptor` classifies any runtime primitive, plot hosting uses a capability, recursive container expansion is unified, asset URLs normalize against document/workspace paths, and `PluginLoader` exposes list/replace/remove APIs. |
| Plugin maintainability pass | Done 2026-08-19. `Type::Enum` round-trips through `TypeAnnotation`, built-in capability defaults use one schema table, CLI/GUI share `animatix-plugin-tooling` manifest generation, failed plugin reloads keep a consistent active snapshot, disposer semantics are explicit, and status/insertion/editor paths gained direct unit tests. |
| Plugin extension fix pass | Done. Enum-typed extension properties accept bare variant identifiers (`mode: ring`) and now round-trip to the native `NATIVE_VALUE_ENUM` runtime value instead of being silently dropped; registering the same property name on multiple actor types is rejected instead of silently cross-writing; the analyzer's common-property list is explicit (`common_property_names`) and drift-pinned; native `write_keyframe`/assignment report `UNSUPPORTED` for built-in properties and fall through to the generic engine; ABI version doc synced to 6; pre-existing clippy warnings cleaned up. Verified via 2 new regression tests plus CLI/plugin-describe round-trips. |
| Primitive abstraction/integration pass | Done (audit follow-up). `evaluate()` `None` vs `Some(vec![])` semantics are now explicit and pinned by tests (empty-content actors draw nothing and record no hit region/bounds; container shells stay pickable at their layout box); dead shape trait methods (`supports_fill`/`uses_custom_path`/`exposes_tip_size`) removed in favor of the `ShapeType` free functions; `RenderCommand` is `#[non_exhaustive]` with an explicit extension boundary; default text font sizes centralized in `renderer::text::default_font_size`; GUI now resolves extension primitives through the live registry (default props, resize mode, nestable-container and group detection, icons); `Math` gained full schema property coverage (analyzer no longer flags `Math, text: ...`); stale primitive/render docs rewritten (real touch-point checklist); remaining `ty == "..."` string dispatches in build/media/plot replaced with kind checks; `PrimitiveFamilyDescriptor` passes capabilities through unchanged (with schema-category fallbacks); registry storage enum-ified (no `BuiltinPrimitive` forwarding boilerplate); scene-eval child-processing dispatch hoisted and documented; native plugin ABI gained optional `default_props`/`default_color_key` callbacks (ABI snapshot 7) so extensions get GUI defaults and colorscheme defaults. |

### eparts Framework Expansion (closed)

The committed framework track is closed: high-value items were delivered and the remainder was archived
rather than kept as indefinitely-open deliverables.

Delivered:
- B7 JSON themes + schema (`theme-json` feature)
- B8 theme hot-reload (`theme-json` feature)
- A5 StyledExt `Ui`/`Response` helpers
- K3 gallery example
- K6 cross-platform CI feature matrix

Archived:
- B9/B10, C7–C15, D6–D9, F6–F10, G9–G11, H4/H5, J4, K4/K5/K7–K11

Archived items have no current consumer and should be re-opened only when a
concrete second-app need exists.

### GUI Follow-Ups (closed)

| Item | Resolution |
|------|------------|
| Opportunistic eparts widget adoption | Done for the remaining high-value call site in this pass: timeline action blocks now use the eparts `text_tooltip` helper. Additional call sites can continue migrating as their surrounding GUI areas are next edited. |

### Language and Runtime Gaps (closed)

| Item | Resolution |
|------|------------|
| Precise shape/path/text bounds | Done for the supported path. The renderer now caches exact world-space AABBs from emitted commands, restores them on frame-cache hits, and `TargetResolver::target_bounds` prefers them for callouts/lines/arrows. Debug overlays also include evaluated text paths. Size-box bounds remain the fallback for actors not evaluated this frame. |
| Text/Typst/Code frame-time content overrides | Done for the supported path. `always` text/content overrides recompile glyphs per frame, explicit empty strings clear stale glyphs, and primitive render errors are surfaced as runtime diagnostics. Frame-time overrides do not remeasure layout size; that remains a documented limitation. |
| Unified `fn` mechanism (P6) | Done 2026-08-20. `action` keyword removed: timeline functions (`fn` without `-> Type`, implicit `self`, block-scoped expansion, nested calls with cycle guard) and pure functions (`fn ... -> Type`, evaluated at build time, callable from expressions and `always`) share one construct. Purity checker rejects timeline ops in pure bodies. Pure-function **tail expressions** (Rust style) and **frame-time calls from `always`** completed 2026-08-20; `pub fn` cross-file imports verified. Demos (`sort_colors` DNF, sorting-visualizer) refactored onto the mechanism. `action` keyword removed: timeline functions (`fn` without `-> Type`, implicit `self`, block-scoped expansion, nested calls with cycle guard) and pure functions (`fn ... -> Type`, evaluated at build time, callable from expressions) share one construct. Purity checker rejects timeline ops in pure bodies and user-fn calls in `always`. Demos (`sort_colors` DNF, sorting-visualizer) refactored onto the mechanism.
| Data-dependent algorithm timelines | Done 2026-08-20. Runtime mutable state stays out of scope to preserve the random-access guarantee; the build-time path now covers the full authoring loop: `let` shadowing + `list_swap`/`list_set` + `if`/`match` precompute the algorithm, **leaf expression-indexed targets** (`swap bars[j], bars[j+1]`) resolve against the build environment, and a `[step: ...]` for-loop modifier sequences the emitted events onto distinct keyframe times. Rewrote `examples/projects/leetcode_sort_colors.amx` (Dutch National Flag) and `dogfood/projects/sorting-visualizer/entry.amx` (insertion sort) to be fully algorithm-driven. |

---

## Completed Backlog (2026-08-19)

All previously open extension/plugin backlog items are done. The remaining
source of truth for implementation details is
[Audit History](#audit-history).

- Plugin lifecycle and GUI runtime integration: `GL-01` through `GL-05`
- GUI plugin UX and discovery: `GUI-01` through `GUI-07`
- Native ABI and runtime polish: `EXT-01` through `EXT-07`

---

## Backlog & Prioritization

### Demo Gallery Redesign (active)

Source of truth: `docs/demo_gallery_plan.md`. Work happens on a short-lived git
worktree off `main` (e.g. `feat/demo-gallery`) and is merged back when a phase
lands.

| Phase | Deliverable | Status | Acceptance | Known Blockers / Notes |
|---|---|---|---|---|
| 1 | Shared `lib/` design system + `theme_studio.amx` | **Done** | clean `check`; PNG render smoke | Engine workarounds documented in plan: wrap positioned components in `Group`; wrap Text in `Group` inside Col |
| 2 | `motion_poster.amx` + `dashboard_story.amx` | **Done** 2026-08-24 (merged) | clean `check`; PNG smoke of every scene | Engine fixes landed with it — see `docs/handoff_phase2.md` |
| 3 | `epicycles.amx` + `sorting_theatre.amx` | **Done** 2026-08-25 (merged) | clean `check`; 3-frame PNG smoke | Epicycles wave-reveal polish noted but merged; `sorting_theatre` uses `dynamic_layout` + build-time sort precomputation + `swap` actions |
| 4 | `brand_reel/` capstone | **Done** 2026-08-25 (merged) | all six `play` transitions ≥1×; `persist`; Audio; cross-file scenes | Multi-scene zero-duration bug fixed; cross-file slot fills / component-instance positioning workarounds landing with it |
| 5 | Tutorial refurbishment + README matrix + `scripts/check_examples.sh` smoke | **Done** 2026-08-25 | script green; render smoke covers all examples | Reuses new `lib/`; `animation/16_showcase.amx` and `composition/20_feature_reel.amx` are superseded by the gallery |

### Resolved Engine Bugs (gallery-era)

These were discovered during the demo-gallery work and are now all **resolved**
in code; they are kept as evidence. The remaining genuinely-open roadmap items
are the deferred performance backlog (PF-3/6/7/8/9) and the deferred Grid
auto-fit, per the 2026-08-26 session decision.

| Bug | Resolution |
|---|---|
| Multi-scene clamp on zero-inferred-duration scenes | Fixed 2026-08-22: floor inferred scene durations to `max(transition duration, 1/60s)` |
| Cross-file `@slot` fills ignored | Fixed 2026-08-25: `resolve_slots` is recursive at any depth. Render follow-up **closed 2026-08-26**: the renderer recurses into container children unconditionally, so the hypothesized traversal-skip does not exist and the Mask+Image clip parallel was already fixed. Residual non-traversal suspects (opacity inheritance / layout) are noted; re-open only with a concrete repro. |
| Cross-file custom component `fn` actions | Fixed 2026-08-25 (`d8bea5b1`): `stmt_needs_rewrite` gained a `Stmt::Action` arm so fn bodies are instance-prefixed at expansion, and `SymbolTable::merge` unions imported action names |
| Component instances ignore `anchor`/`offset`/`at` | Fixed 2026-08-24: expansion forwards `opacity`/`at`/`anchor`/`offset` to the expanded root actor |
| Col/Grid auto `text_max_width` overrides explicit value for CJK | **Fixed 2026-08-26**: width propagation now treats any explicitly set `text_max_width` as authoritative (no longer overrides with the container's propagated width); regression test in `timeline/tests/layout.rs` |
| `Mask` children clipped at the scene origin | Fixed 2026-08-24: clip layer now transforms with the Mask. `clip_shape` defining the clip geometry (and not painting) landed separately |
| Hosted plots occupy central half of their Graph | Fixed 2026-08-25 (`24da1f9bd`): `{graph}_size` stored and consumed as FULL size, with a regression test. Residual non-runtime footguns (stale `math_to_screen_padded` doc, `GraphGeometry` doc wording, `ProceduralPlot.p_size` FULL/HALF overload, `.map` vs `.map_inverse` key-name asymmetry) are noted; behavior is correct, only comments/docs were corrected |
| Failed property expressions fall back silently | Fixed 2026-08-24: multi-segment path failures report the full dotted path → `unknown-lookup-path` diagnostic |
| Graph-hosted PlotCurve stroke color ignored | Fixed 2026-08-25: plot props loop now resolves color tokens/tuples and links `color:` to the stroke |
| Equation Fragment leading `+` renders as `1.` | Fixed 2026-08-25: fragments are marker-escaped and joined with spaces |
| Invalid easing names fall back silently | Fixed 2026-08-24 (`4e8a607d`): unknown `ease:` names are retained for a build-layer `InvalidModifierValue` warning (regression test `invalid_easing_name_warns_on_assignment`) |

### Next Immediate Session Recommendation

The 2026-09-05/06 performance pass closed the ranked PF-4/PF-6 backlog
(rounds 6–12: see the ledger below) and landed the PF-7 baseline plus
readback pipelining. The next session should start by **saving a fresh
gate baseline** (`scripts/perf-bench.sh save`) — the current baselines
predate rounds 6–12. After that, the open items are the deferred scoped
items (`tracks` BTreeMap→HashMap needs fresh `perf_driver` evidence;
Grid auto-fit stays deferred), PF-2/PF-3 (paused), and real-GUI-session
capture via `animatix-gui --perf-log` to validate the seek-path wins on
live authoring. Track the rest as normal, commit-gated work per
`AGENTS.md`.

**Refreshed 2026-09-06 (PF-6 rounds 6–13 + rounds 11–12 + PF-7 + GUI seek validation):** the round-5 blocker is resolved —
the dense slot-id bounds table landed (`820c7e81`), removing the string-keyed
map and the `bounds_key_pool` from the render path entirely: per-node writes
are `Vec` stores, cache-hit restore is allocation-free, and the round-5
"Arc sharing" idea is moot (the clone it targeted is now a flat memcpy).
Round 7 (`a7b342c8`) removed the top remaining allocation residue: one
shared key buffer across the frame-env injection chain, so the steady-state
pooled-env frame performs zero key allocations. Round 8 (`153fcdd4`)
memoized shape commands per track with a take/encode/recycle protocol
(static shape actors re-encode a cached `Vec<RenderCommand>` with zero
allocations; dynamic actors keep the fresh build). Round 9 (`bc0bf0e9`)
profiled the export path for the first time — new `export_alloc_driver`
(real `.amx` through `OffscreenRenderer`) — and recycled the CPU readback
buffer, which alone was 93% of the export frame's churn. Round 10
(`d00d28a7`) took the lens to text-heavy `fft_explain` — the churn was
the plot-closure capture cycle, not text: `merge_missing_into` borrows
for its presence test and `Value::List` is `Arc<[Value]>` (all list
clones are refcount bumps; build-time cost `simple_build_only` +3%
accepted). Round 11 (`d063df86`, PF-7) pipelined the export readback:
`begin_frame_with_debug`/`begin_transition` queue the copy without
blocking and `wait_frame` polls index-scoped, the streaming export
pipelines begin frame N+1 before waiting on N — **dashboard_story @720p
169–234 → 501–554 fps (2.4–3×)**. Round 12 (`0aec3d66`) serves
shape-command local bounds from the memo hit, and (`38041c5a`) keeps the
static-subtree cache active under hit-region requests — the GUI always
requests `compute_hit_regions` for picking, which had been silently
disabling static-subtree reuse for every preview frame; the GUI's exact
frame path measures **16.09 → 5.51 µs (−66%)** on a 50-actor static
scene (new `static_50_actors_hit_regions` bench).

Ledger rows:

| Round | Change | blocks/frame | bytes/frame | Time outcome |
|---|---|---|---|---|
| 6 | dense slot-id bounds table (round-5 prescription; `bounds_key_pool` deleted) | 204 → 168.2 | 18.2 → 15.8 KB | perf_driver +5.1%; `timeline_evaluate_*` −50…−53% in the gate |
| 7 | one shared env-key buffer across frame-env injection | 100.2 | 14.6 KB | perf_driver +4.4% |
| 8 | per-track shape-command memo (take/recycle, `Box`ed) | 32.2 | 9.9 KB | perf_driver +0.8% |
| 9 | export-path readback buffer recycled (`RenderedFrame.rgba: Arc`) | — (export lens) | 3.97 MB → 282 KB | peak live 5.9 → 2.3 MB |
| 10 | `Value::List` → `Arc<[Value]>` + borrow presence test in plot captures | — (export lens, fft_explain) | 533.6 → 425.0 KB | blocks −27%; accepted `simple_build_only` +3% |
| 11 | PF-7 readback pipelining (`begin`/`wait` split, dual MAP_READ buffers) | — (time lens) | — | **2.4–3× export throughput** (169–234 → 501–554 fps @720p) |
| 12 | shape-command local bounds from memo + static-subtree cache under hit regions (GUI path) | — | — | GUI static-path 16.09 → 5.51 µs (−66%) |
| 13 | keyframe-free container layout static slot (evidence-gated `tracks` decision) | — (time lens) | — | perf_driver +1.5% (3×); full BTreeMap refactor closed by evidence |

Cumulative (evaluate lens) vs pre-PF-6: **−94.6% blocks, −98.0% bytes**
(600 blocks / 500 KB → 32.2 / 9.9 KB per frame), and the `many_actors`
bench pair is reliable again (its ±9–17% "contradiction" was leak-driven
swap pressure). Gate-flag discipline held across the rounds: every flag was
re-measured in adjacent A/B isolation; the overwhelming majority failed to
reproduce (§4 contamination — several measured change-*faster* in
isolation), `sample_all_tracks`' +4.3% was recovered by the round-8 `Box`
fix, and `static_50_actors_with_items` remains the one accepted cost with a
documented tooling-only justification (see the PF-6 row).

2026-09-06 full-pass review outcome: every ranked item from the 09-05
list is done (rounds 11–12), deferred with evidence, or closed as
inherent. The deferred `tracks` BTreeMap→HashMap then met its own
evidence gate (round 13): the fresh profile showed the per-node
`tracks.get` NOT in the top-10 — the actionable slice was the layout
fingerprint pass, fixed by the keyframe-free static slot instead of the
storage refactor. The GUI seek/scrub audit's fixes were validated on a
live scripted session (`--demo-script` + `--perf-log`): `rebuild_ms` 0.0
throughout, evaluate p50 1.78 ms, no seek spikes, `stale` always false.
Remaining known costs, all measured and deliberate: `simple_build_only`
+3% (round-10 trade), `static_50_actors_with_items` +9–12% (round-6
tooling-path trade), evaluate-loop residue ~32 blocks/frame (plot
closures + wgpu internals), export readback wait (overlapped by round
11), occluded-window compositor throttling at ~1 Hz (environment).

PF-2/PF-3 stay paused until the harness proves stable (a fresh 65-bench
baseline was saved 2026-09-06 after rounds 6–13). The performance
backlog is now fully worked through; new work should start from fresh
`perf_driver`/`export_alloc_driver`/`export_perf_driver` evidence per
§8 of the perf doc.

---

## Planned: Dogfood-Driven Fix Pass (2026-09-06)

Candidate plan for the Known Issues rows + spec drift + LG items surfaced by
the `taylor-sin` pass. Ordered so each engine fix lands with its regression
before the next depends on it. Nothing here is merged without the AGENTS.md
gates (`cargo fmt`, `cargo check --workspace`, syntax + serial lib tests).

**Stage A — silent Graph-child reveal (bug fix, highest user impact).**
Probe first (`dogfood/probes/010-graph-child-reveal/`): repro from notes.md
plus a Row-child control to bound scope. Fix: container entrance actions
(`FadeIn::execute` and siblings routed through `lift_hidden_by_default`)
cascade into Graph children, mirroring the layout-container behavior accepted
in dogfood run/003. Extend the `never-revealed` keyframe-based diagnostic to
Graph children so future breaks are loud. Regression: pixel test that
`fade-in g` reveals a hosted curve + a rebuilt 07_plots render smoke. Then
re-verify taylor-sin Target scene and drop its per-curve `fade-in` workaround
comment.

**Stage B — plot re-sampling gate (bug fix).**
Probe 011 with the three-spelling matrix from notes.md. Fix `is_dynamic()` to
also return true when the plot's `extra_captures` intersect names written by
an `always` block in the same scene (build-time scan; the shadowing machinery
downstream already works — only the gate is wrong). Regression: two-time
render-differs test on the spec §14 literal example. Re-verify the f3
probe (timed `curve.freq` corruption) separately: temporary tracing on the
injected `param_track.evaluate` value, fix, pixel regression at mid- and
post-transition times — it may share the root cause or be its own bug; do not
assume.

**Stage C — spec corrections (docs + example alignment, no runtime change).**
1. §14 stroke_progress example: add `fade-in signal` (match gradient_descent's
   working pattern) and note the pre-keyframe hiding interaction.
2. §14 Runtime parameters: after Stage B the literal example works; keep it
   and add the inline-`t` spelling as the documented reliable alternative
   until B ships.
3. `highlight` signature: decide one way — either add `intensity` to
   Highlight's ActionSignature + implement it, or remove `intensity:` from
   gradient_descent.amx (and any spec mention). Removal is the honest option
   today.
4. §14 §8: document that unlabeled actors inside `Graph` require labels until
   the anon-prefix exemption lands (Known Issues row).

**Stage D — language gaps (design-gated).**
LG-2 (`format` precision) first: small, both-paths change, immediate authoring
value. Then LG-1 behind a short design note (folder builtins touch the IR and
the capture machinery — design before code). LG-4 (modifier signature
validation) is mechanical but touches timing.rs shared parsing; land after
the highlight call-site cleanup from Stage C so no shipped example regresses.
LG-3 closes automatically when Stage B lands; re-spec then.

---

## Archived Ideas

These are not open tasks and should not be scheduled without a concrete user
story or design requirement. Audit status is from 2026-08-05; some items were
superseded by later implementation.

| Task | Reason / Audit Status |
|------|-----------------------|
| **Scene primitive / picture-in-picture** | Transition blending shipped; existing components and `Stack` cover most reuse cases. Unchanged. |
| **Asset usage tracking** | Show which actors reference an asset; no strong user story yet. Unchanged. |
| **Variable track UI** | GUI for `let` variable tracks; `always` blocks cover most interactive cases. Unchanged. |
| **Module dependency graph** | Visual graph of `.amx` imports; internal tooling value only so far. Unchanged. |
| **Lossless whitespace/trivia preservation** | Current write-back pipeline correct for all normal use cases; comments roundtrip, formatting idempotent. Unchanged. |
| **APNG export** | Request-driven only; GIF covers lightweight previews, video/WebM covers higher-quality sharing. Unchanged. |
| **Source-diff preview sidecar** | Show the `.amx` diff when dragging actors or editing properties in the inspector. Unchanged. |
| **Animation heatmap view** | Heatmap of animated property density across time, actors, categories. Useful for large generated `.amx` files. Unchanged. |
| **Auto-sorted property registry** | Keep manually sorted with `registry_is_sorted` guard; proc-macro adds more maintenance surface than it removes. Unchanged. |
| **Interactive step control (presentational mode)** | Manim-style `wait()` / `next_slide()`. Architecturally incompatible with Animatix's declarative deterministic playback model. GUI scrubbing covers most use cases. Unchanged. |
| **Auto-arrow routing / smart connector layout** | Actor anchor-point endpoint refs (`from: n0.right`, `to: n1.left`) cover manual auto-tracking. Remaining value is automatic edge routing/relayout, still niche. |
| **Speaker-notes metadata** | No presentation/export consumer yet; add `notes` when a concrete user story exists. |
| **AI review evaluator/loop** | Full design is in `docs/ai_agent_animation_quality.md`; implementation is a new review crate/rule engine/agent loop, not a single backlog task. |
| **Per-actor exit before scene transition** | Animate individual actors out before `play SceneName [fade, ...]`. Workaround: `fade-out` actions timed at scene end. Transition blending is already uniform. Unchanged. |
