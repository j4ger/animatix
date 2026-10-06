# Animatix Implementation History

Archive of completed, resolved, and closed work. The forward-looking backlog
lives in [`roadmap.md`](roadmap.md); entries here are kept as evidence and
should not be scheduled — take them as prior art when a related item reopens.

Moved out of `roadmap.md` on 2026-09-13 (the roadmap had grown to 800 lines of
mixed history and open work).

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
| GPU `Filter` `blur`/color effects are not visibly applied in `animatix image` export | A `Filter` with `blur: 10` over a high-contrast checkerboard stayed sharp. Root-caused 2026-08-31: back-to-back compute passes sharing ping-pong textures in one encoder did not synchronize (a color-matrix control proved the machinery works; a two-pass blur returned the untouched copy until split). | **Fixed 2026-08-31**: each blur/color-matrix pass is now submitted in its own encoder (submit boundary = sync point); `gpu_filter_blur_softens_a_hard_boundary` + `color_matrix_actually_desaturates` are the regression guards (backend, content-level); pixel-verified (soft gradients on the checkerboard). See `dogfood/probes/009-filter-gpu-deferred`. The Filter scene-eval silent fallback is now surfaced as a runtime diagnostic (2026-09-07: no-backend and gpu-filtered-failure paths push `RenderFailure` warnings into `runtime_diagnostics`). |
| Typst math with implicit multi-letter coefficients fails to compile | `Typst, content: "$mc^2$"` / `"$E = mc^2$"` error because Typst parses `mc` as a single multi-letter *variable*, not `m*c` (a Typst math gotcha). This is correct Typst semantics, not a bug. | **Resolved 2026-08-28**: the compile error now surfaces Typst's real message ("unknown variable: mc" + its hints) instead of an opaque "failed to compile Typst document". For a multi-letter product write `$m c^2$` or `$"mc"$`. |
| Container `fade-in` never reveals Graph-hosted PlotCurve children (2026-09-06, **silent**) — **Resolved 2026-09-06** (`26b4ca46`) | Pre-keyframe declarations are hidden by default; `fade-in <graph>` lifts the container but did not cascade into Graph children, so the hosted curve stayed at seeded opacity 0 forever with **no diagnostic**. Pixel-verified on `examples/data/07_plots.amx` (t=3.0/6.0): its headline sine was invisible the whole scene. Twin quirk: explicit `opacity: 0` on the child bypassed hidden-by-default | `lift_hidden_by_default_subtree` cascades into children on container entrance actions; the `never-revealed` ancestor check now requires a genuine 0→positive ramp (declaration-time constants no longer suppress it) with generated tick/bar labels exempt. Fixed `07_plots.amx` (headline sine back) and surfaced `23_plot_kinds.amx` (fixed with a container fade-in). Tests: `container_fadein_reveals_graph_hosted_children`, `unrevealed_graph_child_still_warns_never_revealed`; probe 010 resolved |
| spec §14 "Runtime parameters" plot pattern never re-samples (2026-09-06) — **Resolved 2026-09-06** (`291ae9ff`) | `let freq = 2` + `always { freq = ... }` + `func: (x) => sin(freq * x)` rendered **static** (pixel-verified identical at t=0.3/5.0). `ProceduralPlot::is_dynamic()` = `references_ident("t") \|\| !param_names.is_empty()`, so capture-only plots reused cached build-time `vector_paths` and the frame-env shadowing path (scene_eval.rs:522+) was unreachable | `Timeline::collect_frame_written_vars` scans lowered always-blocks (bare assignments + `let`s, walking if/for) into `frame_written_vars`; `is_dynamic` also fires when `extra_captures` intersect that set. Spec §14 pattern now animates as written (probe 011 resolved). Test: `plot_capture_of_always_written_var_is_dynamic` |
| High-frequency plot curves under-sample to a straight line (2026-09-06) — **Resolved 2026-09-06** (`aab8ced9`) | Initially attributed to timed plot-param assignment ("wrong curve"), but a plain `func: (x) => sin(5 * x)` reproduced it: the adaptive samplers' subdivision floor was 3 levels (8 samples), nearly collinear for ~16-period functions — the param-track machinery was innocent (probe 012's corrected diagnosis). Timed `curve.freq = 5 [1s]` itself works | `sample_recursive_cartesian`/`_polar`/`_parametric` take a `min_depth` derived from the plot's `resolution` (`resolution.max(8).min(max_depth).ilog2() + 1`); adaptive subdivision continues beyond the floor. Pixel-verified sin(5x) and the probe both render the full wave. Test: `high_frequency_curve_meets_resolution_floor`. Analyzer infos for declared plot params also closed (2026-09-07, `plot_runtime_params`) |
| ~~Unlabeled actor inside a Graph fails the build on an engine-generated name~~ — **Resolved 2026-09-07** | `__anon_sw_graph_1` tripped `error[build:reserved-label-prefix]` — the generator's own output violated the reserved rule | The check now exempts `is_anonymous` declarations; unlabeled Graph children build cleanly (regression test `anonymous_graph_child_builds_without_reserved_prefix_error`). Spec §8 label guidance stays as good practice |

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
  BoldItalic, Apache-2.0) vendored under `crates/animatix-text/assets/fonts/` with
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

---

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
| Open backlog build target | Done by formal decision, revised 2026-09-13. The `render`/`text`/`svg` features were removed outright: every supported build had them on, so their `not(feature = ...)` branches were unreachable code. `--no-default-features` now builds the engine with `perf-tracing` off; the no-FFmpeg guarantee moved to "build CLI/GUI without `video`", which CI checks on a runner with no FFmpeg installed. |
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
| Dogfood content verification harness | Done 2026-09-11. New `animatix verify` CLI command + `animatix::verify` pixel helpers + `scripts/dogfood-verify.sh` (local only, not CI). A line-based `verify.txt` beside a dogfood project asserts what is actually rendered — `visible`/`invisible` (ink inside evaluated actor bounds), `reveals` (an actor's region changes between a hidden and a shown time), `differs`, `ink` (non-blank). This closes the loop's core blind spot: `check`/`lint` see only the build model and a whole-frame render smoke stays green while one actor silently disappears (`examples/data/07_plots.amx` shipped an invisible headline curve through both). `OffscreenRenderer::render_timeline_observable` returns pixels **and** `precise_bounds` from one evaluation so they cannot disagree; `reveals` is the robust check when an actor's bounds contain other painted content (a curve inside a Graph that paints axes — raw `ink` cannot tell a revealed curve from a missing one). Both dogfood projects now carry `verify.txt` (taylor-sin 14 checks, sorting-visualizer 10) and pass. Verified the new gate fails (exit 1) on a never-revealed actor and passes on the same scene with a `fade-in`. |
| Pre-keyframe plot-host reveal defects | Done 2026-09-11. Two fixes surfaced by the verification pass. (1) A `Graph` / plot host declared before the first keyframe was never seeded hidden: `add_node` creates the parent's track while registering children, so a first declaration *with* children looked like a re-declaration. Its self-drawn axes therefore showed before the entrance action and `fade-in` produced a 1 → 0 → 1 opacity dip instead of a clean reveal (`examples/data/07_plots.amx` and `gradient_descent.amx`). `process_plot_actor_dispatch` now captures first-declaration status before children are processed. Recursive layout containers (Row/Col/Grid/Stack/Group/Mask/Filter) deliberately stay visible-by-default because `expand_group_targets` targets their leaves and skips the container. (2) The build-time plot probe warned `Undefined variable` for *declared* plot params (`freq: 2`) that are injected per frame; `PlotCurveParams.param_names` now exempts them while a genuine typo still warns. `gradient_descent.amx` gained the now-required `fade-in` for its two pre-keyframe graphs. |
| Primitive abstraction hardening (3 phases) | Done 2026-09-11. Closed the remaining places where the render pipeline special-cased concrete primitives instead of going through the unified `Primitive` trait. **Phase A** (`b0e1f22e`): Mask clip geometry and Equation fragments are capability-driven — `Primitive::clip_path` / `Primitive::equation_fragment` replace the hard-coded `ActorKindId::Shape(Rect|Ellipse)` and `ActorKindId::Fragment` checks. A Polygon `clip_shape` now clips to the polygon instead of silently becoming a rectangle, and a `clip_shape` with no geometry warns (`RenderFailure`) instead of failing silently. Native ABI bumped 7→8 with optional `clip_path` / `equation_fragment` callbacks (`NativeClipPathCtx` / `NativePrimitiveEquationFragmentCtx`) so cdylib primitives participate. **Phase B** (`eaf6fec8`): `AnimationTrack::actor_type` is now a required `String` (derived `kind`), so every runtime lookup resolves at most once via the registry — all "registry-first, then `ActorKindId` fallback" sites in `scene_eval`, actions, assignments, property_engine, and the GUI are gone. Legacy `CarryBag` payloads normalize once on injection (`normalize_identity`); plot hosts create their parent track before processing children so `add_node` never has to default one in; a build-time pass warns for any unregistered `actor_type`. **Phase C** (`721e8ae8`): `Primitive::render_children(&mut RenderChildrenCtx, children)` is the single child-rendering entry point — `scene_eval::render_node_children` no longer branches on `ChildProcessing`. |
| Primitive abstraction hardening follow-up | Done 2026-09-11. Gap-fix and completeness round on top of the three phases. `clip_path`'s default now concatenates **every** command and subpath (`clip_bezpath_from_commands`), so a multi-path `clip_shape` (e.g. an `Svg` with several path elements) no longer loses geometry; the Mask clip is placed with the clip child's fully resolved local transform (anchor/offset/rotation/scale), not just its raw position; `clip_shape` is documented in `docs/primitives.md` along with `Filter`. Native `clip_path`/`equation_fragment` callbacks now have an adapter unit test. Identity writes funnel through `AnimationTrack::set_identity` (built-ins) or the primitive's `kind_id()` (extensions can opt into a built-in kind), with a build-time kind/actor_type drift check and a corpus-wide regression test — that guard immediately caught the in-process-extension case where `Gauge` is `ActorKindId::Text`, not `Extension`. Finally, `Filter`/`Mask`/`Equation` now own their pipelines through dedicated `render_children` overrides (`Timeline::render_*_children_ctx`) and the trait default is plain generic recursion; the native adapter dispatches on `child_processing()` so plugin containers keep their declared strategy without new ABI. |
| Primitive abstraction — deferred decisions | Decided 2026-09-11, not scheduled. Two candidates were evaluated and deliberately left out; re-open only with a concrete need. (1) **Native `render_children` callback (ABI 9)**: native cdylib plugins cannot be given the host's child pipelines because `Filter`'s offscreen GPU composite, `Equation`'s Typst compilation, and sub-scene image readback have no `repr(C)` representation. Plugins already reuse the host's Filter/Mask/Equation by declaring `child_processing`; if a plugin ever needs a genuinely new strategy, implement the minimal surface (child enumerate + `render_child` + host `push_clip_layer`/`pop_layer` + the existing `append_*` callbacks), keep it orthogonal to `child_processing` (so analyzer/manifests are untouched), bump 8→9, and document the offscreen/Typst limits. (2) **Privatizing `AnimationTrack.kind`**: `kind` is an intentional independent override for extension primitives (they opt into a built-in kind via `kind_id()`), so privacy relocates one write rather than making `actor_type` the only identity. The invariant is already enforced by `set_identity` + the build-time drift warning + `every_built_track_identity_is_consistent`. Full status/gap analysis and a recommended order if the remaining non-uniformity (build side, property system, identity, taxonomies) is ever pursued: [`primitive_abstraction.md`](primitive_abstraction.md). |
| Built-in primitive registration pairing | Done 2026-09-15. The `CATALOG` (std metadata) and `PRIMITIVES` (engine behaviour) arrays were two parallel lists pinned *positionally* across crates by a runtime zip test. The engine now registers through `BUILT_INS: &[BuiltIn]`, where each row references its card by symbol (`BuiltIn::new(&animatix_std::catalog::RECT, &RECT)`), mirroring the effect registration idiom. Cards are `pub static PrimitiveInfo` built by const-fn constructors (`shape`/`text`/`new` + `advanced`/`stroked`/`with_child_processing` builders; no macros), so the ~12-line struct literals collapsed to one line each and `CATALOG` is a reference list (presentation order only). The positional zip test became a name-pairing + set-equality test; the registry's built-in lookup no longer runs `catalog_lookup(...).expect(...)` (the card rides in the registration row). Bonus drift fix: the `docs/spec.md` LLM checklist was missing `Math` and `Callout`; a new std test (`spec_llm_checklist_primitives_match_catalog`) pins that hand-written name list to `CATALOG` so it cannot drift again. |
| `Code` syntax highlighting | Done 2026-09-15. `Code` already compiled its body through a Typst `raw` block (four-backtick fence), so highlighting needed only the fence's language tag — no new dependency, no second tokenizer. The `language` property (removed 2026-09-14 for having no consumer) returns as a real `Applicable::Actors(&["Code"])` descriptor + `ActorField::Language` lane + `TextTracks.language` track, threaded through `evaluate_text_paths` → `compile_code`. A recognized language (validated case-insensitively against `typst::text::RawElem::languages()`) emits ```` ````rust ````; an empty or unrecognized one renders plain, and the latter logs a `tracing::warn!` so the author's request is never silently dropped. Highlighting recolors tokens only — glyph metrics, layout, and the actor's base `color` are untouched — so the build-time precompile, width-propagation, and baseline-metrics paths all stay correct. `language` joins the process-wide text compile cache key so two languages of the same snippet never share an entry. Tests: `code_highlight_adds_glyph_colors`, `unknown_highlight_language_renders_plain`, `language_is_part_of_text_cache_key` (`animatix-text`), `code_language_reaches_text_lane` (`animatix`). The property table grew to 97 rows (`PropertyId` appended, never reordered). |
| `Code` highlight follows the colorscheme | Done 2026-09-15. Highlighted token colours were Typst's fixed raw theme; now they follow the active colorscheme. `HighlightPalette` (animatix-text) names the eight token roles; `HighlightPalette::resolve` maps a syntect theme colour back to its role. The engine's `ResolvedColorscheme::highlight_palette()` fills the roles from named scheme tokens (`accent.danger`/`primary`/`success`/`secondary`, `text.muted`, …) — the same `color(key)` resolution every other primitive's `color` uses, just eight roles at once, resolved at build. The palette is a timeline-wide constant, so it lives on the timeline-scoped `TextCompiler` (set once in `apply_colorscheme`) rather than per-actor: the evaluate interface (`EvaluateCtx`, `evaluate_text_paths`, `Primitive`) is untouched, and only `compile_code`/`compile_text_cached` gained one `palette` parameter. It joins the text cache key so two colorschemes never share a highlighted entry. Tests: `highlight_palette_recolours_tokens`, `palette_is_part_of_text_cache_key` (`animatix-text`), `highlight_palette_maps_roles_to_scheme_tokens`, `highlight_palette_falls_back_when_tokens_missing` (`animatix`). |

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

---

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

### Post-Processing Effect Abstraction (implemented)

Goal: make pixel effects a first-class, composable chain owned by a `Filter`
compositing scope, and let native plugins author effects by shipping
**WGSL source + a parameter schema** — the host compiles and runs the shader on
its own device, so no GPU handle crosses the FFI boundary. Contract:
`docs/effects.md`. Rationale: `primitive_abstraction.md` §6.

**Shipped (2026-09-11).**

- **Effects are a chain, not primitives.** `Filter` lowers its effect children
  into the scope's `EffectChainTrack` at build time (`timeline/effects/track.rs`,
  `build/effect.rs`); effects create no scene node, layout entry, or hit region.
  Declaring an effect outside a `Filter` scope is a diagnostic.
- **Authoring surface.** Labelled child declarations `soft: Blur, radius: 10`
  in application order; parameters animate as `scope.stage.param = value`
  (scope-qualified assignment, resolved before the generic target walk). The flat
  `Filter, blur: …` properties, `FilterTracks`, and `ActorField::Filter*` are
  deleted (no compatibility shim); examples, `docs/spec.md`, and editor
  completion were rewritten.
- **Parameter storage.** Each stage stores `BTreeMap<param, DynTrack>` plus
  `enabled: DynTrack`, reusing the dynamic-track animation/keyframe/serde
  machinery; effect parameters never enter `PROPERTY_REGISTRY`/`PropertyPlan`.
- **Renderer seam.** `FilterBackend` takes an `EffectChain`; a generic ordered
  N-pass driver runs it over host-owned ping-pong textures, each pass in its own
  encoder (probe 009 sync fix retained). Bind group 0 = in texture, storage
  texture, author uniforms, host `EffectContext`, linear sampler.
- **On-demand.** Identity-defaulted parameters and `enabled` let the pipeline
  skip no-op stages; a fully identity chain skips the offscreen round-trip.
- **Built-ins:** `Blur` (radius, 2 passes), `ColorGrade`
  (brightness/contrast/saturate/hue_rotate/sepia, 1 pass), and
  `ChromaticAberration` (offset, 1 pass through the linear sampler).
  `ChromaticAberration` landed as the seam validator: the diff touched only the
  descriptor table, the analyzer `effect_specs()` table, docs, and tests — no
  core match arms.
- **GUI.** The inspector gains an "Effects" group listing each stage's
  parameters; edits route through a new `SourceEdit::{Set,Insert}EffectParam`
  and the in-memory `EffectChainTrack::write_param`. Effect parameters have
  timeline lanes (`stage.param`, `stage.enabled`), and keyframe
  create/delete/easing/move work end-to-end: `keyframe_edits` matches dotted
  targets (`[scope, stage]`, property `param`), and lane adds keyframe the
  currently sampled value.
- **Analyzer.** `animatix_syntax::schema::effect_specs()` provides effect types
  and parameters for completion and property diagnostics; a drift test pins the
  table to the runtime descriptors.


> The follow-up effect wave that was planned in this
> section now lives in [`roadmap.md`](roadmap.md) ("Planned Effects").


- **Crate layering (2026-09-13).** The workspace gained `animatix-core`
  (capability vocabulary, effect trait, icon glyphs — zero heavy deps) and
  `animatix-std` (the built-in catalog: full effect definitions and primitive
  identity cards). The dependency order guarantees the layering: the analyzer
  and LSP can see every built-in without linking the engine, because author
  -visible metadata now lives *below* the engine, not beside it. Adding a
  built-in effect = one file + one `EFFECTS` line in `animatix-std` (the
  parser crate derives its contract from the catalog); adding a built-in
  primitive = one catalog row (std) + one behaviour file + one `PRIMITIVES`
  line (engine), with the engine's metadata trait methods (display name,
  category, icon, capabilities, child processing across 30 files) deleted and
  `ExtensionContext::register_primitive` taking a `PrimitiveInfo` card.
  Phase C (property-applicability predicates) landed 2026-09-13: the
  `Applicable` predicate vocabulary lives in `animatix-core`, the parser
  crate's property rows declare one predicate each (the ~900 lines of
  hand-maintained actor-type lists are gone — adding a capability-shaped
  property needs no per-primitive list edits), and `property_specs()`
  materializes the per-type lists over the catalog. The golden comparison
  against the old lists caught and fixed two stale-data bugs: the
  near-universal rows (at/anchor/opacity/position/rotation/scale/shift/
  transform/legend) had silently excluded `Math`, and the Phase-3
  `SizedActors` conversion had silently included `Equation` (both restored to
  their pre-refactor intent). Phase D (renderer split) landed 2026-09-13 as
  `animatix-text` + `animatix-render` (below).

- **Renderer split, Phase D (2026-09-13).** An earlier feasibility pass called
  this infeasible as scoped; that verdict was wrong because it conflated three
  very different things under "renderer": the frame vocabulary
  (`renderer/error.rs`, `renderer/types.rs`, `renderer/text.rs`), the build/
  evaluate paths that *use* compiled text, and the GPU stack. Re-research
  showed the GPU code only touches the engine's public API, and the text
  compiler's only crate-internal dependency was `RenderError` — so two moves
  break the cycle:
  1. `animatix-text` — the typst/fontdb compiler (+ its bundled fonts), now
     depending on `animatix-core` for `RenderError` and nothing else in the
     workspace. The engine's `text` feature enables it, and
     `animatix::renderer::text` is a re-export, so build/evaluate call sites are
     unchanged.
  2. `animatix-render` — renderer core, `GpuFilterBackend`, fullscreen blit,
     offscreen frames, transitions, encode/video, and the `video` feature
     (FFmpeg). It depends on the engine, never the reverse; the engine's
     `FilterBackend` trait and `PendingComposite` are the seam, and the moved
     code uses only public engine API. `animatix::renderer` keeps the frame
     vocabulary (`error`, `types`, `text`).
  GUI and CLI gained a direct `animatix-render` dependency and their `video`
  feature now forwards to `animatix-render/video` (the engine has no `video`
  feature). The engine's own `animatix-render` edge is a dev-dependency for the
  export-path examples — note that render-crate types must not appear in engine
  unit tests, because Cargo then builds the engine twice and the types stop
  unifying; the offscreen end-to-end tests therefore live in
  `crates/animatix-render/tests/`, and the white-box readback test stays inside
  the render crate. Three orphaned effect files left behind by the Phase A
  move were deleted.

- **Identity unification (2026-09-12).** Identity is the authored type name
  everywhere: `EffectId` is the effect's name (persisted as a bare string, so
  plugin effects no longer drift through host-assigned registry slots), and
  `AnimationTrack.kind`/`ActorKindId` are replaced by `ActorCaps`, a `Copy`
  capability projection derived at identity time from the primitive
  (`child_processing`, `shape_kind`, `text_kind`, `has_stroke_path`,
  `PrimitiveCapabilities` bits, `group_like`). The `Applicable` property
  vocabulary became capability predicates, extensions now get the full
  built-in treatment (text layout, property plans, effect scopes), the 31
  `kind_id()` impls and the `TYPES` table are deleted (types derive from the
  contract tables), and `ExtensionManifest::from_runtime` now bridges
  plugin-registered effects into `[[effects]]` manifest metadata. Adding a
  built-in effect = one behaviour file + one `EFFECTS` line + one
  `effect_specs()` contract row; adding a built-in primitive = one behaviour
  file + one `PRIMITIVES` line + one `builtin_primitive_specs()` row (+ its
  property rows). Contract: `docs/effects.md`; identity model:
  `docs/architecture.md` §15.
- **Catalog declaration (2026-09-12).** Built-ins are unit structs implementing
  the `Effect` trait (schema, WGSL, `pack`, and `support` in one file each) in
  `timeline/effects/`, registered once in the `EFFECTS` array. Plugin effects
  wrap FFI data (`PluginEffectData`) in the same trait, so the renderer sees only
  `&dyn Effect`. This deleted `EffectDescriptor` as the built-in form, the
  `EffectSupport` / `EffectPackFn` fn-pointer indirection, the three hand-written
  lookup matches, and the per-effect `static` descriptors; adding a built-in now
  touches one file, one array entry, one `EffectId` variant, and one
  `effect_specs()` row. The drift test was upgraded to a bidirectional pin
  (count + display name + parameter names/kinds), matching the primitive
  registry's test.
- **GUI insertion (2026-09-12).** The palette gains an "Effects" tab built from
  `EFFECTS`; selecting an effect inserts `label: Blur` at the head of the
  selected `Filter` scope's body (via `SourceEdit::InsertEffect`, placed after
  existing effects and before content) with a scope-unique label. Inserting
  without a `Filter` selected reports a status message instead.
- **Plugin-authored effects (ABI snapshot 9).**
  `NativePluginApi.register_effect(host, NativeEffectDescriptor)` — WGSL source
  + parameter schema (declared uniform offsets + identity values) + ordered
  passes + `support_px`. Registered descriptors live in a process-wide effect
  registry keyed by authored type name (`EffectId` is the name, persisted as
  such, so saved projects keep pointing at the same plugin effect across
  runs); the lowering path is identical to built-ins, and uniform packing uses
  a generic layout packer. Registration validates kinds/alignment/size and
  rejects name collisions; WGSL compiles lazily at first render with errors
  surfaced as diagnostics. Rollback of a failed install unregisters the
  plugin's effects. The demo plugin ships a `Pixelate` effect. Extension
  manifests do not yet carry effect metadata, so analyzer completion for
  plugin effect parameters is future work.

**Remaining (optional optimizations only).**

1. **Derived-ROI adoption on the zero-readback path** is gated by the existing
   last-rendered-element precondition; generalising it to per-region
   intersection ("no later sibling intersects the ROI") would let mid-scene
   scopes take the zero-readback path too. The readback path already derives
   the region automatically.
2. **Effect metadata in `plugin describe` output** — the generated manifest
   snapshot does not yet round-trip the `[[effects]]` section from runtime
   registrations (the analyzer reads effects from the authored manifest
   directly).
3. **Dogfood finding (probe 010, container-layout workstream):** a Text child
   of an effect scope renders at different positions depending on whether the
   scope's chain is active (container-relative, correct) or empty (full-scene
   fast path, scene bottom-right corner). The two paths disagree on the child
   coordinate space for at-less children when the container has an explicit
   `at`; also, container `at` is parent-center-relative, not absolute. Minimal
   repro: `dogfood/probes/010-effect-chains/enabled_toggle.amx` at t=0.5 vs
   t=2.0.

**Guardrails.** Preserve the zero-readback `PendingComposite` park protocol,
`RenderedFrame` buffer reuse, chain/declaration order determinism, and the PF-7/
PF-9 per-frame allocation budget. Pipeline creation caches on
`(source, entry, layout)`; validation errors surface as diagnostics. Arbitrary
WGSL is trusted-authoring, not a sandbox.

**Historical note.** The original plan made effects primitives
(`ActorCategory::Effect` + `PROPERTY_REGISTRY` params). That was abandoned after
finding that the primary build paths (`Timeline::build(&ast)` in the GUI and in
image/video export) run with `extensions: None`, so extension-registered
parameters would never be written; and that forcing effect parameters through
the primitive property pipeline required ~141 cross-cutting references. The
chain-as-its-own-axis design (D) replaced it and deleted that whole surface.

---

---

## Audit Fix Pass — Round 2 (2026-09-08)

Second review pass (`feat/audit-followups-2`), from re-auditing the first
round's fixes plus new-chain walkthrough:

- **Brace-list assignment leniency**: all-numeric brace lists of matching
  arity are accepted at the property boundary — `at`/`position`
  (`parse_numeric_vec2*` via `list_as_vec2`), `size`
  (`handle_size_assignment`), `color` (all-Num 3/4 lists in
  `color_from_value`), and generic registry Vec2/Vec4 properties
  (`value_parser`). Heterogeneous/wrong-arity lists emit an
  `invalid-property-value` warning suggesting parens instead of the three
  former silent failures (at/size re-keyframed old values; color fell back
  to a wrong gray — all present since `3a9a5bad`, 2026-06).
- **Effect-key whitelist host-gating refined**: the whitelist in
  `parse_timing_modifiers` is Action-host-only — the same keys on
  assignments/declarations now warn as silent no-ops (zero corpus usage).
  Note: an earlier observation that the whitelist was removable was wrong —
  it is load-bearing (parse runs inside each action's execute before the
  LG-4 signature check).
- **env_keys violations cleared**: plot-param injection in scene_eval now
  routes through `env_keys::property_into`; `property_into` shape pinned by
  test; module doc carries an explicit constructor rule with the two known
  past violations recorded.

Also landed: "Adding a New Expression Semantic to the IR" touch-point
checklist in `docs/contributing.md` (8 sites, two compiler-invisible) with a
pointer from AGENTS.md.

---
## Audit Fix Pass (2026-09-07)

From a full-chain review (lexer→parser→IR→eval→render→build) during the
prop-ref/series work; evidence in the session. Landed 2026-09-07
(commits `ccc1b3ed`…`3c46e693`):

- LetChain closures now capture let-bindings (`CapturedEnv::snapshot` merges
  let-scopes); `get_ref` carries a tripwire (unreachable today, three
  presence-only callers) against future scope-blind reads.
- Plot closure evaluation failures surface as diagnostics: build-time probes
  report non-frame-variable errors (a `t`/arg `UndefinedVariable` is the
  designed handoff to frame resampling); frame-time NaN gaps emit one
  `RenderFailure` warning per actor per frame. This closed the
  silent-NaN debugging trap from probe 012.
- Effect-key whitelist in `parse_timing_modifiers` is host-gated: the six
  keys stay silent only for the Action host (the LG-4 signature check runs
  there); on assignments/declarations/text they now warn as silent no-ops.
- `make_vec_value` requires numeric elements (`{1, "x"}` no longer coerces
  `"x"` to a hidden 0.0); Graph tick/bar-label children take
  `text.muted` from the colorscheme (the hardcoded `#888888` was a dead
  literal — `Str` never parsed, effective color was the 0.8 gray fallback).
- Test infra: `without_content_lints` deduplicated into `tests/mod.rs`
  (was copy-pasted in 4 files) and applied to variable_tracks assertions.

Open follow-ups from the audit (see also `docs/series_construction.md`) —
**all three closed 2026-09-07** on `feat/audit-followups`:

1. ~~IR list-literal alignment~~ — **Done** (`5f98b1f8`): lowering
   distinguishes brace lists (`MakeList`, elements preserved) from paren
   tuples (`MakeVec`, numeric coercion) — identical semantics on the
   tree-walker and the IR. Verified: corpus clean, IR parity suite extended
   with `brace_list_preserves_elements_through_ir`. (Note: `box.at = {a, b}`
   brace-assignment was already a no-op on main — the alignment did not
   change assignment behavior; paren tuple assignment is the working form.)
2. ~~PropRef build-time validation~~ — **Done** (`fc26013c`): a post-lowering
   pass walks modifier programs (`collect_prop_refs`), checking label
   existence against `tracks` and injectability against
   `property_registry::resolve_property`; typos surface as
   `unknown-target-path` build warnings instead of frame-time failures.
3. ~~CLI runtime-diagnostics blind spot~~ — **Done** (`1272ed72`):
   `animatix image` drains `runtime_diagnostics()` to stderr after export;
   the same commit fixed the sampler's NaN swallowing (the chord-deviation
   test was always-false on NaN, which terminated subdivision and erased the
   curve — now NaN forces subdivision to the resolution floor so gaps render
   and the empty-path/NaN signal reaches the diagnostics check).

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

---

## Built-in effects, wave 2 (2026-09-14)

`Duotone`, `Posterize`, `Edge`, `LensDistortion`, and `DropShadow` (hard)
shipped through the same single-source route as wave 1: one file in
`animatix-std/src/effects/` plus one `EFFECTS` entry each, no parser row, no
GUI change — the insertion palette and Inspector picked them up from the
catalog.

- `Duotone` — Rec. 709 luma mapped onto a `shadow` → `highlight` ramp, with
  `amount` blending the ramp over the original so identity (all params at
  identity) stays a no-op.
- `Posterize` — `round(c × (levels-1)) / (levels-1)`, which keeps 0 and 1
  exact; `levels: 2` is a hard threshold.
- `Edge` — 3×3 Sobel on luma, magnitude normalised by 4 (the kernel's
  positive weight sum), `threshold` gating and `amount` blending; `support`
  is 1 px for the neighbourhood.
- `LensDistortion` — quadratic UV warp through the linear sampler, scaled by
  the canvas diagonal so the corner displacement equals `amount` px on both
  axes; `support = |amount|` (the true maximum is `0.707 × amount`).
- `DropShadow` — reads the source alpha at `coord - offset` (integer texels,
  so the shadow stays hard and `support = |offset|` exactly) and composes the
  original over it in premultiplied space. Unlike `Bloom`, this needs no
  second input texture: the shadow colour is constant, so one displaced read
  is enough — the roadmap's "second pass over alpha only" was an
  over-estimate. Hard shadow only; the soft variant still waits on the
  second-input ABI.

GPU pixel tests live in `animatix-render` (`duotone_maps_luma_onto_the_ramp`,
`posterize_quantises_midtones`, `edge_lights_the_step_boundary`,
`lens_distortion_displaces_samples_toward_the_centre`,
`drop_shadow_offsets_the_silhouette`).

---

## Authored `solo` (2026-09-14)

`Track solo` landed as a language property, not a GUI-only view state:

- `solo: true` (default `false`) is registered in both property tables
  (`animatix-syntax` shared schema and the engine's `PROPERTY_REGISTRY`,
  `ActorField::Tagged("solo")`), so declarations, assignments, analyzer
  validation, completion, and the formatter all pick it up with no bespoke
  code. It is `ASSIGNABLE` but deliberately **not** `ANIMATED`: solo selects
  what renders, so keyframing it would fight the frame/static-subtree caches
  for no authoring value.
- Evaluation resolves the flags once per frame into `SoloState` (an
  allocation-free `None` when nothing is soloed) and prunes non-solo subtrees
  whole in `render_actor_node`, while ancestors of a soloed actor stay
  traversable so deeply nested soloed actors still draw. `visible: false`
  wins over solo.
- The timeline track header gained a headphones toggle that pushes a
  `PropertyEdit` for `solo` through the existing source-edit pipeline, so the
  change is undoable and persisted to the `.amx` — unlike the eye/lock pair,
  which remain ephemeral by design.
- Tests: `timeline/tests/solo.rs` covers the no-solo baseline, root-level
  solo, sibling hiding inside a container, recursive subtree pruning, a
  deeply nested soloed actor, and the hidden-wins rule.

---

## Debug scaffolding that shipped as behaviour (found and fixed 2026-09-14)

Three investigation leftovers had reached `main` and were only caught while
chasing the effects-wave1 video loss. Recorded here because each one is a
process failure, not a typo:

| Leftover | Effect on the shipped build | Why the gates missed it |
|---|---|---|
| `EXPERIMENT: render everything inline` block with an unconditional `return` in `render_pipeline.rs` | The whole multithreaded PF-7 export path (chunking, bounded channels, pipelined readback) was unreachable; video export ran single-threaded. Kept compiling behind `#[allow(unreachable_code)]` | Committed as a `chore` about stage dumps, so the behaviour change was invisible in review |
| Ungated `[stages] render_view bright=` probe in `render_and_filter_scene_to_view` | A full-canvas GPU readback plus `device.poll(Wait)` on every filter scope, every frame, in release | Ungated sibling of an env-gated probe; perf was not part of the commit checklist |
| `can_post_composite_filter` bypassed by passing `true` at the root loop | A mid-scene `Filter` scope would be blitted after the main render, covering later siblings; the predicate and its tests stayed behind as dead code | The tests pinned the predicate, never the call site that had to use it |

Fixes: the experiment and both probes were removed, the z-order precondition was
wired back into the root loop, and `AGENTS.md` gained three guards — the
`#[allow(unreachable_code)]` ban, a perf-bench requirement for hot-path
changes, and the "instrumentation is additive or it does not land" rule.

---

## Property descriptors and render contexts (2026-09-14)

Two structural refactors from the same review pass.

**The property list is declared once.** `animatix-core::property::PROPERTY_DESCRIPTORS`
now holds name, `Applicable`, and the finite `PropertyValueKind` for all 105
built-in properties, with the row index as the serialized `PropertyId`. The
parser derives its specs from it plus a type-system view; the engine keeps a
bindings-only table and composes `PROPERTY_REGISTRY` by joining the two. The old
one-way superset test (whose value-kind mapping ended in `_ => Generic`) is
replaced by total checks: binding↔descriptor membership, a pinned list of
descriptors without bindings, and total `ValueType → kind` and `Type → kind`
mappings that must agree with the descriptor.

That change paid for itself immediately by finding two bugs the weak test could
not see: `solo` had been registered with a `Generic` kind while the engine
stores `Bool` (the earlier drift failure had been "fixed" by changing the wrong
side), and `solo` was missing from `common_property_names()` — a test that had
been failing since `solo` landed. The engine's never-read `group` /
`GroupMembership` / `GroupHandlerId` vocabulary was deleted in the same pass.

**The render recursion takes contexts.** `evaluate_node` and its friends went
from fourteen positional arguments to seven by bundling the frame-invariant
state (`RenderFrame`) and the mutable outputs (`RenderOutputs`), with
`with_scene` for the container strategies that draw into an offscreen
sub-scene. `RenderChildrenCtx` keeps its documented public fields (extension
authors read them) and assembles the contexts in its two render methods.

**Process lesson (scripted rewrites).** Moving that 97-row engine table was
done with a parser + rewrite script. The first version found zero rows and wrote
its (empty) output anyway, wiping the table; it was restored from git within the
same minute. The fix is the rule for any future table rewrite: assert the parsed
row count *before* writing, never after.

**Process lesson.** The full-suite check used to summarize
`cargo test` output with a field-splitting `awk` over `test result:` lines. A
failing suite prints `test result: FAILED. N passed; M failed`, whose fields
shift, so the summary reported "0 failed" while `animatix-syntax` was failing —
which is how the `common_property_names` failure survived a claimed green run.
Count `FAILED` lines instead of summing columns.

---

## Redundant property aliases removed (2026-09-14)

The property table carried eight names the runtime could not store. Measured
before removal, on a real scene:

| Name | Behaviour | Real content use |
|---|---|---|
| `content` | worked (second name for `TextContent`) | 53 files, all rewritable to `text` |
| `stroke_color` | worked (second name for `stroke`) | 1 file |
| `fill`, `radius`, `start`, `end`, `function` | **silently ignored** — an actor declaring only `fill` drew nothing, `radius: 18` left square corners, `start`/`end` never moved a `Line`, `function` never reached the plot | 0 |
| `language` | no consumer at all (`Code` renders plain text) | 0 |

All eight are gone from the descriptor table and the parser's type table, so the
analyzer now reports `unknown-property` instead of accepting a value the runtime
drops on the floor — a direct violation of the "never silently drop" rule that
had been shipping unnoticed.

Along the way the text body was unified: `text` is now applicable to every
text-like actor (it previously listed only `Text` and `Math`, which left `Typst`
with no body property the analyzer recognised once `content` was gone), `code`
remains the `Code` spelling, and the unused `latex`/`math` aliases went with the
other eight. The runtime's per-kind spelling table and the frame-time override
lookups were narrowed to match, so analyzer and runtime agree on one set.

Removing `latex`/`math` shifted the ids of every row after them (`transform`
85 → 83). Nothing in-tree writes built-in ids to disk — autosave stores source
text and carry bags normalize by name on injection — so no migration is needed;
`property_id_order_is_pinned` now records the new values and explains why.

`Code` syntax highlighting is tracked in the roadmap: it needs a `language`
property back, plus tokenizer selection and theme-aware token colours.

---

## `Rect` corner radius (2026-09-14)

`Rect` gained `corner_radius` (F32, default `0`, scene pixels), animatable like
any other property and clamped to half the shorter side so an over-large value
becomes a stadium rather than a self-intersecting path. A rounded `Rect` used as
a `clip_shape` clips to the rounded outline, because `clip_path` derives from the
same render commands.

It travels the shape-property route rather than the tagged-property one:
`RectPrimitive::apply_property` lands the authored value on `RectState`, the
build seeds that state from the existing track (so a re-declaration that omits
the property keeps its radius) and `insert_end_keyframes` writes it into
`ShapeTracks::corner_radius` — the same path `arc_angles` takes. `KurboShape`
gained a `RoundedRect` variant; the PF-6 shape→path memo keys on the sampled
shape, so a radius change rebuilds the path with no extra bookkeeping.

Tests: a geometric unit test (a corner point is outside the path at radius > 0,
inside at 0, bounds unchanged, radius clamped) and an end-to-end render test
(rasterized corner empty, edge and interior painted).

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

---

## Demo Gallery Redesign (2026-08-21 → 2026-08-25)

Five phases shipped on `feat/demo-gallery` / `feat/demo-gallery-p3` and merged
into `main` (both branches appear in `git branch --merged main`). Source plan:
`docs/demo_gallery_plan.md`.

| Phase | Deliverable | Outcome |
|---|---|---|
| 1 | Shared `lib/` design system + `theme_studio.amx` | Done; engine workarounds documented in the plan (positioned components wrapped in `Group`, Text wrapped in `Group` inside `Col`) |
| 2 | `motion_poster.amx` + `dashboard_story.amx` | Done 2026-08-24; engine fixes landed with it (see "Resolved Engine Bugs (gallery-era)" above) |
| 3 | `epicycles.amx` + `sorting_theatre.amx` | Done 2026-08-25; `sorting_theatre` uses `dynamic_layout`, build-time sort precomputation, and `swap` actions |
| 4 | `brand_reel/` capstone | Done 2026-08-25; all six `play` transitions, `persist`, audio, cross-file scenes; multi-scene zero-duration bug fixed |
| 5 | Tutorial refurbishment + README matrix + `scripts/check_examples.sh` | Done 2026-08-25; `animation/16_showcase.amx` and `composition/20_feature_reel.amx` superseded by the gallery |

---

## Built-in effects, wave 1 (2026-09-13)

`Sharpen`, `Vignette`, `MotionBlur`, `Grain`, and `Levels` shipped, one file
each in `animatix-std/src/effects/` plus one `EFFECTS` entry — the
catalog-as-single-source promise held: no parser contract row, no enum variant,
no dispatch arm, and the GUI palette picked them up from `EFFECTS` untouched.

- `Sharpen` — unsharp mask (box kernel, `amount`, `radius` px); `support` =
  radius, so the derived ROI pads correctly.
- `Vignette` — aspect-corrected edge falloff (`amount`, `radius`, `softness`,
  `color` as `Vec4`, the first built-in vec4 parameter exercising the 16-byte
  alignment gap in the packing rule).
- `MotionBlur` — directional smear (`length` px, `angle` degrees) sampled
  through the linear sampler; alpha keeps its maximum so smears never erode
  silhouettes.
- `Grain` — PCG-hash noise keyed on `(pixel, seed, frame)`; the frame index
  derives from the context's `time_ms`, so it animates without authoring.
- `Levels` — in-black/in-white/gamma/out-black/out-white per channel.

Each effect has a content-level GPU test in `animatix-render`
(`vignette_darkens_corners_not_center`, `levels_black_point_maps_below_floor_to_black`,
`sharpen_overshoots_at_hard_edges`, `grain_perturbs_flat_field_deterministically`,
`motion_blur_smears_horizontally`), and the std packing test was generalised to
allow alignment gaps (Vignette's vec4 exposed that the old sequential-offset
model contradicted the documented host rule).

## Entrance opacity and declared duration (2026-09-29)

Two silently-dropped values, found while rebuilding the web demo's scenes. Both
were cases where the documented contract and the implementation disagreed, with
no diagnostic either way.

**Entrance actions disagreed about an authored `opacity`.** `FadeIn::execute`
ended every non-hidden target at a hardcoded 1.0, so an authored
`opacity: 0.25` was discarded; `wipe-in`, `draw-in` and `reveal-in` never wrote
`opacity` at all, so an authored `opacity: 0` kept the target invisible for the
whole timeline. Measured with GPU readback of a one-rect scene (frame average of
the red channel: 10 = invisible, 19 = at 0.25, 48 = at 1.0): `opacity: 0.25`
ended at 1.0 under `fade-in` and 0.25 under the other three; `opacity: 0.0`
inverted — 1.0 under `fade-in`, invisible under the other three. The in-repo
blast radius was measured before choosing: 15 actors author `opacity: 0.0` as a
"start hidden" seed (a naive "animate to the authored value" fix would have made
all 15 permanently invisible) and exactly 2 — `examples/animation/08_effects.amx`
`bg_mark` (0.08) and `examples/projects/fft_explain.amx` `playhead` (0.7) — were
being flattened to 1.0. The rule now: an entrance action settles on the authored
opacity, except an authored 0, which all four actions treat as a seed and lift to
1.0. Shared helpers `entrance_opacity_target` / `reveal_authored_zero_opacity`
live in `timeline/actions/mod.rs` so the four actions cannot drift apart again.
Zero in-repo cases hit the mirror-image bug (a reveal action on an
`opacity: 0.0` actor), which is why it went unnoticed.

**`config { duration: N }` was inert on single-scene files.**
`Timeline::duration_seconds()` took the maximum keyframe time, and the only code
that read the config key was the composition path's
`extract_duration_from_config`; the scene-config validator allow-lists `duration`
as scene-scoped, so it looked supported. The six transformer scenes declared
6.0/6.8/7.2/6.2/7.4/7.0 s and ran 5.5/5.1/6.55/5.3/6.45/6.15 s. `spec.md` had
documented the intended semantics all along ("overrides keyframe-inferred
duration"), so this was the single-scene equivalent of a contract the
composition path already honoured. The timeline now carries the declared value
(`apply_config_settings`, beside `resolution`) and exposes
`playback_duration_seconds()`; `BuildTarget::duration_s()` and the web host read
that, while editing surfaces (the inspector timeline strip, the keyframe table)
keep `duration_seconds()` so every keyframe stays reachable in the editor.
Because overriding means content past the duration is unreachable, the build now
warns `duration-shorter-than-content` rather than truncating in silence — checked
against the whole corpus, no file in the repo triggers it (an earlier
regex-based estimate suggesting otherwise was wrong: it added the largest
animation duration to the largest keyframe time and overshot).

Two things this left behind, both filed in `roadmap.md`: unknown `config` keys
are still ignored without a warning, and the `--slim` profile builds with
`dead_code` warnings from the rich-text helpers it compiles out.

## Web player: transitions, assets, runtime fonts (2026-09-29)

The three "not wired yet" items from the WASM port, plus the config-key catalog.

**Multi-scene transitions blend in the browser.** `render_frame` used to cut to
the incoming scene when `composition.evaluate` returned a blend. The player now
renders both scenes into their own offscreen targets and runs the same
`TransitionCompositor` the GUI preview and the export path use, presenting the
composited output. Two new offscreen targets exist only while a multi-scene
document is loaded, and the incoming scene gets its own filter backend so the
two evaluations cannot stomp each other's pass state.

**Assets fetch.** The engine's asset path was synchronous `std::fs` with no
injection point, and the web player passed `None` for the asset cache — an
`Image`/`Svg` actor was a hard `MediaLoadFailure` error. `AssetCache` gained
`insert_svg_source`/`insert_image_bytes` (keys normalised like every load), the
player exposes `list_asset_urls` (literal `url` properties on `Image`/`Svg`
actors, walked from the parsed AST) and `load_source_with_assets`, and the
embed fetches the URLs relative to the scene file before building. Build-time
asset loading stays synchronous; the async boundary is the JS pre-pass.
Missing assets keep the desktop semantics (a build error), and `--slim` builds
still report the feature gate for image/SVG.

**Fonts can be supplied at runtime.** The web sandbox has no system fonts, so
the only faces were the bundled Open Sans set — measured: CJK text renders as
a pure background frame with zero diagnostics while Latin and Cyrillic render
(open-sans covers both). `FontContext::load_font_bytes` builds a private copy
of the shared font database, registers the face, and bumps the font epoch so
memoized text compiles invalidate; `AmxPlayer::add_font` plus the embed's
`data-fonts` attribute (TTF/OTF URLs) feed it before the scene compiles. No CJK
font is bundled by choice — a page supplies what it needs, and the payload
stays small.

**`config` keys are a single-source catalog.** The key list was hand-written in
four places that had already drifted: the timeline build's unknown-key
allow-list, the composition path's `SCENE_SCOPED_KEYS`, `docs/spec.md`'s scope
table (which omitted `text_fast_path` and `export_preset` entirely), and the
unknown-key warning's message. `animatix-syntax` gained `config_keys.rs`
(`CONFIG_KEYS` with name/scope/value-kind/summary), and the engine consumers
derive their sets from it; a test parses the spec table and fails on drift, in
the pattern `catalog.rs` already used for the primitives checklist.

Along the way the two paths through `web.rs` (`render_frame` for the canvas and
`debug_readback` for diagnostics) were collapsed into one `render_document` —
they were the same target/match logic twice, and a transition fix would have
had to be made twice. `render_document` returns the frame target; the canvas
path blits its view, the readback path copies its texture to a buffer.

Roadmap impact: the unknown-config-key and slim-warning items are resolved
(above); a new item records the missing-glyph silence (`⋮`, `ᵀ`, `ₖ` render as
tofu with no diagnostic, and the same silence hides missing CJK faces).

## The wasm-SVG item that never was (2026-09-29)

The roadmap carried "`Svg` actors render empty on wasm" since the asset-fetch
work: a fetched SVG built fine (no diagnostics) while the player drew nothing,
and the same file rendered natively through the CLI. Closed as **not
reproducible** — both probes that "confirmed" it were measuring their own
scene, not the engine.

The stage-by-stage bisection (now `AmxPlayer::debug_svg_stats(t_ms)`, plus the
`web/demos/svg-probe/` pages) found every stage correct on wasm32: the asset
cache held the parsed paths, the track held them, `svg_paths_at(t)` evaluated
them at opacity 1, and the vello encoding at the probe time contained the
draws — the GPU rasterized them, and both a plain-`<rect>` and a
`<circle>+<rect>` SVG render correctly in the browser.

Two probe traps produced the phantom. First, the probe scene declared its own
backdrop before the first keyframe with no explicit `opacity`, so the
hidden-by-default seed swallowed everything and the frame was the bare theme
background. Second, after fixing that, the frame average was read against the
wrong baseline: backdrop-only is (247, 249, 255) while backdrop+logo is
(242, 234, 239) — the logo *was* in the average — and the confirmation
screenshot happened to land at t=0.1 of the loop, before the logo's 0.2 s
fade-in began. A screenshot of a looping embed pins whatever moment is on
screen, not the moment you meant to sample; pin frames with
`debug_readback(t)`, and compute the backdrop-only baseline before calling an
average "empty".

## Transition frames keep their filter scopes (2026-09-29)

`OffscreenRenderer::render_transition_to_output` evaluated both scenes with
`filter_backend = None`, so every exported transition frame silently dropped
the `Filter` scopes the preview (and the single-scene export path) applied —
a scene blurred in the preview exported sharp inside its own transition. The
path now runs the same per-target tail as the single-scene path
(`render_timeline_into_view`): each scene evaluates with its own lazily-built
`GpuFilterBackend` (second slot `filter_backend_b`, sharing one dimension key
so a resize drops both), renders, and blits its pending zero-readback
composites onto its own texture before the compositor blends. A regression
test pins it: at progress 0 the transition frame must match the single-scene
render of the outgoing scene channel-for-channel (pre-fix the red channel sat
~9 points apart on the blur fixture).

The same audit found the web player's `render_timeline` passing a filter
backend but never draining `take_pending_composites()` — the GUI preview's
blit tail was missing there — so a last-root `Filter` scope that took the
zero-readback path would have vanished from web playback too. Fixed in the
same commit; the web transition targets (one backend per scene, already
correct since the transition work) now run the identical tail.

## Build-time warnings for tofu text and dropped properties (2026-09-29)

Two silences closed in one pass — both were cases where the build succeeded,
the scene "worked", and a value quietly never reached the screen.

**Missing glyphs warn `missing-glyph` at build time.** `Text` rendered tofu
for characters no face covers (`⋮` U+22EE, `ᵀ` U+1D40 and `ₖ` U+2096 garbled
the demo's `softmax(QKᵀ / √dₖ)` formula with no diagnostic anywhere). The
probe `animatix_text::missing_text_glyphs` mirrors `compile_text_cached`'s
path selection exactly (rich-text cfg included): fast-path text is checked
against the family face plus the bundled fallback set, and everything else
goes through the Typst engine, whose fallback chain draws from the whole
registered database — so the probe checks the db when the Typst path governs.
Finding the gates exposed a wrong assumption in the original roadmap note:
`ᵀ`/`ₖ` sit outside `is_latin_text`, so the demo formula was *Typst-path*
tofu, not fast-path tofu. The declaration build pushes one warning naming the
actor and each uncovered character with its codepoint. U+2065 (permanently
unassigned, in-gate) makes the engine test machine-independent; the probe
test runs the Typst branch against a controlled one-face database because the
real context carries the system's fonts.

**Unknown declaration properties warn `unknown-property` at build time.**
A typo'd property (`colour:`, `opasity:`, `font_sise:`) was dropped by every
consumer in silence — the scene built, rendered, and just lacked the value;
only the analyzer's soft "not commonly used" info hinted at it. One helper
(`warn_unknown_declaration_properties`) now guards every declaration surface
(built-in shapes/containers, extension primitives, media, audio, text-like):
a name is known when the property registry binds it, the primitive declares
it, or an extension registry declares it for the type — all single-source
tables, so the warning cannot drift from what the build reads. Run against
the whole corpus (`examples/`, `dogfood/`, `web/demos/`) it produces zero
warnings; the four hand-written typos above each produce exactly one.

## Web scenes can import fetched modules; a diagnostic-snippet panic (2026-09-30)

**`import` now works beyond the bundled library.** The web build used to stop
at the first `SourcesOnly` miss with a generic module error, so only the
compile-time-embedded `examples/lib/*.amx` imports resolved. The load now
reports the module graph's resolved key (the import string joined onto the
importing file's directory and normalized) in `LoadResultDto::missing_imports`
with an actionable diagnostic; the embed fetches that path relative to the
scene, registers it via the new `AmxPlayer::add_module`, and retries —
transitive imports close over repeated rounds (bounded at 24, failed fetches
remembered). A host test walks a two-level import chain through the protocol
round by round; `web/demos/svg-probe/imports.html` exercises it in the browser
against `pkg`. Asset urls inside imported modules resolve against the module's
own location but the engine keys the asset cache by the literal url string, so
asset names share one flat namespace across the scene and its imports (by
design, matching the single-document desktop model).

**`animatix check` panicked on multi-byte sources.** `extract_source_snippet`
sliced the source at the diagnostic span's raw byte offsets; a span ending
inside a multi-byte character (an em-dash in a comment under a parse error was
enough) panicked `byte index is not a char boundary`. Both ends now snap to
the nearest boundary and a regression test holds it.

**Slim-profile truth table, verified in the browser:** a slim build does not
error on an `Svg` actor — the primitive itself is compiled out, so the actor
gets an `unknown-actor-type` *warning* and the scene renders without it,
while an `Image` actor (primitive registered, decoder gated) still fails the
build with `MediaLoadFailure`. `web/README.md`'s new differences table says
which.

## Per-embed engine profile and build quality (2026-09-30)

The engine choice (slim vs full wasm) and the build fidelity (Draft vs
Production) used to be page-level or hardcoded: `data-runtime-base` on the
loader script picked one engine directory for the whole page, and the player
built every scene at `BuildQuality::Draft`.

Both are now per-element `<amx-player>` attributes. `profile="slim"` (default)
| `"full"` selects the engine build — engine module promises are cached per
resolved directory, so embeds sharing a profile share one download and device
while a mixed page holds one context per profile (each wasm-bindgen instance
owns its thread-local engine context, which is what makes per-element selection
possible at all). `quality="draft"` (default) | `"preview"` | `"production"`
flows through the new `AmxPlayer::set_quality` into the build; quality is a
build-time knob, so changing the attribute rebuilds the scene.

`data-runtime-base` is reinterpreted as a directory *pair*: the legacy
exact-directory form (ending in `pkg`/`pkg-slim`) derives its ±slim sibling,
anything else is a parent containing both — existing pages are unaffected, and
omitting everything resolves to slim with the other profile as fallback.

Verification: `web/demos/svg-probe/profiles.html` (four embeds — slim default,
profile=full, quality draft, quality production) runs in the browser with both
wasm instances fetched and each scene correct; runtime attribute flips
(quality → rebuild, profile → second engine download) both return to ready.
The quality knob's invariant is pinned at the geometry level in
`animatix-render`'s offscreen tests (rose curve: draft 65 vs production 129
path elements) — its first draft asserted a *pixel* difference and failed with
exact equality, which is the honest finding that a smooth curve can rasterize
identically at both tolerances; the pixel gap is content-dependent and the
docs say so.

## The blit pipeline vs the browser's canvas format (2026-09-30)

Firefox (experimental WebGPU) failed on every present with
`Incompatible color attachments: the RenderPass uses textures with formats
[Bgra8Unorm] but the RenderPipeline 'Animatix Fullscreen Blit Pipeline' uses
attachments with formats [Rgba8Unorm]`. Not a Firefox limitation — WebGPU
requires a pipeline's color-target format to match the attachment exactly, and
the blit pipeline had `Rgba8Unorm` hardcoded while the canvas surface format
comes from `capabilities.formats.first()`: Chromium happens to order
`Rgba8Unorm` first on this machine, Firefox's wgpu orders `Bgra8Unorm` first,
so the same code was one format ordering away from failing everywhere.

Two layers: `FullscreenBlitPipeline::new` now takes the target format, and
`RendererCore::blit_texture_to_format` compiles (and caches) a pipeline per
non-internal target format — internal targets stay on the `Rgba8Unorm`
pipeline. The web player additionally *prefers* `Rgba8Unorm` when the surface
supports it (configuring the canvas to our format instead of adopting the
backend's), so the common path never needs a variant. Regression test
`blits_into_bgra8_targets` renders through a `Bgra8Unorm` target with
`on_uncaptured_error` → panic and asserts the swizzled pixels land.

## The authored-opacity gate, and the demo lib that ships inside the wasm (2026-09-30)

The transformer walkthrough grew a seventh scene (a bottom-up architecture map
with section tags), in-scene phase labels, and a shared `LabeledBox` component.
Three engine behaviours shaped (and had been silently shaping) the scenes:

**An authored non-zero `opacity` is visible from frame 0, whatever entrance
follows.** `wipe-in`/`draw-in` gate only the fill and stroke channels; the
hidden-by-default seed exists solely when no `opacity` is authored, and
`fade-in`'s `(start, 0)` keyframe only backfills the first keyframe — before it,
tracks read their default (1.0). A probe scene settled it: authored 0.9 +
`wipe-in`, authored 0.6 + `draw-in`, and authored 0.35 + `fade-in` are all fully
visible at t=0 (and a later fade pops down before rising). Every dimmed actor in
the demo scenes is therefore authored `opacity: 0.0` — the seed — and settles on
its value through a keyframe assignment (`rail.opacity = 0.40 [560ms]`) at its
beat; assignments do NOT lift the hidden-by-default seed (only entrance actions
do), so full-opacity actors still need a real entrance. The old demo had been
living with this: attention's query arrow and scan rule were on screen from
frame 0 of every loop, and the `d → 4d → d` label dropped both arrows in the
slim build (U+2192 has no glyph in the bundled face — ASCII `->` now).

**A component instance with an authored `opacity: 0` goes invisible.**
`fade-in`'s authored-zero path sets only the instance's own opacity track and
never lifts the hidden seed on the component's children, so a `LabeledBox` with
authored 0 fades in as nothing. The seed path (no authored opacity) cascades the
reveal into the subtree. Rule: never author opacity on a component instance you
intend to reveal.

**`examples/lib/*.amx` is embedded at wasm build time.** The web player
resolves `import "../lib/…" ` from the copy baked into `animatix_web_bg.wasm`
(`bundled_library()`), not from the server: adding `LabeledBox` to
`components.amx` made every chip render `Unknown actor type 'LabeledBox'` until
`scripts/build-web.sh --slim` was rerun. The scene sources themselves are served
live (no-cache), so the failure shows up only in the browser, hours after the
edit, with a veil message as the only clue. The demo scenes reach the lib
through a symlink (`web/demos/transformer/lib/components.amx`) so the CLI's
disk-based import resolution and the wasm's bundled keys agree.

The scenes were also converted to layout containers (`Grid` for the positional
matrix and the head panels, `Col` for the embedding columns, `Row` for the
concat segments) — one bottom-aligned `Row` attempt was reverted: `align: "end"`
plants children on the *container's* bottom edge (centre plus half the tallest
child), which is the hand-computed constant it was meant to remove, one step
removed.

## A failed build replaced the live scene (2026-10-01)

The `<amx-player>` live editor advertises a specific safety property: a failed
build throws with `.diagnostics` attached and **the previous scene keeps
playing**, so a typo never blanks the figure. The JS half honoured it
(`applySource` throws before touching the canvas or the clock), and the host
API documented it (`BuiltDocument::target` is `None` when the source cannot be
built; the caller keeps rendering the last known good document).

The host did not. `build_document_with_modules` returned `target: Some(..)`
whenever the source *parsed*, even when the build produced error diagnostics —
and the wasm shell installs whatever target it is handed. Driven through the
editor's own **Apply** button in a real headless Chromium (flake `.#web`,
`agent-browser --webgpu`), a one-line typo (`DoesNotExist` as an actor type)
turned a rendering figure into a black one: the GPU readback went from
`1280x720, 53 distinct colours` to `1920x1080, 1 distinct colour` — the broken
source's default resolution, with the element's canvas (990x557),
`aspect-ratio` and scrubbed duration still describing the scene that was no
longer loaded. A probe of the shipped dev bundle confirmed the mechanism
(`scene_width()` flipped 1280 → 1920 while `has_document()` stayed true), which
is what made the failure look like "the editor broke the figure" rather than
"the build failed".

The fix is one predicate in `crates/animatix-web/src/host.rs`: `ok == false`
yields no target, so the shell's existing `if let Some(target)` keeps the
previous document. Error diagnostics still carry the extent and markers the
build saw, so the shell can still say where it stopped; warnings
(`never-revealed`, `unused-label`) keep the document installable, which is what
every shipped demo scene relies on. Two tests pin both halves
(`build_failure_reports_diagnostics_without_target`,
`warnings_still_yield_a_document`).

Worth stating because the desktop behaves differently on purpose: the CLI logs
an error diagnostic and still renders the partial document (`animatix image`
writes a PNG for a scene with an unknown actor type). That tolerance is a
desktop affordance; the web player has no way to present a half-built document,
so for it "failed build" must mean "keep what is on screen".

Note for anyone reproducing locally: `web/pkg-slim` is a build artifact. The
JavaScript-side fixes in this pass take effect on reload, but this one needs
`scripts/build-web.sh --slim` (and CI's Pages job rebuilds both profiles on any
push touching `crates/animatix-web/**`).

## The instrument redesign: web/ as a living timeline (2026-10-01 → 2026-10-02)

The site's "light editorial" skin was rebuilt into the approved direction:
**the page itself is an Animatix timeline**. Palette, chrome, player API,
every scene, and the engine bugs the honest frames exposed.

**Design system (`web/demos/lib/theme.amx`, scheme `ink`).** Scene background
equals the site `--bg` (`#0b0e14`) so plates melt into the page; bone
`#ece7db` text; amber `#f5b942` is the brand lead, coral `#ff8666` a one-beat
drama voice, sage `#8fc7a3` / steel `#8ab4f8` rationed to genuine diagram
semantics. The old "gallery" scheme was retired. Registration idiom: only the
**unaliased** `import "…/theme.amx"` flattens `pub let ink` into the file —
the aliased `as theme` form (which the epicycles/sorting group carried) never
registers the scheme, so those ten files had been rendering default-dark
while claiming ink.

**Timeline chrome (`web/site-chrome.js`).** The nav carries a scroll playhead
(ruler with one tick per section, diamond head, `#12.4s` timecode chip: one
beat of 4 s per section); section heads stamp their master time; ledger rows
stamp in via IntersectionObserver; `amx-player[data-hoverplay]` poster cards
play on hover/focus; the homepage duality figure scrubs with scroll and lights
the code line owning the current beat; gallery/theater uses the Fullscreen
API. One regression shipped unnoticed: `.site-inner` (wordmark, links,
timecode) was built but never appended to the nav — every page rendered a
bare ruler strip until the headless DOM pass of 2026-10-02.

**Player API (`web/embed/src/amx-player.js`).** `sealed` (page-driven: no
center button, no canvas gestures, resumes on re-entering the viewport),
`fit="cover"` (stage unlocks from its aspect box — the full-bleed hero),
the bubbling `amxready` event plus `duration`/`time` getters, and `play()`
from the poster frame restarting at 0.

**Engine fix: cross-type morphs resolved per frame.** A same-label
re-declaration that changes actor type called `set_identity`, and the frame
path keyed the primitive on `actor_type` — the morph target owned the whole
timeline (a Rect re-declared Ellipse drew as a circle from birth),
falsifying every cross-type morph on the site. `render_type_name(time_ms)`
now resolves from the `shape_type` track inside the vector-shape family
only, with the "changes over time" verdict memoized on the track (the
un-memoized scan measured ~+7% on `scrub_layout_scene_100frames` in the
perf-bench A/B). The pre-fix baseline had been saved; the compare gate
(120 benches, 0 regressions) had to be replayed outside the dev shell —
`perf-bench.sh`'s compare step needs python3, which `nix develop` does not
provide, a gap that also silently killed the first compare run.

**Serial scene review pipeline.** Every scene group was frame-audited by
subagents (one at a time): keyframe stamps + mid-beat offsets rendered to
contact sheets, then per-keyframe verdicts and prioritized .amx fixes,
re-rendered and re-verified until PASS. Production rules that emerged:
casts ≥25% of frame width; per-beat on-screen captions; amber finale unify;
a ~350 ms reverse-order outro so loops wrap from a quiet plate; coral only
for one beat. The pass also exposed engine behaviours now documented or
roadmapped: `pulse intensity` is additive (1.04 = 2× scale); `Arrow` paints
only from `stroke_color` (a `color:`-authored arrow is silently grey);
an `import` inside a scene block freezes `always` clocks (a hub card was a
static poster of a rotation that never happened); single-line `text_align`
does not move the anchor; container + child `fade-in` settles the child at
the mid-lift opacity (~17%); `ContourSet`/`VectorField` ignore `opacity`
outright (0.05 renders identical to 1.0 — dim backdrops go by colour
instead); a static keyframe `.text =` assignment overprints the declared
and assigned strings simultaneously (per-query actors are the workaround);
`Graph.map()` maps math coords at half the px/unit the plotted curve uses
(a ball tracking `map(f(t))` rides beside its own trail — double the coords
to land on the stroke); Graph/BarChart axes render pure white under
`dynamic_layout`; and state-shape morph spans swap silhouettes instead of
interpolating (the tour's morph scene now teaches point-matched Polygons,
where every span is a measured hybrid).

**Acceptance notes.** Headless Chromium (flake `.#web`, `--webgpu`) cannot
exercise rAF-driven furniture (no compositor frames — screenshots show the
canvas region white while `debug_readback` proves the scene renders: hero
avg (17,19,24) / 50 distinct colours), so scroll-wiring verdicts rest on
DOM probes + the readback path; the no-WebGPU veil was confirmed by running
without the flag ("no WebGPU adapter available" surfaces, no white screen).

## The Easing Pass — one vocabulary for the site's motion (2026-10-02)

Started as a bug report ("the bounce animation on the page is cheap") and
ended as a pass over the easing library, the `bounce` action, the formatter,
and every content scene.

**Why the bounce was cheap.** `Easing` is a scalar `progress → progress` map
applied to the lerp between two keyframe values, so it can only modulate
travel *along the line between them*. `ease: bounce` on
`ball.at = (1360,726)` from `(240,726)` therefore cannot make an arc — the
endpoints share a y, so the curve has nowhere to put the hop, and what you get
is the ball jerking sideways past its target. The name invited it: there is
also a `bounce` **action**, which does move through space. Five uses in the
hero plate, and the most-seen motion on the site was a wobble wearing a
physics name.

**What the library gained.** `expo-out` and `expo-in-out` (the corpus had 155
annotations on a *quadratic* ease-out and no fast-start/long-settle curve at
all — `expo` was expo-in), `spring(damping, frequency)` as a damped oscillator
that overshoots once and rings down onto its target, and `ease:
cubic-bezier(x1,y1,x2,y2)` / `ease: spring(6,9)` call syntax. The bezier
variant and its evaluator already existed; `extract_easing` only accepted a
bare identifier, so `ease: custom` had been silently resolving to fixed
default control points ≈ ease-in-out.

**Four tables, now one.** Name→curve mappings lived in
`animatix-syntax::easing`, `animatix::timeline::timing` (a shadowing copy that
had already fallen behind — it lacked `custom`, so the editor offered an
easing the build layer rejected and quietly ignored), the GUI's
`easing_display_name`, and the GUI's source-edit table which overwrote any
custom curve with `linear` on save. All four resolve through
`parse_easing_name` / `easing_source_form` / `easing_to_expr` now, pinned by
`every_registry_id_parses`, `every_curve_holds_its_endpoints`,
`source_form_round_trips`, `parameterized_eases_check_their_arity`,
`engine_easing_names_cover_the_syntax_registry` and `ease_roundtrip.rs`.
Registry ids became the hyphenated forms the corpus actually writes, so the
GUI stopped emitting `easeout` into hand-authored files.

**The formatter deleted eases.** `Stmt::Assignment` keeps its easing in a
typed field that the parser lifts *out* of the modifier list, and the printer
walked only the list — so `animatix fmt` turned
`[1.2s, ease: ease-out]` into `[1.2s]`. Statement-count roundtrip tests could
not see it. Fixed, with `eases_survive_the_source_roundtrip` as the guard.
(The formatter still destroys comments — see `docs/roadmap.md`.)

**The `bounce` action is now gravity.** It wrote three `motion_offset`
keyframes — down `intensity`, up 30% of it, settle — one overshoot wearing the
name. It is now a decaying series: hop *n* rises `intensity·r^(2n)` and stays
airborne `d₀·rⁿ`, so contacts fall closer and closer together, with
`restitution` as the handle and the hops scaled to fill the duration exactly.
Rising decelerates and falling accelerates, which *is* the parabola; and
because it stays on the additive `motion_offset` channel it composes under a
positional `at` keyframe instead of fighting it. The hero plate's ball moved
onto it, replacing a hand-solved `always` block with two lines.

**Action modifiers from their signatures.** `parse_timing_modifiers` tolerated
effect keys through a hardcoded six-name list, so `bounce [restitution: …]`
warned about a modifier the bounce signature declares. The signatures are now
the source, with tests for both directions (declared keys silent, typos loud).

**Content pass.** 42 scenes audited: 155 of 207 explicit easings were the same
curve and ~880 timed statements carried none at all, interpolating Linear.
Adopted a vocabulary — entrances `expo-out`, exits `ease-in`, emphasis
landings `spring`, measurement motion `linear` — applied mechanically to
entrance/exit verbs, then reviewed scene by scene from rendered keyframes.
That review found and fixed 20 layout and correctness defects; see the
`fix(animatix): clear the collisions…` commit. The worst were the matrix
scenes drawing +y downward while their live readout said `(cos, sin)` —
the diagram contradicting the numbers for an entire scene — and morph spans
whose point counts differed, collapsing into a blob mid-interpolation.

**Acceptance notes.** All 17 pages × 3 viewports (1280/820/420) pass a DOM
layout probe with zero page-overflow, off-screen, or clipped-text findings;
the probe found the nav playhead hanging half off the edge at scroll 0 and a
`figcaption` nowrap flex row that squeezed the caption to 12px and poured its
text over the source link on every demo page below ~500px. The wasm bundles
are gitignored and CI-built (`pages.yml`), and this environment's dev-shell
`rustc` cannot see the rustup-installed wasm32 std, so the new curves are
verified through the native renderer and the shared evaluator rather than a
locally rebuilt browser bundle; a stale bundle degrades them to linear rather
than erroring.

---

## Phase 2 handoff — closed out (2026-10-03)

The phase-2 gallery handoff (removed 2026-10-04 as redundant with this section —
recover it from git history if needed) left six candidates for "Phase 3". Five
turned out to be already closed by later sessions; the sixth — `BarChart`
missing from the `gap` applicability row — was still open and is now fixed. The
handoff recorded, for each item, the commit and the test that fails if the fix
is undone rather than prose claiming it works; that evidence lives in the
commits and tests named below.

### The BarChart `gap` row

`docs/primitives.md` documents `gap` as bar spacing, `build/plot.rs` has read it
since the primitive shipped, and every other BarChart-specific property already
had a row — but `gap`'s row listed only the layout containers. Consequences:
`animatix check` emitted an info `unknown-property` for a line that works, and
the inspector did not offer it, because the inspector's list, plan-slot
filtering and that hint all derive from the same `Applicable` value.

The old handoff guessed this needed "a per-actor schema variant"; it did not,
because `Applicable` *is* already per-actor, and the runtime binding for `gap`
(`ActorField::ContainerLayoutGroup`) has no track storage at all — so adding the
type widened the surface without widening what the engine stores or can animate.
`chart.gap = 10` still fails loudly with `unsupported-assignment-property`,
exactly like `chart.bar_width = 6`.

The regression test is `bar_chart_gap_is_applicable_to_bar_chart` in
`crates/animatix/tests/applicability_table_agrees_with_reads.rs`, verified to
fail when `BarChart` is removed from the row. It lives beside the generic
row-vs-primitive test rather than inside it because BarChart has no primitive
file — the chart's properties are read by the shared plot builder, which the
generic test deliberately does not attribute.

### What it turned up

The type table is one `Type` per property name, so `gap: "auto"`, `bar_width:
"auto"`, `max_value: "auto"` and `show_axis: "true"` are all reported as
`type-mismatch` even though their builders accept those strings. Left open in
`roadmap.md` instead of patched, because widening `gap`'s type row would also
silence the only signal that a `Row` dropped an unreadable `gap`.

## The Motion-Vocabulary Round — Landed Work (2026-10-04 → 10-05)

> The round's remaining work and its item-by-item feasibility verdicts
> live in [`handoff_motion_vocab.md`](handoff_motion_vocab.md); this is what
> shipped. Batches are per session, all local and unpushed at the time of
> writing.

### Batch 5 (2026-10-04, fifth session) — eleven commits, local only

| Commit | Item | Evidence it landed |
|---|---|---|
| `889c6149` | A delayed `camera.zoom` no longer zeroes the scene until its stamp | `a_delayed_camera_write_holds_its_identity_until_its_stamp` (IDENTITY at t=0 and t=999, the authored 2× at t=1500); the *general* fix is recorded in the message as not free — seeding `max_height`'s `INFINITY` identity makes the 1 ms preserve segment interpolate to `NaN`, which `test_write_read_roundtrip_max_height` catches |
| `b329b19c` | **Authored `Filter` bounds follow the camera** (the gap batch 4 measured and left open) | `dogfood/probe_camera_scopes.amx`: authored vs the same scene with `bounds:` deleted went 73,512 differing pixels at t=1.6 → **0**, with the 9-pixel padding floor at t=0.2 unchanged; `filter_bounds_follow_the_camera` covers the mapping; the full 120-bench compare is quoted in the message |
| `a8647000` | The analyzer's exemption vocabulary is built once per process, not per keystroke | 111 + 2 analyzer tests unchanged; measured **neutral** on the bench (129.4 → 129.2 µs) and kept on that basis, not as a perf claim |
| `6d4b08c5` | BarChart builder split into `BarChartLayout` + `paths_for`, the precondition for animating `data` | Pixel-identical renders of two bar-chart scenes before and after, plus the existing fifteen `bar_chart` tests |
| this commit | **#23 the BarChart race** — `data = {…}` keyframes, bars matched by label | `a_data_assignment_records_a_transition`, `bar_data_interpolation_matches_by_label`, `bar_data_at_walks_the_transition_list`, `bar_geometry_follows_the_data_transition` (bbox heights at t=0 / mid / after); `examples/data/27_bars_race.amx`; the assignment that used to be an `unsupported-assignment-property` error now checks clean |
| `aee62252` | **`always-overrides-keyframes` no longer reads a declaration seed as animation** (item 4) | `is_property_animated` asks for ≥2 keyframes with differing values; three new source-level tests pin seeded-`dash_offset` silent, constant-declared-`size` silent, real `#1s b.size = (140, 80) [1s]` still warning; `check examples/animation/36_light_pack.amx` no longer mentions `dash_offset` |
| `39bbf79a` | The loop lint samples the camera axes, and a camera-only scene stops measuring zero length | `seamless_loop_lints_camera_axes`; `check` on a camera-only loop scene now names `camera.zoom`, and the same scene with the push returned to 1.0 stays silent |
| `757b5b70` | Docs closeout: the six-scheme table, `architecture.md`'s stale scheme list, `roadmap.md`'s effects section, the plugin ABI's sixth binding | Swept for numbers and lists of vocabulary the tests cannot see; the roadmap now says what landed and why a chain `Mix` is deliberately not on the list |
| `1105234a` | The handoff's claim that `animatix verify` checks ink in scene space was wrong | Settled from the code and from `b329b19c`'s 0-pixel agreement, not a new render |
| `f65b497c` | The tour now teaches the round: new §06 "Light & camera", and §05's effects scene gets its bloom | Both scenes `check`-clean; four frames of the new scene rendered and measured (15% → 29% → 32% → 51% content as the layers land); the bloom beat moves the sphere's two unsaturated channels (255,214,13) → (255,255,28); `never-revealed` caught the first draft's invisible bulb |
| this commit | Docs closeout: six-scheme table, `architecture.md`'s stale scheme list, `roadmap.md`'s effects section, the plugin ABI's binding note, and #24/#25 rescoped | See the notes below |

Notes the next session will want from batch 5:

- **`analyzer_update/small` is a real, unattributed +10.7%.** It has now
  reproduced across four runs against the 2026-10-02 baseline with a *tight*
  within-run spread (129.2 µs ±1.4% vs a 116.8 µs baseline), while
  `analyzer_update/dogfood` and `/large` — same code path, bigger inputs — read
  11-15% *faster*. So it is a fixed-cost effect on the smallest input, not noise,
  and not machine drift. Two candidates were measured out: the per-update
  exemption set (`a8647000` made it static; the number moved 0.2%) and
  `raw_property_types()` (it sits inside a `get_or_init`, so it was never
  per-update). The next step is a profiler on `Analyzer::update` for
  `examples/basics/00_hello.amx`; no profiler was available in this environment
  (`perf`/`valgrind` both absent).
- **The guard's own noise bound on that bench is useless.** Across runs of the
  same tree its limit has ranged 5.0% → 84.8%, because the bound is built from
  whichever `std_dev` that run happened to measure. Treat a flag there as a
  question ("re-run isolated and look at the spread"), never as a verdict.
- **#24 was mis-scoped and #25 was mis-blamed.** Variable-font weights need three
  changes, not an asset swap (see the inventory row); the vello pin's gate is not
  upstream's issue tracker but our own `vello_img_probe`, which needs a rev bump
  and a quiet machine. Both rows now say that.
- **Docs drift is wider than the code you changed.** This round's effect additions
  left stale counts in four places the tests cannot see: the homepage's "13 GPU
  effects"/"19 built-in actions", the tour's "Thirteen ship built in",
  `architecture.md`'s three-scheme list, and `roadmap.md` still describing the ABI
  bump as blocked. `docs/extension_authoring.md` also still told plugin authors the
  binding layout had five entries. Sweep the site and docs for *numbers and lists
  of vocabulary* after any addition — `grep` for the old count is the whole method.

### Batch 4 (2026-10-04, fourth session) — seven commits, local only

| Commit | Item | Evidence it landed |
|---|---|---|
| `b27ca5e5` | **#21 ABI v2** — effect passes get the pre-chain original at `binding: 5` — plus `Bloom` as its first consumer | Built the CLI with and without the bump (`strings` confirms only the new binary carries the `Animatix Filter Original` texture label) and rendered nine filter-bearing frames: every PNG byte-identical, and the 35 pixel-asserting GPU tests in `filter_backend::tests` pass against the v2 layout unchanged. `dogfood/probe_bloom.amx` measures the payoff: the bulb goes (167,142,15) → (255,255,24) and 32 px away the plate lifts (11,16,23) → (21,30,44) |
| `a046cbf6` | `DropShadow.softness` — sixteen golden-angle taps over a disc, hard case kept structurally | `examples/animation/30_effects_catalog.amx` md5-identical across the change (`53af058e0f0013f7ee09f427215693de`); `drop_shadow_softness_spreads_the_silhouette` reads alpha 0 at x=12 with `softness: 0` and 20..200 with `softness: 6` |
| `455f403b` | Eleven builtins the engine answered and the language never declared, plus `LIST_FUNCTIONS` | `git show HEAD~:crates/animatix-syntax/src/builtins.rs` matches none of the thirteen names; `check` on a scene calling all thirteen now reports no `unresolved-variable` at all |
| `e7fa1a1b` | `docs/spec.md`'s built-in math line rewritten to match the declaration table | The docs listed a subset, which is how the undeclared names stayed unnoticed |
| `7d568e64` | The site's recipes gallery — decision 8's third preset layer | `web/recipes/index.html` + seven scenes, all `check`-clean and all rendered at t=2.6 s with content measured (0.97% of frame for a dashed rule, 43.6% for the bloom stage) |
| `d4bfdd25` | `docs/spec.md`'s `format` example used a placeholder form the engine never had | Rendered proof of the grammar: `named {x}` stays literal, `plain {}` yields `12.345`, `{:.0}` yields `12`, `{:.2}` yields `12.35`, `{:,}` stays literal. A grep for the class over `examples/`, `web/`, `docs/`, `dogfood/` found no other use |
| `22b3632f` | The batch-4 record, plus the camera/authored-bounds limit this batch then fixed | The probe's header carries the 9 px / 73,512 px measurement that `b329b19c` closed |

Notes the next session will want from batch 4:

- **Correction (batch 6): the perf guard DOES see the GPU path.** This note said
  the opposite — that every bench stops at parse/build/scene evaluation and
  `render_scene_to_image_gpu_filtered` is in none of them. It is wrong:
  `crates/animatix/benches/demos_frame_cost.rs` is a criterion group
  (`demo_frame`) whose each sample runs
  `OffscreenRenderer::render_timeline` — evaluate → vello rasterize → **effect
  chains** → readback — and the last full compare carried 43 `demo_frame__*`
  rows. The real gap is narrower: the *engine* benches never construct a
  `GpuFilterBackend`, so an ABI change costs nothing there, while
  `demo_frame__*` does pay for scope count (and is exactly the family that
  inflated 2-4× under desktop load). Anyone adding a filter-path guard should
  extend `demos_frame_cost` with a multi-scope scene rather than write a new
  harness — the pipeline, the loader and the readback are already there.

### Batch 3 (2026-10-04, third session) — four commits, local only

| Commit | Item | Evidence it landed |
|---|---|---|
| `44bdba93` | The review pass's three engine gaps: plot decorations, `always`-block `size` doubling, `always`-block color text | `plot_actor_declarations_seed_dash_and_gradient`, `size_override_is_authored_in_full_extents`, `text_color_override_reaches_shape_style`; rendered probes measured to the pixel — `always { l.stroke = "#30d158" }` now paints (48,209,88), `always { r.size = (100.0, 60.0) }` now paints 100×60, and a `PlotCurve` with `stroke_gradient` shows the ramp it was dropping |
| `a0960f98` | **#17 scene camera** — `camera.at` / `camera.zoom` / `camera.rotation` | Eight tests in `timeline/tests/camera.rs` (identity when untouched, zoom about the center, pan-after-zoom, interpolation across a timed write, `always`-driven engagement, both diagnostic paths, the reserved label); `examples/animation/35_camera_moves.amx` verified by pixel position at t=2.6/4.6/9.0 |
| `6c40c243` | The `web/` visual review pass — 11 scenes re-lit | Rendered and read back: hero at t=4.6s, the tour's word reveal at t=1.3s, the `along` route at t=2.2s |
| `1727d558` | The reactive-frame cost batch 2 flagged and never dispositioned | `reactive_evaluate_100frames` +16.7% → **+5.0%** in isolation; `vello_path_stays_small_enough_to_clone_per_frame` pins the struct size |
| `9f344cdb` | The type table catching up with hex color strings, and two missing analyzer exemptions | `check` is silent on `color: "#ffe9c7"` in `34_particles_analytic.amx`, which it used to warn `type-mismatch` about |
| `2e170adf` | Decision 8's second preset layer — the `examples/lib` genre pack | `examples/lib/light.amx` (`KeyLight`, `CoolLight`, `MarchingRail`, `Ticker`, `Breather`) and `examples/animation/36_light_pack.amx`, rendered at t=2.0s: warm key behind the card, cool fill low-right, the ticker reading its 1284× target, the rail dashed |
| `e7d0b3ac` | Three scenes re-tuned after the `size` fix | The particles example reads as sparks again; `04_motion`'s orbit is centred on the plate rather than the origin (`scene.center.x` is an anchor, not an expression value); the tour's reticle breathes on `size` again |

Notes the next session will want from batch 3:

- **The three gaps were all the same bug shape**: the language declares a
  property as applicable to an actor, and one of the three write paths
  (declaration, keyframe assignment, frame-time override) or one of the two
  read paths (the generic shape renderer, the plot primitive) did not honor
  it. When adding vocabulary, the check to run is not "does my probe scene
  look right" but "does every actor × every write path × every read path
  carry it" — `animatix check` reported nothing for all three.
- **`geometry.size` is half-extents everywhere.** Declarations halve in
  `build/shape.rs`, keyframe assignments in `handle_size_assignment`, and now
  frame overrides in `primitives::override_size`. A new shape that reads
  `geometry.size` must treat it as half, and a new write path must halve.
- **`inject_property_into_env` is the hot path nobody watches.** It walks every
  INJECTABLE registry row for every actor every frame an env is built, and each
  row costs a read, an env write and an `animating_flag` key allocation. Adding
  a property with `F::ASSIGNABLE_AI` therefore costs real frame time on scenes
  that never use it — the reason six of batch 1/2's rows are now
  `ASSIGNABLE_A`. `dash_offset` stays injectable on purpose.
- **Camera semantics, in one line each:** pan is applied *after* zoom (so
  centring a station 400 px off-center at 1.7× takes 680), scene-anchored
  actors move with it, the background does not, and authoring a camera
  bypasses the static-subtree cache.
- **Eleven builtins the engine implemented and the language never declared.**
  Diffing `eval_shared`'s dispatch against `animatix-syntax::builtins` turned up
  `atan2, fract, hypot, ln, pow, rem, round, signum, step, deg_to_rad,
  rad_to_deg` absent from `MATH_FUNCTIONS`, and `list_set`/`list_swap` absent
  from every list. Those names resolve at runtime — `check` on a scene using
  them is silent — but the analyzer's `unresolved-variable` pass exempts only
  what `MATH_FUNCTIONS` declares, so the editor flags working, documented code
  (`ln(x)` and `list_swap(…)` are both in `docs/spec.md`) and completion never
  offers them. All eleven are declared now, `list_set`/`list_swap` in a new
  `LIST_FUNCTIONS` because they return a list and typing them `Num` would break
  the assignment the docs show. The diff is worth re-running after any builtin
  is added; nothing else catches it.
- **Tooling drifts behind a vocabulary addition in three places, not one.** When
  batch 2 made color strings resolve everywhere, `raw_property_types()` still
  said `Color` only, so `check` and the editor warned on source that rendered
  correctly (`9f344cdb`). The property-addition checklist covers the parser and
  the engine; the *type* row is where a widened value grammar has to follow, and
  nothing tests that a warning-free scene stays warning-free.
- **Still open from batch 2's notes**: `docs/effects.md` / `docs/primitives.md`
  now do cover dash/blend/gradient (done in `f00d3770`/`fc4dc879`). The
  `web/demos/posters/*.png` turned out not to need regenerating — nothing
  references them (the hub plays live embeds), so `web/README.md` was wrong
  rather than the images being stale.

### Batch 2 (2026-10-04, second session) — five commits, local only

| Commit | Item | Evidence it landed |
|---|---|---|
| `26665289` | #1 gradient fills/strokes, SVG ramp import, hex colors | `gradient_paints_are_parsed_stamped_and_interpolated`; three rewritten `svg_import` tests now assert the ramp survives import; rendered probe of all four forms in `dogfood/probe_gradient.amx` |
| `b4d8e0ac` | #7 + #9 `settle-in`, `pop-in`, `[anticipate: …]` | `settle_in_and_pop_in_ramp_scale_onto_the_authored_scale`, `anticipate_inserts_a_counter_move_before_the_travel` |
| `f69650d5` | #14 loop-perfect lint (`config { seamless_loop: true }` → `loop-not-seamless`) | `seamless_loop_lints_values_that_do_not_wrap` (both directions) |
| `adf0fcbc` | #15 CLI `--set NAME=VALUE` | `cli_defines_shadow_the_authored_let_defaults`; end-to-end render of a template with overridden text and color |
| `a05210b7` | #16 `#2b` beat stamps + `config { bpm }` | `beat_stamps_resolve_against_the_declared_tempo` (60/120/absent tempo) |

Notes the next session will want from batch 2:

- **Gradients are keyframable**, not static: two `linear()` values of the same
  kind and stop count interpolate geometry and colors. `gradient_extend:` and
  `gradient_space:` are separate scalar properties (default `pad` / `oklab`)
  applied by the stamp, so they are not per-ramp arguments.
- Ramps ride **outside** the shape-command memo, exactly like dash — an animated
  ramp would otherwise be served stale from a cached encoding.
- **Hex color strings now resolve anywhere a color is accepted**
  (`utils::color_from_text`). They previously fell through to the silent
  `[0.8, 0.8, 0.8, 1.0]` default, which is how the first gradient probe rendered
  gray. `lerp_color_oklab` also accepts color strings (frame-time `Value::Str`).
- **`pop-in`'s overshoot is an explicit intermediate keyframe**, not
  `Easing::Back`: the easing layer clamps a segment curve at its end, so a
  back-eased ramp peaks exactly at its target and never pops. Verified by test.
- **`anticipate:` is declared on the motion verbs' signatures**, not added to the
  universal `TIMING_KEYS` whitelist, so using it on another action reports
  `unsupported-modifier-key` instead of doing nothing silently.
- **`--set` shadows the authored `let`.** The first version re-applied the values
  *after* the statement walk and a `color: tint` fill stayed red — properties
  resolved during the walk never saw it. `process_body` now skips the authored
  assignment for a shadowed name.
- **`seamless_loop` is spelled that way** because `loop` is a reserved keyword.
  The lint is an engine post-build check (alongside `never-revealed`), so
  `check`, the GUI and the LSP all get it; only *keyframed* values are sampled —
  `always`-driven and plot-`func` values are frame-time functions and are the
  documented v1 skip.
- **Beat durations (`[2b]`) are not resolved** — only stamps. The modifier path
  has no scene tempo in scope; it reports `invalid-modifier-value` rather than
  guessing. Every non-build consumer (formatter, outline, analyzer, web marker
  strip, editor keyframe shift) resolves beats at the documented default 120 bpm.
- **Perf for #1** (`scripts/perf-bench.sh compare`, gradient-only tree, 120
  benches): `mixed_scene_evaluate` +5.03%, `reactive_evaluate_100frames` +13.55%,
  `stage__build_frame_env` +12.02% flagged — the same machine-drift families
  batch 1 flagged and cleared under isolation. `VelloPath` grew two
  `Option<GradientSpec>` fields, the plausible real cost. **Not dispositioned by
  an isolation re-run**; do that on a quiet machine before trusting it.
- **Docs debt partially paid**: `docs/properties.md` has the dash/blend rows plus
  a gradients section; `docs/spec.md` has the hex-color contract, the corrected
  "do not use hex strings" checklist line, the entrance-preset and anticipation
  prose, the `seamless_loop` and `bpm` config rows, a
  "Built-in Functions for Motion" table (noise family, `lerp_color_oklab`,
  `format`) and a Recipes section (count-up, seamless loop, light vocabulary).
  Still owed: `docs/effects.md` and `docs/primitives.md` for dash/blend/gradient,
  and the `vivid`/`paper`/`neon-night` scheme descriptions.
- **New examples**: `examples/animation/32_light_vocabulary.amx` (radial
  screen-blend wash, lit gradient bar, swept stroke, marching dashes, `fbm`
  drift, both new presets, anticipated move) and
  `examples/animation/33_count_up_ticker.amx`. Both render-verified. Note the
  trap found while writing them: an `always` block that writes a property the
  primitive *defaults* (e.g. `size` on a `Rect`) trips
  `always-overrides-keyframes` even though the author never animated it.

### Batch 1 (2026-10-04, first session)

Four commits on `main` (local only, unpushed as of 2026-10-04 evening):

| Commit | Item | Evidence it landed |
|---|---|---|
| `385f6e5d` | #6 `draw-in` really trims stroke-only shapes | `draw_in_trims_stroke_only_path_geometry_mid_draw`, `draw_in_cuts_a_straight_line_mid_segment` + 3 `path_progress` unit tests; trim assertions fail with the hook disabled (negative control). Hero probe at `--time 1.75` now draws the underline to the carriage (was full-width dim) |
| `6cb18912` | #4 noise family (`noise`/`noise2`/`seeded_noise`/`seeded_noise2`/`fbm`/`seeded_fbm`) + #10 `lerp_color_oklab` | `noise_family_and_oklab_lerp_agree_between_ir_and_ast` (IR/AST parity + value-level assertions incl. the red→green midpoint brightness property) |
| `049f9da2` | #3 `dash_pattern`/`dash_offset` + #2 per-actor `blend:` | `dash_pattern_stamps_paths_and_dash_offset_animates`, `blend_mode_is_bound_and_sampled`; visual probes: dashed/ants lines at two phases, `screen`-blend ellipse over a rect |
| `ef6a0c00` | #11 `vivid` / `paper` / `neon-night` colorschemes + GUI picker entries | colorscheme test battery passes; rendered probes for all three schemes look right |

### Implementation notes the next session will want

- **The property-addition checklist in practice** (dash + blend touched every
  one of these; use this as the canonical evidence chain): core descriptor row
  (append; pinned count `property_id_order_is_pinned` 97→99→100),
  `raw_property_types()` index-aligned row, `common_property_names()` if the
  row is `Applicable::Everything` (the sync test fails otherwise),
  `ValueType`/`ActorField` variants, `field→default` arm, the
  **name-sorted** `BINDINGS` insertion (`registry_is_sorted` fails if out of
  order — `blend` goes *after* `baseline`, first insertion was wrong),
  `Kind` mapping, `TrackFieldRef/Mut` arms + the three `name => Field` maps,
  `StyleTracks` fields, declaration seeding (`build/actor.rs` locals + prop
  arms + `insert_actor_keyframes`/`insert_end_keyframes` threading), and the
  plugin-ABI `PropertyValue` match in `extension_native_plugin.rs`.
- **DSL list literals use braces**, not brackets: `dash_pattern: {8, 6}` — a
  JSON-style `[8, 6]` is a parse error. Same shape as `points:`.
- **Dash rendering**: `VelloPath` grew `dash_pattern: Option<Vec<f32>>` /
  `dash_offset: f32`; the executor maps them onto kurbo `Stroke`. Dash rides
  *outside* the shape-command memo (an animated offset would be served stale
  from a cached encoding), stamped in `stamp_shape_dash`; a `draw-in` combines
  trim + fixed-length dashes. The static-subtree cache is safe because every
  dash-bearing actor has keyframes and is excluded there anyway.
- **Blend rendering**: node-level `push_layer` with an unbounded rect in
  `evaluate_node`, all 16 CSS mix-mode names parsed, unknown names warn and
  fall back to normal. The per-frame track probe is gated by
  `Timeline::blend_used` (refreshed at build end and in
  `invalidate_frame_cache`), so scenes that never author a blend pay one bool
  read. Known limits, worth revisiting: the layer is full-surface (not the
  node's bounds), `blend` on `Filter`/`Mask` scopes is untested, and there is
  no per-actor `Compose` control (vertical writing modes etc.).
- **F32List crossed the plugin ABI** as a plain `NATIVE_VALUE_LIST` of NUMs —
  no ABI bump needed; if plugins ever need to *author* dash patterns that path
  needs a proper type.
- **Perf dispositions for `049f9da2`** (hot-path rule): full-suite compare
  flagged `env_50`, `reactive_evaluate_100frames`, `text_rebuild` cold/warm;
  isolation re-runs of `env_*` and `reactive_evaluate` **PASS** (7.7–13.1%
  inside dynamic limits). `text_rebuild__mixed_48` was flagged at the same
  magnitude in the *first* full run of the session, before dash/blend touched
  anything text-shaped — machine-drift suspect (the suite's own history notes
  ±40% run-to-run swings on this box), and its warm arm reads 48.8% vs a 46%
  band. Not dispositioned by isolation; re-measure on a quiet machine before
  believing it.
- The `draw-in` fix changed `trim_path_by_progress` semantics from
  segment-count popping to arc-length cutting — plots benefit too (smoother
  curve trace-on). `docs/effects.md`/`docs/primitives.md` have not been
  updated for any of these four commits yet (docs closeout is item #5+#13).

### The brief, the references and the decisions

#### Why this round existed

The owner's brief: the demos and web pages are competent but *boring and dated*;
make it easier for users to produce lively, visually striking animation — by
adding language capability and/or adjusting defaults and design.

Three read-only surveys (web corpus, `examples/`+`dogfood/`, engine capability
inventory) plus an external study of two AI-animation skill repos converged on
one diagnosis: **the engine already "speaks" lively — the defaults and the corpus
don't use it, and the "light" vocabulary (gradients, blend modes, glow, noise,
camera) is supported by the pinned dependencies but not exposed.**

Evidence, in numbers:

- Web corpus: 652 entrances are `fade-in` + `expo-out`, 457 exits are
  `fade-out` + `ease-in`; `spring` appears 9 times and `bounce` twice across
  40+ scenes. Every loop ends with the same quiet-plate fade-out (the comment
  "Outro: quiet plate before the wrap" recurs verbatim in ~15 files).
- `examples/`+`dogfood/`: of 1,006 timed statements only 285 carry an ease —
  **72% animate linear**; `fade-in` is ~80% of all entrances; 76 of 87
  colorscheme declarations are the same two dark schemes; 2 `font_family`
  declarations in the entire corpus; zero gradients, shadows, glow, grain or
  texture in any `.amx`; the 13 shipped filter effects are used decoratively in
  ~6 files, nearly all of them scenes *about* the filter system.
- What the strong scenes prove (`gallery/epicycles.amx`, `web/demos/matrix`,
  `web/scenes/hero.amx`, `gallery/motion_poster.amx`): `always`-driven procedural
  motion, real `bounce` physics, morphs and stroke traces already produce
  landing-page-quality work. The gap is that nothing *default* points there.

## External reference points (2026-10-04 study)

Neither repo is an engine; both are **craft encoded as AI-agent skills**, which
is exactly what makes them useful to us — a demand list of named techniques and
numeric craft rules.

- **[iart-ai/motion-skills](https://github.com/iart-ai/motion-skills)** — 54
  skills / 17 packs (motion design, web animation, kinetic typography,
  explainer, data animation, maps, WebGL…), rendering via Remotion/manim/HTML,
  each skill carrying a deliver-and-verify loop.
- **[Unclecheng-li/AI-Animation-Skill](https://github.com/Unclecheng-li/AI-Animation-Skill)**
  — a Chinese workflow + 44 HTML templates for explainer decks (26 "Level2"
  templates organized by genre: VS-comparison, hierarchy, steps, code-case,
  warning; 14 flowchart templates). Its pain points are all "does not own the
  runtime" symptoms (cloneNode to replay CSS animations, "≥95% template
  similarity" rules) — our deterministic timeline is the structural advantage;
  the transferable idea is *templates organized by genre with a one-page SUMMARY*.

### The craft tables worth absorbing (from iart-ai `animation-principles` / `motion-background`)

- **Durations**: micro-interactions 100–200 ms; UI transitions 200–400 ms; hero/
  full-screen elements 400–800 ms; camera moves 800–2000 ms. Scale +30–50 % per
  doubling of distance/area ("a 20 px icon and an 800 px panel sharing a
  duration reads weightless") — the corpus's "everything is 500 ms" disease.
- **Easing by role**: enter ease-out `cubic-bezier(0.16,1,0.3,1)` (identical to
  the site CSS `--expo-out` — the taste exists, the language doesn't default to
  it); exit ease-in `(0.7,0,0.84,0)`; on-screen reposition ease-in-out
  `(0.65,0,0.35,1)`; playful overshoot `(0.34,1.56,0.64,1)`; **linear only for
  continuous loops**. "The eye forgives a slow start far less than a slow end."
- **Stagger**: lists 40–80 ms; dense grids 20–40 ms; cap a group reveal at
  ~600–800 ms total; direction follows the eye; ≤ ⅓ of elements in motion at
  once; no element travels > ⅓ of the screen without an intermediate keyframe.
- **The three principles that matter most for motion graphics**: anticipation
  (a 60–120 ms counter-move before the action), follow-through (attached
  elements settle 40–80 ms after the hero), arcs (straight-line travel "looks
  mechanical").
- **Anti-pattern diagnosis table** — reads like our corpus's chart: *stiff* →
  linear/symmetric enter easing; *floaty* → duration too long (cut 30 %);
  *cheap* → everything appears at once, or all elements share one duration;
  *mechanical despite easing* → missing anticipation/follow-through/arcs.
- **Loop craft**: `0%` and `100%` keyframes identical; motion built only from
  `sin`/`cos` of `(t % P)/P · 2τ` (and integer multiples) loops seamlessly; a
  `?t=N` freeze harness for deterministic screenshots.
- **Background recipes**: layered radial gradients with alpha colors over a dark
  base (the cheap one); value-noise + 5-octave fbm aurora mixing two colors via
  `smoothstep`; particle constellation (~80 points, distance-faded links). All
  three are product forms of the "light vocabulary" below.

## Found while probing: `draw-in` does not draw on non-plot shapes

**Fixed 2026-10-04 in `385f6e5d`** — the probe round also forced an upgrade:
the segment-count trim popped whole segments (a single-segment underline drew
all-or-nothing), so the trim is now arc-length aware and cuts inside a segment.
The section below is kept as the record of the defect.

**`draw-in` writes a `stroke_progress` track, but the only render-time consumer
of `stroke_progress` in the engine is the plot primitive.** A stroke-only
`Path` (the signature use) therefore *fades in at full width* — the draw is
fictional.

- Write side: `timeline/actions/reveal.rs:127` (`DrawIn` keys
  `style.stroke_progress` 0→1; the visible effect comes only from
  `lift_hidden_by_default`/`reveal_authored_zero_opacity`, i.e. an opacity fade).
- Read side: `primitives/plot.rs:91` is the only
  `track.style.stroke_progress.get(...)` in the tree; `primitives/path.rs:32-46`
  builds the VelloPath without progress; `primitives/mod.rs` mentions
  `stroke_progress` only in a doc comment (:1175). `trim_path_by_progress`
  (`timeline/path_progress.rs:14`) has exactly one caller.
- Probe (2026-10-04, `nix develop`, lavapipe):
  `cargo run -p animatix-cli -- image web/scenes/hero.amx --time 1.75 -o /tmp/hero_t175.png`
  (draw is 6.7 % in, expo-out ≈ 27 %): the underline renders **full-width and
  dim** — a fade, not a partial stroke. At `--time 1.95` it is full-width at
  full brightness. Re-run these two commands after the fix; the mid-draw frame
  must show a partial line.
- Consequences: the hero's carriage "rides the draw tip" over a tip that does
  not exist; `web/tour` teaches draw-in on plots (where it *does* work), so the
  tour hides the bug. **Fixing this is the prerequisite for the icon-set route**
  below, and it is user-visible on the flagship scene today.

Fix shape: consume `stroke_progress` in the shared shape render path (mirror
`plot.rs:91-99` via `trim_path_by_progress`), open strokes only — closed/filled
shapes must keep the fill-reveal semantics from `8e244595`.

## Item inventory and feasibility

Verdicts: TRIVIAL / SMALL / MEDIUM / LARGE, with the one or two things that make
each non-trivial. Verified 2026-10-04.

### M1 — light/color vocabulary + motion defaults

| # | Item | Verdict | Where the work is | Non-trivial part |
|---|---|---|---|---|
| 1 | Gradient fills/strokes (linear/radial/sweep) | MEDIUM | `RenderCommand::Paths` carries solid colors only (`primitives/mod.rs:968-1030`); peniko 0.6.1 at the pinned rev has full `GradientKind` support; SVG import flattens gradients to averaged solid (`svg_import.rs:63,115`) — un-flattening rides along | Animatable gradient params need a `ValueType` decision (ship static first); text paint excluded (the text pipeline degrades to solid RGBA) |
| 2 | Per-actor/scope `blend:` (16 peniko Mix modes) | ~~SMALL-MEDIUM~~ **DONE** `049f9da2` | see implementation notes | Node-level unbounded layer; scope-scoped blend and `Compose` control deferred |
| 3 | `dash_pattern` / `dash_offset` | ~~SMALL~~ **DONE** `049f9da2` | see implementation notes | Animated offset bypasses the command memo; list literal is brace form |
| 4 | `noise` / `fbm` / `seeded_noise` builtins | ~~TRIVIAL~~ **DONE** `6cb18912` | see implementation notes | Pure leaf functions; IR/AST parity pinned |
| 5 | Count-up tickers | **NOTHING TO BUILD** | `format("{}", …)` already exists (`eval_shared.rs:510-548`, `spec.md:938`) and `always { label.text = … }` recompiles glyphs per frame (`dispatch.rs:449-457`) | Deliverable is a recipe + example. Caveat to document: `geometry.size` is not remeasured from frame-time overrides, so a growing counter does not reflow its box |
| 6 | **Fix: `draw-in` trims non-plot shapes** | ~~SMALL-MEDIUM~~ **DONE** `385f6e5d` (plus the arc-length trim upgrade the first probe forced) | see implementation notes | Closed/filled shapes keep the `8e244595` fill-reveal semantics; hero re-probed |
| 7 | Preset actions + `anticipate:` / settle params | SMALL | New action = trait impl + `get_builtin_actions()` (`actions/mod.rs:367`) + syntax `ACTIONS` row (`builtins.rs:55-77`) + `action_documentation` (`:164-181`); a pinned test enforces runtime==syntax so drift fails the build | Pre-keyframes are established mechanics: `ensure_guard_keyframe` at `t-1` (`entrance.rs:59-62`), the plan-slot fence (`property_engine.rs:527-558`) — anticipation is the same move at `t-offset` (saturating at 0) |
| 8 | Default easing for un-eased statements | ~~SMALL code, LARGE blast radius~~ **DONE** `90285df1` + `ef3b400b` | the role-based default now lives in `default_easing_for` (`timeline/timing.rs`) | Landed narrower than proposed: entrances get `expo-out`, exits `ease-in`, reposition `ease-in-out`, oscillators stay linear — and **assignments stay linear**, because an action expands into assignments that inherit its timing modifiers and would double-ease a pre-baked curve (`bounce` regressed to scale 1.4375 against a 1.375 baseline). Corpus A/B run with two builds and a positive control |
| 9 | `settle` entrance (fade + slight scale) | SMALL | new action (preferred over changing `fade-in` semantics) | — |
| 10 | `lerp_color_oklab` | ~~TRIVIAL~~ **DONE** `6cb18912` (new name; `lerp_color` untouched; morph path deliberately not switched — decide separately if it should be) | — | — |
| 11 | New built-in colorschemes (incl. a vivid set) | ~~TRIVIAL data~~ **DONE** `ef6a0c00` (`vivid`/`paper`/`neon-night`) | — | GUI picker list updated in the same commit |

### M2 — make it visible (content, skin, verification tooling)

| # | Item | Verdict | Where the work is | Non-trivial part |
|---|---|---|---|---|
| 12 | Bundled stroke-icon set (Lucide-derived, ISC) | ~~MEDIUM~~ **DONE** `719d0c77` | the data-table route: `animatix-core::stroke_icons` (25 Lucide ISC paths) + an `icon:` property that expands into `Path` geometry | Landed with two real parser fixes in `svg_import::parse_svg_path_data` (compact numbers `7-7`/`.53.53`; implicit `line_to` after `M`/`m`) and the authored-`scale:` fix that makes an icon sizeable at all |
| 13 | Theme/vivid pack, display font, fast-path glyph gaps (ᵀ/ₖ tofu → ASCII math on the tour), site content redo, outro variety | content/web — **mostly landed**: schemes `ef6a0c00`, genre pack `2e170adf`, glyph gaps already closed by the `missing-glyph` warning + ASCII formulae | `docs/ai_agent_animation_quality.md`-adjacent authoring work | Pure content, but the glyph gap is an `animatix-text` fix |
| 14 | Loop-perfect lint (`check` warns when a loop does not wrap) | SMALL | `animatix check` already builds the full Timeline (`main.rs:1383-1465`); scalar/style tracks + plan slots are sampleable at any t (`read_property_plan_slot`, `property_engine.rs:561-566`) | v1 policy: skip `always`/plot-`func`-driven values (document it); define the loop boundary (scene end vs `play` loop point). Shares the facts exporter with `ai_agent_animation_quality.md` |
| 15 | CLI `--set name=value` (template × data batching) | SMALL engine + plumbing | inject `env.set` at the pre-walk seam (`build/entry.rs:399-407`) | ~10 build entry points thread a defines map (or canonicalize one); the analyzer needs the defines or false `unresolved` fires |
| 16 | Beat timestamps `#2b` + `config { bpm }` | SMALL-MEDIUM | lexing site `token.rs:380-387`; **ordering is a non-problem** (config pre-pass precedes stamp resolution, `entry.rs:421-443`; scene configs hoisted, `composition/build.rs:238-240`) | a new `ast::Time::Beats` fans out to exhaustive matches (`walk.rs:317`, analyzer `duplicates.rs:232`, GUI `ast_utils.rs:271-275`, formatter); bpm threads to `time_to_ms` (`utils.rs:839`) via builder state; `config_keys.rs` is test-pinned against spec.md (`:128-163`) so docs move in lockstep |

### M3 — camera, text, particles

| # | Item | Verdict | Where the work is | Non-trivial part |
|---|---|---|---|---|
| 17 | Scene camera (pan/zoom/shake, keyframable) | ~~MEDIUM~~ **DONE** (batch 3) | `timeline/camera.rs`: `camera.at` / `camera.zoom` / `camera.rotation`, routed in `assignments` before target resolution and applied as the parent transform of every root node | Landmines resolved as: the static-subtree cache is bypassed while `camera_used` (a cached encoding cannot be re-transformed on append), the background fill deliberately stays un-camerad, hit regions come out in screen space for free because the camera is in the node transform, and **scene-anchored actors do move** — documented as a limit, with no per-actor opt-out yet |
| 18 | Per-letter/word reveal (`draw-in [by: letter]`) | ~~MEDIUM~~ **DONE** at word granularity `bf820fef` | `draw-in [by: word]` reveals a text actor word by word through the existing per-`TextPath` command list | Per-*letter* still needs the `TextPath` per-glyph transform this row named (`animatix-text/src/lib.rs:20-28`); `by: letter` is deliberately rejected rather than silently doing the word thing |
| 19 | Particles (`burst` / `ambient`) | ~~MEDIUM~~ **DONE** `fdbdc5c0` | decision 3 went the analytic way: `examples/animation/34_particles_analytic.amx` is seeded `always` math (`seeded_noise(i, …)` for direction and speed, gravity as `age²`) — no new primitive, no state model | Determinism holds because there is no unseeded `rand()`. Follow-up this exposed: a `range(n)` builtin, so the index list is generated instead of spelled out |
| 20 | Motion-along-path (`move [along: …]`) | ~~MEDIUM (unverified)~~ **DONE** `00441684` | `run_move_along` samples the route with `trim_path_by_progress` (`sample_path_along`) and keys ~48 positions at one per 40 ms | Scoping-pass result: arc-length parameterisation was already there (from the #6 trim upgrade); `orient: true` turns the actor along the local tangent, and the sampler must be seeded from the first `MoveTo` because `trim_path_by_progress(path, 0.0)` yields an empty path with no current position |

### M4 — glow (the ABI bump) and the parked items

| # | Item | Verdict | Notes |
|---|---|---|---|
| 21 | Second-input-texture ABI bump → Bloom, soft DropShadow, chain Mix | **DONE** `b27ca5e5` + `a046cbf6` | ABI v2 binds the pre-chain original at `binding: 5` (`filter_backend.rs`), copied once per scope before pass 0. `Bloom` is its first consumer (`animatix-std/src/effects/bloom.rs`) and `DropShadow.softness` the second. "chain Mix" needed no new effect: `Bloom`'s `keep` parameter *is* a linear mix of the chain result with the original, so a second effect over the same math would be a duplicate |
| 22 | `glass` / backdrop-blur | LARGE, **parked, but now decision-ready** | Re-scoped in batch 6 against the pipeline rather than the summary. ABI v2 does **not** help: binding 5 is the *scope's own* pre-chain sub-scene, and glass needs the main target's pixels *below* the scope in z, which nothing in the frame ever exposes as a texture (`offscreen.rs:267-294` renders the whole scene in one pass; `filter_backend.rs:54-67` renders only sub-scenes into the backend's own targets). And the obvious shortcut — render the frame, sample the region, blit the above-glass content afterwards — is wrong for the common case, because a label sitting *on* a glass card would be swallowed by the composite. The shape that is correct: evaluate once, emit **two** vello scenes pivoted at the glass scope, render below→texA, copy texA's region as the chain's second input, render above→texB with a transparent `base_color`, blit texA then the glass result then texB. Two hard facts to design around: (a) `RendererCore` only ever hands vello a `RenderParams { base_color, .. }` (`core.rs:151-170`), i.e. every render clears its target, so "draw a second scene over the first in the same texture" does not exist here — the split must go to separate textures and be composited; (b) `EffectRegion` is a plain rect, so a rounded glass panel needs either a mask pass or rounded-region support. Cost: N glass scopes = N+1 vello renders per frame, and the cost has a guard already: `demos_frame_cost`'s `demo_frame__*` group renders
evaluate → rasterize → effect chains → readback per sample (batch 4's note claiming
the opposite is corrected above). Still also owed: compositing *below* children, and a decision on whether glass is one effect or a `Filter` variant |
| 23 | BarChart race | ~~MEDIUM-LARGE~~ **DONE** (batch 5) | Landed along the five steps as scoped — `BarDataTransition` beside `FuncTransition` (`timeline/plot.rs`), label matching in `interpolate_bar_data`, the frame-time sampler `bar_data_at` called from `evaluate_node`, and the layout kept on the track so a rebuild never re-parses properties — with **two corrections to the plan**. Step 4's memo is unnecessary: the rebuild is gated on `bar_data_transitions` being non-empty, so a chart that never animates its data pays one `is_empty()` per frame. And step 5 was a no-op — the `data` entry in `build/plot.rs:1160`'s skip list is the *declaration* walk, not the assignment path; removing it would have broken the builder. The assignment hooks in through `Primitive::handle_assignment`, the extension point that already existed for exactly this (eight primitives use it). Two documented limits: a changed label set **or order** warns, because captions are compiled at build into the declaration's slots, and `max_value: auto` normalises the tallest bar every frame, which hides the race unless the author pins it. A third branch the plan assumed turned out to be **dead**: `build/plot.rs` resolves the layout for every `BarChart` whether or not `data:` was declared, so assigning without a declaration is not an error — it is an empty `from`, bars entering from 0, with its own warning that there were no captions to compile. And it is the render, not the test suite, that proves label matching: bar tops read back from three frames of `examples/data/27_bars_race.amx` land on the interpolated values, with `api` overtaking `web` inside the first window and losing it inside the second.
| 24 | Variable-font weight animation | **half of it was never about the font** — the fast-path bug is fixed, the axis is blocked | Probing the premise (2026-10-05) found the visible defect one layer under the three changes this row listed: `compile_text_fast` (and its two siblings) resolved a bundled family by taking `BUNDLED_FONTS.iter().find(family == …)` — the family's **first entry**, always Regular — so `font_weight` and `font_style` were silent no-ops on plain Latin text, which is most of the repo's titles. `web/demos/epicycles/*.amx` ask for `font_weight: "bold"` and were drawing regular. Fixed by giving `BundledFont` its own `weight`/`style` and choosing style-first-then-nearest-weight like a font database (`bundled_face`), measured end to end: rendered ink for one 64pt line goes 8,975 at 400 → 14,363 at 700 (and at `"bold"`, and at 900, which lands on the nearest face), 8,613 for italic, 13,815 for bold-italic, with `fc-list` confirming this box has **no** Open Sans installed — so the bold is the bundled face. Two tests pin it, one of them through the compiler (`compile_text_fast_honours_weight_for_a_bundled_family`), and `Bold` is no longer `rich-text`-gated so the slim embed gets it too (+220 KB of face; the wasm was not rebuilt from this shell, so the bundle size in `web/README.md` is still the pre-change figure). What still blocks *continuous* weight: the stack cannot instance a variable axis at all. `fontdb` 0.23 exposes no variation API, so a VF registers as its default instance for both paths — verified by rendering an installed variable family (`Noto Sans CJK JP`) at 400/600/800: identical ink, 4,881 px. The candidate asset was fetched and examined rather than assumed (`OpenSans[wdth,wght].ttf` 532,636 B + italic 583,992 B, `wght 300–800`, `wdth 75–100`, 14 named instances) and is deliberately **not** committed: it would be larger than the four statics and still render one weight. Reopen when a dependency can set axis coordinates (or when instancing offline into named instances is acceptable — that trades one file for nine) |
| 25 | Vello pin lift | **attempted and answered — the blocker is a wgpu major, not upstream** | Batch 5 said this was not answerable from here because `gh` is absent, `WebFetch` is quota-blocked and search returns no status. True, and irrelevant: the probe is the gate, and the revision to probe for is `git ls-remote` away. Batch 6 did it. Upstream `main` is **`f3000c8d`**; this box's cargo cache tops out at `17166312`, whose parent is `c55a2b5e` "vello: Keep image atlas residency across renders (#1558)" — the change the pin avoids, sitting directly on the pinned `d8686d52`. Bumping all three `vello` entries to `f3000c8d` **does not compile**: `Renderer::new(device, …)` reports `expected vello::wgpu::Device, found wgpu::Device`, because that vello builds against wgpu 30 while the workspace pins `wgpu = "29.0.0"`. So lifting the pin needs a coordinated wgpu 29→30 bump first, and whether #1558's regression is fixed upstream remains unmeasured — the probe could not run on the new rev. While there: `tests/vello_img_probe.rs` **asserts** its A–H matrix now (it used to only `eprintln!` the counts, so the "gate" depended on a human reading numbers), passes on the pin under both GPU and `ANIMATIX_CPU_RENDER=1`, and was checked to fail with "only 0 of 921_600 canvas pixels have ink" when a draw is forced empty. Pin comments, `core.rs`'s dependency invariant and `docs/roadmap.md` all say what would actually lift it |

## Remaining work, next-session order

M1 (#1-#11) and M2 (#12-#16) are complete, as is M3 (#17-#20). What is left, in
the order that makes sense to attempt it:

1. ~~**#21 the second-input-texture ABI bump**~~ — **done** (`b27ca5e5`,
   `a046cbf6`). `web/recipes/scenes/bloom_stage.amx` demonstrates the pair on the
   site, and the content follow-up that sentence named is landed too: the tour's
   §05 `effects` scene now carries a `glow: Bloom` that goes up at 2.3 s and back
   down at 3.2 s (`f65b497c`), so the second input is taught where people learn
   the vocabulary, not only in the gallery.

2. **#22 glass** (needs mid-frame scene splitting that ABI v2 does *not*
   provide — the main vello target is still never an input texture; batch 6
   turned the row into a design: two vello scenes pivoted at the glass scope,
   because `render_to_texture` clears its target and cannot draw over an
   existing one). ~~#23 BarChart race~~ landed in batch 5, and **#24 font
   weights** split in batch 6 into a shipped bug fix (the plain-text fast path
   ignored `font_weight`/`font_style` for bundled families, so bold titles drew
   regular) and a blocked feature (continuous weight needs variable-axis
   instancing, which `fontdb` 0.23 cannot do — see its row).
   ~~#25 vello pin lift~~ was **attempted in batch 6 and answered**: the pin
   cannot move until wgpu moves 29→30 workspace-wide, which is a different job
   than this round's and now named in the pin, `core.rs` and `docs/roadmap.md`.
   None of the rest is a quiet afternoon.
3. **#13 closeout, mostly done.** Landed across the round: the theme/vivid pack
   (`ef6a0c00` schemes, `2e170adf` `examples/lib/light.amx` genre pack), the
   glyph-gap item — which turned out to be closed already: the build warns
   `missing-glyph` naming each uncovered character (`declarations_text.rs:523`,
   pinned by `build_diagnostics.rs:260`) and the tour's formula scenes are
   written in ASCII, so no tofu ships, and the third preset layer —
   `web/recipes/` (`7d568e64`), single-idea scenes with the source beside each —
   eight now that `bar_race` joined them for #23 — so `docs/spec.md`'s Recipes
   section and the site now point at the same
   vocabulary. Still owed: the site content redo beyond the review pass,
   a decision on `web/demos/posters/*.png` (eight 1280×720 stills nothing
   references any more — the hub plays live `data-hoverplay` embeds, and
   `web/README.md` now says so; **deleting them is the owner's call**), and
   pruning this handoff into `docs/history.md` when the round closes.

4. **`always-overrides-keyframes` fires on declaration seeds, not on
   animation.** `examples/animation/36_light_pack.amx` warns that the `always`
   inside `MarchingRail` writes `dash_offset` on actor `rail`, "which also has
   keyframe animation". The guess recorded here when it was filed — that the
   write lands on `rail.line` while the lint looks at `rail` — was wrong: a
   single-actor component's internal actor *is* the instance track, because
   `animatix-syntax/src/module/rewrite.rs` drops the root label when it rewrites
   `self.line.dash_offset`, so the target really is `["rail"]`. The cause is one
   level down: the lint asks `has_keyframes_for`, which is
   `keyframe_count() > 0`, and `insert_end_keyframes` stamps a constant keyframe
   for every declared property — `dash_offset` gets one at the scene end holding
   its default `0.0`. Nothing animates. That is the same root cause batch 2 noted
   for `size` on a `Rect`: a non-zero keyframe count says *declared*, not
   *animated*, and no lint should treat the two as the same question.

   **Fixed in batch 5.** `AnimationTrack::is_property_animated` asks whether the
   track holds at least two keyframes that do not all carry the same value, and
   the lint asks it; `examples/animation/36_light_pack.amx` is now silent about
   `dash_offset`. What is given up: a *single* authored keyframe of a constant
   value can no longer be flagged, because it is bit-for-bit what declaration
   seeding leaves — the seeder writes the **declared** value, not the registry
   default (measured: `size: (100, 60)` yields one keyframe at `(50.0, 30.0)`,
   halved as `geometry.size` requires). Separating them needs keyframe
   provenance, and the storage is a bare `BTreeMap<u64, (T, Easing)>` with nowhere
   to put it. If that case ever matters, add provenance to `add_keyframe`; do not
   add another value heuristic.

   Two related things this surfaced. One is **fixed in batch 6**; the other stays:
   - `check examples/animation/36_light_pack.amx` reported
     `unused-label: Unused binding: 'p'` at `36_light_pack.amx:1:1` — but `p` is
     `let p = clamp(t / 0.9, 0, 1)` inside `Ticker`'s `always` in
     `examples/lib/light.amx:69`, and it is used on the very next line. The guess
     recorded above was wrong twice: nothing runs over an expanded body (the
     analyzer and the module graph both collect references from the *same*
     statement list they take labels from — `symbol_table.rs:509` walks
     `Stmt::Always`/`Block`/`ComponentDef` bodies, and `Stmt::Assignment` collects
     its right-hand side), and the expander deliberately leaves `let` names alone
     (`module/expand.rs:641` excludes them from `known_labels`, with a comment
     saying so). The cause is one call away from the lint: **`SymbolTable::merge`
     copies `labels` and not the three sets the unused-label pass reads beside
     them**. A direct import flattens every private name in the library into the
     host's table, while `referenced_labels` stays the host's own — so the host is
     told the library's internals are dead code, attributed to its first line.
     `merge` now unions `referenced_labels`, `component_internal_labels` and
     `array_labels` as well. Measured: that one warning is gone, the same file's
     folded hint count drops 6 → 1 (five leaked library actors), and a sweep of
     every `.amx` under `examples/`, `web/` and `dogfood/` reports **no**
     `unused-label` at all. `resolve_symbols_carries_the_references_of_what_it_flattens`
     fails without the fix — checked by reverting it and re-running.
   - The `property → ActorField` table is duplicated **three** times in
     `timeline/dispatch.rs` (`has_keyframe_at`, `has_keyframes_for`,
     `list_keyframes`) — verified identical but for the fall-through arm. The new
     method avoided a fourth copy by resolving through the runtime
     `property_registry::lookup_property(…).field`; the three copies still want
     folding into one mapper.
5. ~~**The camera's known limits**~~ — one of the two is **closed in batch 6**.
   `camera_follow: false` pins a root actor outside the scene camera (the HUD
   that must not move), documented in `docs/spec.md`'s Scene Camera section and
   `docs/properties.md`, with `examples/animation/37_hud_overlay.amx` as the
   worked scene. The remaining limit is that the camera is not carried across
   scenes by `persistent`/carry-bag.

   Two things the implementation learned the hard way, both worth repeating:

   - **A declaration read in the generic property walk does not reach every
     actor.** Text, Media, Audio, the plot primitives and extensions build
     through their own `process_*_decl` and return before that walk — the first
     version worked on a `Rect` and silently ignored the same flag on a `Text`,
     which is precisely the actor that wants it. The read now lives in
     `process_body`'s actor arm (`Timeline::apply_camera_follow_decl`), the one
     place every family passes through with its props in hand, and
     `camera_follow_reaches_the_actor_families_that_build_their_own_declaration`
     pins a `Text`, not a `Rect`. Any future build-only property should go there
     for the same reason.
   - **Only a render would have caught it.** The unit tests written first all
     passed while the feature was broken: they measured a `Rect`'s bounds. The
     example's first render showed the text overlay gone entirely — culled,
     because it had been camera'd to `y = -39` off the top of the frame. The
     measurement that now backs the feature: the pinned band (hud text plus its
     rule, 11,520 px) is **byte-identical** between t=0.6 s and t=2.6 s with 591
     ink pixels in both, while the plate band under the camera changes in
     101,165 of 195,000 pixels.

   A nested `camera_follow: false` is reported (`inapplicable-property`) instead
   of doing nothing quietly, and an opted-out `Filter` scope keeps its authored
   `bounds:` as screen coordinates — the rule sits inside
   `effect_scope_region`, which consults the track it is given, so it is unit
   tested rather than eyeballed.

   The other follow-up was already on this list and is still open: the camera is
   not carried across scenes by `persistent`/carry-bag.

   Two more were on this list and are now closed. The loop-perfect lint samples
   the camera axes (`Camera::seam_pairs`, named in the warning as `camera.zoom`);
   wiring it turned up a worse bug sitting next to it — `duration_seconds()` finds
   the content end by walking keyframe times over tracks, background and variable
   tracks, and had never been told the camera is content, so **a scene whose only
   motion is a push-in measured zero length** and the seam check skipped it
   entirely. `Camera::max_keyframe_time_ms` is now folded into that max.

   One follow-up this handoff used to list is **not** a gap: it claimed
   `animatix verify`'s ink checks were "in scene rather than screen space". They
   are not. `verify::bounds_map` reads `SceneProgram::precise_bounds`, which is
   documented and used as *world*-space, and its own doc line requires the
   observable renderer "so the bounds and the pixels come from the same
   evaluation" — so bounds and pixels are the same, camera-included space, and an
   actor the camera pushes off-frame clamps to an empty region and reads
   invisible, which is the right answer. The independent proof is the bounds work
   below: the derived filter region is built from those same subtree bounds, and
   it agreed with a camera-mapped authored rectangle to **0 pixels** at t=1.6.

   A `Filter` scope is correct under all three axes now, authored bounds
   included. A derived region never was a problem: the sub-scene renders with the
   node's `global_transform`, the region comes from the subtree bounds that
   transform recorded, and the composite blits back at that same
   `region.origin`. Authored `bounds: (x, y, w, h)` are scene coordinates, so
   `effect_scope_region` maps them through the camera affine before the support
   padding. Measured on `dogfood/probe_camera_scopes.amx`, authored against the
   same scene with `bounds:` deleted: **73,512 differing pixels at t=1.6 → 0**,
   with the 9-pixel padding floor at t=0.2 unchanged.

   Two things about that fix are worth keeping. It was scoped here as needing a
   field on `RenderFrame` plus its five construction sites, and that was wrong —
   `RenderFrame` already carries the frame's `overrides`, so the affine the root
   subtrees were wrapped in can be recomputed at the scope from the same inputs.
   And the first attempt passed the scope's own `global_transform`, which
   double-counted the scope's `at` and slid the region by (320, 180): 42,126
   differing pixels at t=0.2 where the answer is 9. The affine that belongs here
   is the camera's, not the node's.

   A delayed `camera.zoom` also used to zero the scene until its stamp arrived:
   the write helpers preserve the pre-stamp value, and for a track with no
   declaration behind it that fallback is `f32::default()` — `0.0` for zoom.
   Fixed in `889c6149`, where the *general* version of that fix is recorded as
   not free: `max_height`'s identity is `INFINITY` and the 1 ms preserve segment
   then interpolates to `NaN`.
6. **The residual reactive-frame cost is closed** (`1727d558`): the mechanism
   was `inject_property_into_env` walking every INJECTABLE row of every actor
   every frame, and six of batch 1/2's rows are now `ASSIGNABLE_A`. What is
   left is `dash_offset` — deliberately still injectable, because
   `(r.dash_offset + 0.4) % 17` is how marching ants are written — and the ~5%
   that remains on that bench, which is inside the run-to-run spread of this
   box. If it ever matters again, the structural fix is to inject lazily (only
   the properties a modifier program actually reads) rather than to keep
   demoting rows one at a time.

Milestones as originally proposed: M1 = items 1-11 (**complete**), M2 = 12-16
(**complete**), M3 = 17-20 (**complete**), M4 = 21-25 — #21, #23 landed, #25
answered (the pin needs a wgpu 29→30 bump, which is outside this round), leaving
#22 (glass) and #24 (variable weights) as the two unfinished features.

#### Relationship to the other documents

- `docs/handoff_silent_drops.md` — still the open handoff for WP6/WP7/WP8; this
  round does not touch those items, but property additions here must follow the
  descriptor discipline that round documented.
- `docs/ai_agent_animation_quality.md` — the deterministic scene-facts evaluator
  proposal. The lints in M2 (loop-perfect, motion-vocabulary, the anti-pattern
  table) should be expressed as facts in *that* exporter's schema rather than as
  a second, parallel checker.
- `docs/handoff_web_redesign.md` — kept as the locked design-direction reference
  (palette, chrome spec, review pipeline); the M2 content/skin work builds on it.

## Decisions (resolved 2026-10-04, with the choices each implementation made)

The owner approved executing the whole handoff; the decisions below are
recorded with what was actually chosen so they are not re-litigated:

1. **Default easing** — done (`90285df1` + `ef3b400b`), narrower than proposed:
   entrances `expo-out`, exits `ease-in`, reposition `ease-in-out`, oscillators
   and *assignments* stay linear, because an action expands into assignments that
   inherit its timing modifiers and would double-ease a pre-baked curve. Measured
   with the noise-floor A/B discipline (two builds, a positive control, a corpus
   run).
2. **Color interpolation** — new `lerp_color_oklab` name (non-breaking);
   `lerp_color` untouched; the morph path's u8 sRGB lerp deliberately not
   switched yet.
3. **Particles** — done (`fdbdc5c0`) in the seeded-analytic form: no new
   primitive, no state model, deterministic under scrubbing in both directions.
4. **Icons** — done (`719d0c77`) on the Lucide-derived data-table route,
   unblocked by the #6 `draw-in` fix (`385f6e5d`).
5. **Beat syntax** — done (`a05210b7`): `#2b` stamps and `config { bpm }`.
   Durations (`[2b]`) are deliberately *not* resolved — the modifier path has no
   scene tempo in scope and reports rather than guesses.
6. **`--set` semantics** — done (`adf0fcbc`): CLI defines shadow the authored
   `let` defaults by *skipping* the authored assignment during the walk, which is
   what the first attempt got wrong (a `color: tint` fill stayed red).
7. **Loop lint v1** — done (`f69650d5`), with the `always`/plot-`func` skip
   documented as designed; batch 5 added the scene camera's three axes
   (`39bbf79a`), which the track walk could not see.
8. **Presets** — all three layers: engine verbs (`b4d8e0ac`), `examples/lib`
   genre packs (`examples/lib/light.amx` + `examples/animation/36_light_pack.amx`,
   batch 3), site recipes gallery (`web/recipes/`, batch 4). All three layers
   exist; the tour's Recipes section in `docs/spec.md` and the page now name the
   same moves.
9. **Sequencing** — this round runs after the silent-drops gates each session;
   both tracks stay unpushed per the standing rule until the owner says push.

#### The gates this round held itself to

Standard `AGENTS.md` gates (fmt, check/clippy `--workspace --all-targets`,
syntax tests, serial lib tests) inside `nix develop`; `cog commit`; no push
without the owner. Round-specific additions:

- Anything that changes what every scene renders (default easing, OKLab,
  camera, draw-in fix) needs **same-binary + positive controls** per the
  noise-floor rules in `handoff_silent_drops.md` §Verification — no corpus pixel
  claims without them.
- Per-frame-path items (per-letter reveal, particles, dash/trim) need
  `scripts/perf-bench.sh compare` numbers in the commit message.
- The `draw-in` fix re-runs the hero probe (`--time 1.75` must show a partial
  stroke, not a dim full-width line).

<!-- Draft: batch-6 record section for docs/handoff_motion_vocab.md. Insert right
 before "### Batch 5 (2026-10-04, fifth session)". Fill __PERF__ after the gates. -->

### Batch 6 (2026-10-04 into 10-05, sixth and seventh sessions) — 18 commits, local only

Begins with what #23 made possible; `f12295ca` (the race itself) is in the
Batch 5 table above.

| Commit | Item | Evidence it landed |
|---|---|---|
| `aee62252` | #14's follow-up: `always-overrides-keyframes` fired on declaration seeds | `AnimationTrack::is_property_animated` asks for ≥2 keyframes that differ; `examples/animation/36_light_pack.amx` is silent about `dash_offset` |
| `11052342`/`39bbf79a` | the camera's two real gaps | `verify`'s ink checks were already camera-space (the handoff's claim was wrong, now corrected with the 0-pixel agreement as proof); `duration_seconds()` never counted camera keyframes, so a push-in-only scene measured zero length and the loop lint skipped it |
| `b329b19c`, `889c6149` | authored `Filter` bounds under the camera; a delayed `camera.zoom` snapped early | both found by the new tests, both pinned |
| `f12295ca`, `6d4b08c5` | #23 BarChart race | keyframable `data` with label matching; the dead `bar_layout.is_none()` branch and the useless step-5 "fix" the plan named are both recorded as not-real |
| `efeb50ae` | #13 | `bar_race` joined the recipes page (eighth scene) |
| `aa2513cf` | the `unused-label` false positive batch 5 left open | cause was `SymbolTable::merge` flattening labels but not `referenced_labels`/`component_internal_labels`/`array_labels`; a sweep of every `.amx` in the repo now reports no `unused-label` at all |
| `b5dc4bd3` | #25 | the vello probe **asserts** its A–H matrix; upstream `main` (`f3000c8d`) does not even compile here because it builds against wgpu 30 while the workspace pins 29 — the pin's real blocker, now written into all three `Cargo.toml`s |
| `10e8d38e`, `ded1b17d` | the camera's per-actor opt-out | `camera_follow: false`, with the two lessons in its row below |
| `4221ac36` | #24's real half | the plain-text fast path resolved a bundled family to its *first* `BUNDLED_FONTS` entry (always Regular), so `font_weight: "bold"` drew regular; ink for one 64 pt line goes 8,975 px at 400 → 14,363 at 700, with `fc-list` confirming no Open Sans is installed on this box |
| `43722523` + `d8ec31f6` | found while probing #26 | see "a bare action ate the next line" below — the first cut broke `swap bar1 bar2`, which the second fixed by bounding the list at the statement head instead of at a comma |
| `9c60862c` | #26 (the camera's last gap) | `persist camera` / `remove camera`, sticky like every other carry; the render A/B at t=3.2 s puts the title's ink at rows 111–160 and the subtitle's at 405–425, exactly `y' = 360 + 1.4·(y−360)` |
| `2f10b6d7` | doc defect found by the same probe | four action hovers advertised `move target to (x, y)`-style particles the engine never implemented |
| `288e6250` | #24 | `TypeEnv::with_stdlib()` copied ~60 `String` keys per construction, and `SymbolTable` builds one per annotation and per inferred expression; `analyzer_update/small` 137.38 → 129.57 µs |
| `fdb0f783` | **#22 `Glass`** | a container whose chain runs on a copy of the finished frame behind its rect; measured the way the round does everything else — a 4 px border authored on the scope reads (255,0,0) unfrosted and (93,27,32) frosted, which is why the scope paints nothing and the card is its child. Also found by the same probe: a `Glass` that was not the scene's **last root actor frosted nothing at all, silently**, because it inherited `can_post_composite_filter`'s last-position rule; `a_glass_scope_still_frosts_when_something_renders_after_it` fails without the fix (checked by reverting) |
| `1f44a217` | **#24's other half** — continuous `font_weight` | the nine-keyword collapse in `font_weight_to_typst` is gone (Typst takes an integer), and the Open Sans variable pair is registered so its `wght` axis gets instanced: compiled ink for one 64 pt line runs 510.67 / 516.35 / 522.05 / 527.73 / 533.35 / 544.63 / 555.88 px across 400…700 in 50 steps, pinned by `compile_text_instances_a_weight_no_static_face_ships`. Registering the pair only for non-canonical weights was tried first and the same ramp rejected it (450 drew heavier than 500; 599/600/601 = 533.2/553.8/533.5). Cost measured too: 496 of 921,600 pixels in `examples/layout/27_layout_text.amx`, no line rewrapped |
| `4ff90b68` | #22's applicability follow-through | the `ShapeKind::Rect` left on the catalog card was still telling `Applicable::AllShapes*` that a `Glass` scope takes `fill_opacity` and the gradient paints — on an actor that paints nothing, so the value vanished quietly. Without it the checker says so (`Glass never reads it, so the value is dropped`), pinned by `a_glass_scope_names_the_surface_properties_it_cannot_use`; `corner_radius` stays silent because the frost clip consumes it, and the re-rendered probe matches the earlier frame pixel for pixel, rounded corner included. `color` (`Applicable::Everything`) is the remaining hole, named in `docs/roadmap.md` with `Filter`/`Mask` sharing it |
| `f79099c1` | the `dispatch.rs` table fold this handoff kept noting | the three identical `property → ActorField` maps are one `Self::field_for_property` now, each caller keeping only its own fall-through and its `ImageData`/`Svg` case. Guarded the way the repo asks for a query-path change: three repeats of `scrub_layout_scene_100frames` on the folded tree (+1.49% / +1.10% / +4.41%, the bench's own spread is ~5%, so the single +6.52% reading is reported as the outlier it was), `sample_all_tracks` -1.62%, `stage__sample` -5.1…-7.6%, all seven `static_*` ok |
| `web(tour)` outro variety (#13) | four sections now close differently, each claim measured rather than asserted: `effects` loses detail before existence (edge energy −91.6%, 3.8% of the frame still content), `plots` exits by `shift` so the data leaves and the type stays crisp (content 56.8% → 12.6%, edges only −64.6%), `glass` fogs over into its own backdrop, `light_camera` ends on the key and fill. All four `animatix check` clean — which is how the first draft's undeclared `fade-out wash` and its content running past the declared `duration: 8.6s` got caught before they shipped |
| `770c9383` | the rest of that decision, as a mechanism | `color` was `Applicable::Everything`, so it bypassed every capability predicate and a scope-level colour disappeared without a word. New `Applicable::Except(&[names])` (the mirror of `Actors`) excludes `Glass`/`Filter`/`Mask` — verified against the primitives, none of which reads a style colour. Measured end to end: a scope written with `color`/`fill_opacity`/`stroke_width` now yields exactly three `inapplicable-property` warnings (`a_glass_scope_names_the_surface_properties_it_cannot_use` pins all three plus the silence around `corner_radius`), the corpus lint is unchanged at 3 errors / 18 warnings, and reverting either half of the surface decision fails the test |

**A bare action statement ate the next line** (found writing `persist camera`, fixed
in `43722523`). An action is `verb targets? args modifiers` and the grammar has no
newline tokens, so *both* the comma-less target list and the bare-args list were
unbounded repeats. `animatix ast` on `rotate t by 90 [500ms]` printed
`targets: ["t", "by"]` — the phantom-target reading — and `persist t` followed by
`t.opacity = 0.5 [200ms]` was a **parse error**, which is the natural order for the
carry. `dogfood/probe_persist_line.amx` is the canary: `repo_content_parses_clean`
lints every `.amx` in the repo, so that shape parses only while the guard holds.
Two chumsky traps worth remembering: a filter passed to `perf-bench.sh` is matched
against the criterion id *and* the `__`-joined collected id, so it must contain no
`/`; and wrapping a `repeated()` **item** in `.rewind()` trips the debug assertion
"Repeated combinator making no progress" — `.rewind()` belongs on the lookahead
only. Also `demos_frame_cost` ignores the criterion filter and sweeps every demo,
which is why each guard run costs minutes — and why it panics on any `.amx` in
`examples/` that no longer parses (a corpus canary in its own right).

**#24's verdict, replacing "open +10.7%".** `git bisect` over the 110 commits since
the 10-02 baseline (test: re-bench `analyzer_update/small`, bad above 128 µs) put
the jump between `385f6e5d` (+1.4%) and the mid-10-04 commits (+8.6…12.8%), then
resolved its last steps into noise — its nominal "first bad commit" is
`ea1d8827`, a rustfmt-only change, and the run-to-run spread measured 1.5%. Two
measurements do more than the bisect did: today's parser guard costs ~1.8% of the
+18%, and the *lexing* path is 7–10% **faster** than the same baseline
(`code_compile__*`), while `analyzer_update/large` is −15.3% and `/dogfood` −11.8%.
So the shape is a fixed per-update overhead that lands hardest on a 449-byte
fixture, not a hot path — and `288e6250` removed ~6 µs of it. What is left
(+10.9%) sits in the analyzer's post-parse passes; the next probe is an env-gated
stage timer inside `Analyzer::update`, not more bisecting.

**Still open after this batch:** #22's limits (its row, now in `docs/roadmap.md`), the #13 site-content redo
beyond the tour/recipes additions and the count sweep (the demo-poster PNGs remain
the owner's call), and two perf residuals the guard flagged against the 10-02
baseline: `analyzer_update__small` +10.4% and `property_plan_lookup_and_sample`
+17.3% on a 9.8 ns bench — reproduced in isolation, on a path the `Glass` commit
does not touch, and both left as named open questions rather than waved off as
noise.)

**#22's shape, as landed — and the two rows in this handoff that were wrong.** The
row said the correct design is *two vello scenes pivoted at the glass scope* and
that the shortcut ("sample the region, blit the above-glass content afterwards")
is wrong "because a label sitting *on* a glass card would be swallowed by the
composite". The implementation lands the pivot with **one** scene per side and a
flat layer queue instead of a `SceneProgram` restructure: evaluate records
`PendingLayer::Backdrop` + `PendingLayer::Composite` in declaration order, the
main render goes to the output texture as it always did, and `drain_pending_layers`
then (1) copies the padded region out of the finished target into the chain's own
scratch, (2) runs the chain, (3) blits the result back clipped to the panel's
rounded rect, (4) blits the children's sub-scene over it. The label is not
swallowed because it was never in the main scene: `render_glass_children` renders
children into `sub_scene` via `out.with_scene`.

What that design *does* cost, and what the row did not anticipate: the scope's own
surface. The frost is a copy of the finished frame, so anything the scope paints
itself is inside the pixels that get blurred — a 1.5 px card border at radius 18
is gone. So **`Glass` paints nothing of its own**, like every other container, and
the panel you see is the scope's first child (`examples/animation/38_glass_panel.amx`,
`web/recipes/scenes/glass_card.amx`, `web/tour/scenes/glass.amx` all write it that
way). `corner_radius` stays applicable to `Glass` because it shapes the frost's
clip; `stroke_width` is **not** (its actor list dropped `Glass` with the surface), but
`color` and `fill_opacity` still are: `Applicable::Everything` and
`AllShapesExceptLine` have no per-actor exclusion today, so a scope-level fill
compiles quietly and paints nothing. The `Glass` commit claimed a diagnostic that
does not fire; the correction is its own commit rather than a quiet rewrite, and
the gap is now in `docs/roadmap.md`.

Also wrong in the row, and corrected by the same measurement that prompted this:
`EffectRegion` is still a plain rect, but a rounded panel needed no mask pass —
the rounded clip is a signed-distance `coverage` term in the existing blit
fragment shader (`fullscreen_blit.rs`), which also grew a `src_rect` so a texture
covering the padded region can paint only the panel's window of it.

**#24's remaining half landed in this batch, and the reason it was still open was** `font_weight_to_typst`
(`animatix-text/src/lib.rs:997`) collapses every weight to one of nine CSS keywords
before Typst sees it, and the workspace pins typst **0.15.1**, which ships
`FontVariations::resolve` + `Font::instantiate` and calls it from five shaping sites.
The "identical ink at 400/600/800" measurement that justified the blocked verdict ran
the *plain-text fast path* (CJK shapes there, and that path uses `ttf_parser` without
setting axis coordinates) — it never asked the question of the typst path.

**The site pass this batch could actually verify.** `web/tour/scenes/glass.amx` joined §05 as
a second figure (the tour's own wiring is per-figure — `editor.js:157` skips a figure with
no `amx-player`, which is how §07's still already works), the recipes page gained its ninth
scene, and every vocabulary *number* on the pages was re-derived from the tables rather than
remembered: `ACTIONS` has 23 rows, 14 effect modules, the catalog has 32 primitives with
`Glass`, six built-in colorschemes, `find examples -name '*.amx'` is 71 files of which 12 are
import-only modules. That sweep found three stale claims the tests cannot see — the homepage's
`31 Primitives` and `62 runnable .amx files`, and the tour's `Ten scenes` header — plus the
recipes page's `Eight moves`, which went stale when `bar_race` joined it and nobody looked at
the heading.

**A gate earned its keep.** The workspace clippy run failed the batch on the last edit: the
change that made `Glass` paint nothing deleted the primitive's shape code, and an earlier
edit had left `wants_variable_faces` defined-but-unused when its call site went away. `cargo
build` did emit both as warnings, and the bench build printed them — but a warning is
survivable and `-D warnings` is not, which is the whole point of the CI rule. The lesson is
the one this round keeps re-learning: the checklist only counts when it runs *after* the
last change, and a tree that was green an hour ago is not evidence.

### Batch 7 (2026-10-05 into 10-06, the web delivery pass) — 7 commits, local only

Opened by the owner's report: `localhost:8124` showed an error on the homepage
animation. That turned out to be a stack of four separate faults, only the first
of which was visible, and finding the rest needed a browser — which is why this
batch is mostly about making the browser's state inspectable at all.

| Commit | What it fixed | Evidence |
|---|---|---|
| `afcc7eab` | the slim feature set had not compiled for days | `svg_import` is `#[cfg(feature = "svg")]`-gated, but it also held `parse_svg_path_data` — which the **bundled stroke icons** need, because their shapes are path-data strings. The slim profile is what CI's Pages job builds, so this was a deploy-level failure, not a theoretical one. The parser moved to `timeline/path_data.rs` (ungated), the asset stubs' `return Err(..)` became function-tail `Err(..)`, and two tests gained the feature gate they were supposed to have. `cargo clippy -p animatix --no-default-features --all-targets -D warnings` → 0 issues |
| `e8923887` | the web player never drained the pending layer queue | `Glass`'s frame split put backdrop/composite layers in a queue the wasm driver ignored, so the region it copied stayed stale. The drain became one scale-aware `drain_pending_layers` shared by offscreen, GUI and web (the web canvas renders at an adaptive `render_scale`, so regions, clips and corner radii all scale together). Guard: `scripts/perf-bench.sh compare demo_frame` → 43 benches, 0 regressions |
| `bf244125` | four stale bundle-size claims in `web/README.md` | re-measured against the rebuilt profiles: slim ~5.5 MB raw / ~1.7 MB brotli, full ~29.1 MB / ~8.5 MB |
| `d35ef16e` | **the gate the site content needed** | `crates/animatix-web/tests/site_scenes.rs` reads every `<amx-player>` under `web/`, takes the `profile` each element asks for, and builds that scene through `animatix_web::host` using the embed's own fetch protocol — `missing_imports` keys resolved against the scene, assets seeded relative to the declaring source, 24 retry rounds like `_loadScene`. Measured: 56 scenes referenced, 56 build with the full feature set, 54 build in slim and 2 skipped because every player referencing them says `profile="full"`. The slim run could not exist before this commit: `host.rs`'s two pre-seed tests call `AssetCache` methods that are feature-gated, so `cargo test -p animatix-web --no-default-features` failed to *compile* — the slim profile of the web crate had never been exercised natively |
| `d35ef16e` | the SVG probes probed nothing | `scene.amx` / `scene_rect.amx` are `Svg` actors and the primitive is registered only under the `svg` feature (`primitives/mod.rs:1642`), yet both pages played them through the default slim profile — so the probe pages showed a "Scene error" veil instead of the thing they exist to look at. They name `profile="full"` now, and `index.html`'s `?profile=slim` switch moves the element as well as the runtime directory |
| `f0757786` | three deploy defects | (1) the assemble step overwrote `web/demos/transformer/lib/components.amx` with `examples/lib/components.amx` — written for a symlink that no longer exists anywhere in the repo (`git ls-files -s web` finds none), and the two copies are **not** interchangeable: the site's opens with `import "../../lib/theme.amx"`, so the deploy shipped a library that cannot resolve its own colours to four transformer scenes and three gallery figures. (2) `on.push.paths` watched only `web/**` and `crates/animatix-web/**`, so an engine fix — the whole content of this batch — would not redeploy. (3) nothing checked content or the embed bundle before deploying; the job now regenerates `web/embed/amx-player.js` and fails on `git diff --exit-code`, then runs the scene gate in both profiles |
| `afaefc11` | **the engine fault behind the page errors** | five `ANIMATIX_FILTER_TIMING` probes gated their *printing* but not their *clock*: `let t = Instant::now()` ran unconditionally, and `std::time::Instant::now()` traps on wasm32-unknown-unknown (`unsupported/time.rs:13`, "time not implemented on this platform"). `GpuFilterBackend::render_and_filter_scene_to_view` is the path the web player takes for every `Filter`/`Mask`/`Glass` scope, so a browser frame with a backdrop scope panicked inside the engine and the page then reported `RefCell already borrowed` and `recursive use of an object detected which would lead to unsafe aliasing in rust` for as long as it lived. `web/recipes/` went from 4 scene errors + 464 console messages to 9/9 figures ready + 0 messages |
| `afaefc11` | the embed's own flood | `_renderScene` caught and `console.warn`ed every failed frame from the shared rAF loop — that is how one panic became 464 messages. One warning per failure run now |

**Gating the output is not gating the probe.** The rule "debug instrumentation
is additive or it does not land" was already in `AGENTS.md`, and every one of
these five sites obeyed it in the sense of *only printing when asked* — which
is why four code reviews' worth of gates missed it. What made this a browser-only
fault is that a probe also *acquires* something, and on this target the
acquisition itself traps. The probe clock is now `timing_probe() -> Option<Instant>`,
`None` on wasm and when the knob is off, with the decision in a `LazyLock` — so
the per-scope path does one static read where it used to take the environment
lock once per scope, i.e. the fix is cheaper *and* conditional. `AGENTS.md` states
the generalised rule.

**What the browser harness here can and cannot prove.** Every figure-level claim
in this batch rests on a headless Chromium driven over CDP, one fresh process per
page: a shared browser exhausts Dawn after a handful of navigations and every
later page reports `No suitable graphics adapter found`, which is a property of
the harness, not the site. With that fixed, the four content pages are clean —
`tour/` 11 figures, `recipes/` 9, `gallery/` 7, `demos/transformer/` 7, all
`ready`, zero messages. Two things remain unprovable on this box and are recorded
as such rather than claimed:

- **`Glass` frost pixels in the browser.** The engine's own readback cannot run
  headless: `Could not find SharedImageBackingFactory with params: … format:
  RGBA_8888 … WebgpuRead`, and the device is lost as soon as the readback texture
  is created. The compositor route is no better — two `--screenshot` captures of
  the looping homepage at different `--virtual-time-budget` came back
  **byte-identical** (`sha256 1143c7be…` for both), so canvas contents are not
  in the capture. The frost's measured behaviour therefore stays native-only (the
  4 px border reading `(93,27,32)` frosted vs `(255,0,0)` unfrosted); the browser
  side proves the scene builds, the engine initialises, and the figure renders
  through the same `drain_pending_layers` the native path uses.
- **A page mixing both profiles.** `demos/svg-probe/profiles.html` deliberately
  loads slim and full in one document; headless grants the second engine no
  adapter, so that probe reports one scene error here and needs a real browser to
  judge. Every delivered page uses one profile.

The remaining browser-console noise is by design, and worth naming so the next
pass does not chase it: `amx-player: <scene> built with N diagnostic(s)` is the
embed surfacing *warning*-severity diagnostics (the scene gate proves none of
them is an error), and the bare probe pages 404 on `/favicon.ico` because they
deliberately carry no site skin.

### Batch 8 (2026-10-06, the transformer review pass) — 3 commits, local only

The owner's ask was specific: re-review the transformer page, no typographic
errors, elegant and lively, **checked keyframe by keyframe**. That last clause
set the method — render all 80 keyframes of the seven scenes (plus t=0 and a
settled frame each, 94 frames), measure them, and look at the busiest.

| Commit | What it fixed | Evidence |
|---|---|---|
| `80f9da11` | one reading column for the article | `p` was 42em, `.lede` 44em, `ol.flow`/`p.outro`/`ul.reading` 46em, the hero caption 62em — em measures that resolve to a *different physical width per element* (714 / 924 / 782 / 868 px), so the column's right edge jumped block to block, and 42em at 17px runs 90+ characters. Replaced by `--col` 36rem and `--col-note` 30rem; `figcaption` and `.walk li` had no measure at all and ran to 133 characters. Browser audit (measure, size, leading, contrast, clipping, overflow, text overlap): transformer 32 findings → 0, tour 11 → 0, recipes 5 → 0, home 3 → 0 — with a negative control (62em columns make it report 6 blocks at 100–133 chars again), because a tool that says zero after you tune it has to be shown to still bite. `code.inline` also gets `white-space: nowrap`: a chip containing a space (`blend: "screen"`) wrapped between its words, drawing one bordered box split across two lines and colliding with the chip below — the three overlap findings on recipes disappeared with it |
| `89f080b8` | five figure defects, all invisible to `check` | see the narrative below |
| `test(web)` keyframe acceptance | 255 checks across seven `<name>.verify.txt` + `scripts/transformer-verify.sh` | `differs` per beat at a third of measured change, `visible`/`invisible` only where **both** halves hold, `ink` on the held frame at 85% of measured rest coverage |

**The five figure defects.** (1) Fifteen times across the seven scenes the
phase line printed **two labels on top of each other** for 60 ms: every swap
was `fade-out phaseN [260ms]` plus `fade-in phaseN+1 [280ms, delay: 200ms]`,
and the labels share the anchor (150, 96). Each delay now equals the outgoing
duration; verified by measuring the label region across the swap in tokens —
phase 1 present at 1.30/1.35/1.45, nothing at 1.55, phase 2 from 1.60.
(2) **Every outro erased the figure**, 22–45 staggered `fade-out`s, so the
frame the player holds for `hold="1.5"` was blank: tokens 0.00% ink, positional
0.00%, overview 0.11%, multihead 2.0%, feedforward 2.7%. The embed already
dissolves the finished frame into the next cycle, so the erase bought nothing
and cost the whole rest. Outros now clear only the narration line, and the held
frame keeps 97–99% of each scene's peak ink. (3) `\"it\"` in attention's phase
1 rendered as `\"it\"` — the lexer lets an escaped quote through without ending
the literal but never unescapes the value, so the backslash reaches the text;
replaced with typographic quotes, after rendering `“ ” ‘ ’ « »` to confirm the
fast-path face has them (the same check that caught the missing `⋮` in tokens).
(4) attention's `base` rule was authored `opacity: 0.35` with a `draw-in` at
1.40s, which is the exact mistake the scene family's own header comment warns
about — draw-in trims the stroke, it does not gate visibility — so a lone grey
rule sat on an empty plate for the first 1.4s. Seeded at 0.0 and lifted at its
beat: no ink at 0.15s or 1.20s, drawn at 1.60s. (5) Three alignment slips,
each measured rather than eyeballed: multihead's concat row (3×346 + gap 4)
spanned 117..1162 against its panels' 130..1150, so the band summarising three
heads overhung them and its rounded caps met in a pinch — now 3×320 at gap 30,
measured 130..1149 with each segment under its own panel; feedforward's two
sibling box titles sat 12px apart; overview's `residual stream` began 24px
left of the rail it names.

**feedforward's skip arcs ran through the boxes they were skipping.** `skip1`
lifted off the rail at x=150 and landed on the Attention box's *right edge*
(620, 390), which means it passed under the box's top border and travelled
inside it — the crossing is visible in the rendered frame at the shallow angle
where line meets rounded corner. Both arcs now run ring → add-point (210→648,
740→1168) with the control at y=20, which clears the box top by ~23px at the
corner and crosses the side border steeply where the skip rejoins the stream.

**A check that passed the bug it was written for.** The first version of the
rest-frame `ink` floor was half the measured coverage. Re-adding an outro that
erases the chips and first cells left the check **passing** — 13.2% against a
10.3% minimum — which means the file did not guard the very regression this
batch fixed. At 85% the same edit fails both ink lines and the restored scene
passes 23/23. Related: `differs` samples the frame sparsely, so a beat whose
whole change is a 3px rule (0.26% of the plate) reads 0.00%; those beats are
recorded in the checks file as comments naming what was measured rather than
asserted at a floor the sampler cannot honour. And `visible` was kept only
where its `invisible` partner also held, because a region that already carries
another actor's ink passes `visible` for the wrong reason — 16 of attention's
23 candidate actors are checkable that way, 6 of feedforward's 18.

### Batch 9 (2026-10-06, the demo and feature-page review) — local only

The same ask as Batch 8, aimed at everything that is not the transformer page:
the five demo pages, the eleven-figure tour, the nine recipes, the gallery and
the home page — no typographic errors, elegant and lively, **checked keyframe by
keyframe**. Method: sweep every scene the site plays with `animatix verify`
(ink at every `#Ns` stamp plus the held frame), then look at the fullest frame of
each figure, then fix what the numbers could not see.

| Commit | What it fixed | Evidence |
|---|---|---|
| `459a35c3` | seven figures that ended on an empty plate | held ink 0.26→34.0%, 0→46.6%, 0→39.3%, 0.36→28.9% (tour syntax/timing/actions/morph) and 0.46→2.1%, 0→2.8%, 0.38→6.8% (onecircle/losssurface/collision) |
| `04da60ac` | the remaining fourteen, and the rule | components 0.0→33.7%, reactive 0.0→15.8%, plots 9.4→37.4%, effects 3.7→20.4%, light_camera 0.6→54.1%, rotation 0.6→2.4%, sort 0.4→11.8%, plus the five `scene.amx` plates and the epicycles hero. **An outro may animate parameters, but it may not erase the cast** — the embed dissolves the finished frame into the next cycle (`_fade` in `web/embed/src/amx-player.js`), so the cascade only added a blank. effects and light_camera keep their closing gesture (the chain swallowing the plate; the camera pulling back) with the fades removed and effects' blur/vignette/grade stopped at 18/0.5/0.62 instead of running the frame to black |
| `2e8b7c27` | copy that contradicted its own figure | tour: "10 live scenes" (there are eleven players + one still), "a ten-section tour" (eleven), §01 promising `ease: bounce` that `syntax.amx` does not contain, §02 calling the five-ease race "the same recolor", §03 cross-referencing `always` to §08 (it is §09), §05's glass "ramps to 22 and back" (22 → 4 → 44), and five code sketches quoting values absent from the scenes they illustrate (140/400,440/210,90; `fade-in square[0]`; `ring.color`/`ring.size = (50 - p*10)`; `width: 180`; `brightness: 0.6`). recipes: "Eight small moves" over nine, and the glass limit stated as paint-order when it is geometric |
| `7c141efa` | three defects the sweep could not see | see the narrative below |

**The three the numbers missed.** (1) `glass.amx`'s close faded the card's own
surface, heading, sub-label and caption out under a `frost.radius = 44` — the
held frame was a frosted rectangle with no card in it, and frame ink stayed high
because the *plate behind* carries it. Fixed by the parameter-only rule; the
figure now rests on the shot it exists to demonstrate. (2) The same scene's
46px title sat at `at: (-150, -28)` and ran from x=325 to x=655 — 55px outside
the card's own left edge at 380 — so the label of a panel hung off the panel.
Centred, and its caption's "22" removed because the caption outlives that value.
(3) `hash/lookup.amx`'s second and third query chips were **never on screen at
all**: an authored `opacity: 0.0` on a component *instance* stops `fade-in`
lifting it, so the chips rendered zero pixels and their `shift` beats measured as
"dead beats". Two identical `LabeledBox` declarations, one with the authored 0 and
one without, isolate it: 11 168 px of travel versus 0.

**What the sweep's own limits turned out to be.** A `DEAD-BEAT` flag is only
evidence when the change is aperiodic: `marching_ants` and `light_camera`'s rail
both step the dash offset by exactly one period per beat — which is the recipe —
so frames one period apart are pixel-identical and a midpoint sample lands on the
pattern's own symmetry. Sampling at 0.1 s shows both marching (0.52% and 0.28% per
step). In a multi-scene plate the `#Ns` stamps are scene-local, so the plate
"dead beats" were measured at the wrong moments entirely — `hash/scene.amx`'s q3
shift moves 9 831 px at the time the sweep called dead. The committed
`scripts/site-rest-check.py` therefore asserts only the resting-composition rule,
states in its own docstring what it cannot see (a single actor vanishing over busy
pixels — `visible <t> <label>` measures the coverage of an actor's *bounds*, not
its own pixels), and was checked with a negative control: re-adding an erase
cascade to `syntax.amx` fails it at "0.1% against a 34.2% plateau", restoring the
scene passes.
