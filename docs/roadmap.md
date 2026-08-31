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

## Known Issues (2026-08-26)

| Issue | Detail | Next Step |
|---|---|---|
| BarChart `gap` registry visibility (unverified) | An early-session note flagged the BarChart `gap` property's registry visibility as a known issue; status unknown after the subsequent build refactor | **Verified 2026-08-26**: `gap` is parsed by the shared plot props loop (handles `auto` or numeric), and BarChart delegates to `process_plot_actor_dispatch` — the only signal is an info-level `unknown-property` ("may still be valid"), same class as theme_studio. No fix needed. |
| Track `parent` back-reference is not always back-filled | First-declaration children inside containers can lack the parent pointer (noted during the component Group fix), so parent-chain queries cannot trust the stored field. The children lists ARE authoritative (regression-tested) | **Done 2026-08-26**: `Timeline::parent_of()` (derives child→parent from the children lists) added in `crates/animatix/src/timeline/mod.rs`; the never-revealed diagnostic routes its query through it. |
| ~~`descent_graph` cross-scene modifier warning~~ | ~~Symptom of the graph.map bug: 06_reactive/gradient_descent's `descent_graph.map` call couldn't resolve the receiver and the modifier IR logged `Undefined variable: descent_graph`.~~ | **Resolved by the dotted-NativeFn IR fix (env_keys module + lower.rs CallEnv join) — verified: 0 warnings at t=14 in gradient_descent.** |
| `hidden_by_default` flag goes STALE when reveals bypass lift_hidden_by_default | **Root cause found (probe, 2026-08-26)**: 06_reactive's title/ring carry staggered fade keyframes `[(0,0),(500,0),(1000,1)]` (authored fade-ins beyond the file's 40th line) yet the flag stays `true` — the fade keyframes were added without routing through `lift_hidden_by_default`, so the flag is a stale "never revealed" signal. The SOUND signal is keyframe-based: warn only when the opacity keyframes are all zero AND no ancestor's opacity lifts (parent chain derived from children lists) AND the actor is not a generated sub-actor. The earlier attempt's false positives were generated sub-actors (ticks/labels) whose visibility inherits from parents | **Resolved 2026-08-26**: the diagnostic was re-implemented on the keyframe + parent-chain + generated-sub-actor-exclusion model (`151c02f5`); `Timeline::parent_of()` supplies the parent chain and `FadeIn::execute` now routes its reveal through `lift_hidden_by_default`. Verified against the 42-example corpus. |
| GPU `Filter` `blur`/color effects are not visibly applied in `animatix image` export | A `Filter` with `blur: 10` over a high-contrast checkerboard stays sharp. Root-caused 2026-08-31: a backend test with a **color-matrix control** (`color_matrix_actually_desaturates`, passes) proves the scene render/compute/readback machinery works on the same device, while `gpu_filter_blur_softens_a_hard_boundary` (`#[ignore]`, known bug) shows the blur passes leave a perfectly hard edge — a **real code bug in the blur shader/ping-pong chain, not a software-Vulkan limitation**. | **Confirmed real bug** — see `dogfood/probes/009-filter-gpu-deferred` (updated). Fix `dispatch_blur` / the ping-pong in `filter_backend.rs` and enable the `#[ignore]` blur test as the regression guard. Also consider surfacing the Filter scene-eval silent fallback as a diagnostic. |
| Typst math with implicit multi-letter coefficients fails to compile | `Typst, content: "$mc^2$"` / `"$E = mc^2$"` error because Typst parses `mc` as a single multi-letter *variable*, not `m*c` (a Typst math gotcha). This is correct Typst semantics, not a bug. | **Resolved 2026-08-28**: the compile error now surfaces Typst's real message ("unknown variable: mc" + its hints) instead of an opaque "failed to compile Typst document". For a multi-letter product write `$m c^2$` or `$"mc"$`. |

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

The demo-gallery suite is complete and the gallery-era engine bugs are resolved.
The remaining open roadmap work is the **performance backlog** (PF-3 baseline
de-dup, PF-6 memory profile, PF-7 GPU throughput, PF-8 shared stage tracing,
PF-9 GUI perf sink) plus the deferred **Grid auto-fit**. Recommended next pass:
(PF-8 shared stage tracing behind a default-on feature, then PF-9 GUI perf sink)
so bench and GUI HUD measure the same stages before gating them in CI. Track
the rest as normal, commit-gated work per `AGENTS.md`.

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
