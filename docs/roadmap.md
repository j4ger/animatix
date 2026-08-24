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
| PF-3 | Persist/de-dup benchmark baselines across CI runs | Open | Compare PR runs against last `main` baseline via artifacts; promote `perf-bench compare` to a gate |
| PF-4 | P1 frame-evaluation hot path (cache-hit restore clone, per-frame `Vec`/`SceneItem` churn, allocation in `encode_scene`) | Partly done | Frame-cache-hit no longer clones items/bounds/diagnostics — only the scene (commit `94873806`; `many_actors_cache_hit` 2408→1201ns). Remaining: no-cache miss path (~33µs) and `encode_scene` allocation; profile and gate `frame.*`/`scrub.*` |
| PF-5 | P2 rebuild latency (font load, expand/typecheck, planner) | Partly done | System font DB shared process-wide (commit `5b12b015`); Text/Code/Typst compilations memoized process-wide keyed on all inputs (font-environment epoch guards staleness); build-time expression cache keys on an O(1) environment stamp; and `build_eval_env` now injects only actor labels referenced by the program (`build::referenced_roots` AST pre-scan), turning environment construction from O(declarations²) into O(declarations × referenced). `text_rebuild/mixed_48_warm`: 49.6ms→0.41ms (~120×); `components_full` −58%, `modules_full` −15%; lib test suite 58s→9s. Remaining: `expand_components` recursion on generated scenes; gate `rebuild.*` |
| PF-6 | P3 allocation / memory profile (peak RSS, per-frame clones) | Open | Add mem capture; DHAT/tracy on scenarios |
| PF-7 | P4 GPU / export throughput (raster ms, video/GIF encode FPS) | Open | Layer-3 perf binary under `nix develop`; wire `PerformanceMetrics::set_gpu_memory` |
| PF-8 | Shared stage tracing (`crates/animatix/src/perf.rs`, `ScopedStage`) so benches + GUI HUD measure the same stages | Open | Implement behind a default-on feature; verify it doesn't perturb bench numbers |
| PF-9 | GUI JSONL perf sink (`--perf-log`) from `PerformanceMetrics` | Open | Add sink; collect real-authoring data |

---

## Open Questions

### Comment Directives

Presenterm uses HTML-comment directives because it must extend markdown without
creating a second DSL. Animatix already owns a semantic DSL, so comment
directives are likely the wrong mechanism. The recommendation is to map valuable
commands to native `.amx` features and add first-class metadata (for example
speaker notes or export presets) only when a concrete user story appears.

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
| 2 | `motion_poster.amx` + `dashboard_story.amx` | **Done** (on `feat/demo-gallery-p2`, pending merge) | clean `check`; PNG smoke of every scene | Engine fixes landed with it — see `docs/handoff_phase2.md` |
| 3 | `epicycles.amx` + `sorting_theatre.amx` | Open | clean `check`; 3-frame PNG smoke | `sorting_theatre` needs `dynamic_layout` + build-time sort precomputation + `swap` actions |
| 4 | `brand_reel/` capstone | Open | all six `play` transitions ≥1×; `persist`; Audio; cross-file scenes | Multi-scene zero-duration bug fixed; still need cross-file slot fills / component-instance positioning workarounds |
| 5 | Tutorial refurbishment + README matrix + `scripts/check_examples.sh` smoke | Open | script green; render smoke covers all examples | Reuses new `lib/`; delete `animation/16_showcase.amx` and `composition/20_feature_reel.amx` once `brand_reel` lands |

### Engine Bugs to Fix Before Phase 4

These were discovered during Phase 1 and are currently worked around. Fixing
them unlocks multi-scene gallery demos and cleaner component authoring.

| Bug | Impact | Where to Reproduce | Suggested Fix Area |
|---|---|---|---|
| Multi-scene files clamp playback when a scene has zero inferred duration | A scene with only actor declarations got duration 0; when it was the target of a `play` transition the composition global duration collapsed to the outgoing play time, cutting off prior-scene actions | Any multi-scene file whose last/target scene has no keyframes | **Fixed 2026-08-22**: floor inferred scene durations to `max(incoming transition duration, 1/60s)` |
| Cross-file `@slot` fills are ignored | Imported `Card` always shows fallback children | `examples/lib/ui.amx::Card` used from another file | Module import/expand path for slot overrides |
| Cross-file custom component `fn` actions are not resolved | `error[build:unknown-action]` for actions defined in imported component | `lib/ui.amx::MetricCard.pop_in` invoked from a scene file | Action name resolution across module boundaries |
| Component instances ignore `anchor`/`offset`/`at` | Cannot position a component instance directly | Any component instance with `anchor:`/`offset:`/`at:` | **Fixed 2026-08-24**: expansion forwards `opacity`/`at`/`anchor`/`offset` to the expanded root actor |
| Col/Grid auto `text_max_width` overrides explicit value for CJK | Chinese labels wrap after 2–3 chars even with explicit `text_max_width` | Text directly inside a Col inside a component | Layout width propagation should not override an explicitly set `text_max_width` |
| `Mask` children clipped at the scene origin | Every child of a Mask not positioned at the top-left corner was clipped away entirely (Mask + Image rendered nothing) | `Mask { .. }` with `at:` away from origin | **Fixed 2026-08-24**: clip layer now transforms with the Mask; `clip_shape` child is still decorative (clip = Mask `size` rect) — making it define the clip geometry is the follow-up |
| Hosted plots occupy only the central half of their Graph | `{graph}_size` stored as half-size but consumed as full-size by bars/curves/`.map()` | `Graph { chart: BarChart, .. }` vs the axis extent | Pick one size convention; touches plot builders + GUI inspector |
| Failed property expressions fall back silently | `theme.text_md` with a non-aliased import renders at defaults (font size 0) with no diagnostic | `font_size: theme.text_md` after plain `import "lib/theme.amx"` | Warn in `evaluate_expr_with_lookup_diagnostic` when a path lookup fails |
| Invalid easing names fall back silently | `ease: bounce-out` (canonical: `bounce`) animates with the default easing, no warning | `[1s, ease: bounce-out]` | Warn in `parse_timing_modifiers` on unknown easing names |

### Next Immediate Session Recommendation

1. Pick **Phase 2**: start with `dashboard_story.amx` (uses existing `lib/`
   components heavily) or `motion_poster.amx` (typography/morph/Filter).
2. Keep each phase in its own worktree, run the pre-commit gates from
   `AGENTS.md`, and merge back before starting the next phase.
3. For Phase 4, remember the remaining component workarounds: wrap imported
   component instances in `Group` for positioning, avoid cross-file `@slot`
   fills, and avoid custom `fn` actions on imported components.

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
