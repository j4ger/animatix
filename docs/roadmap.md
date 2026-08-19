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
| Data-dependent algorithm timelines | Closed by design. Runtime mutable state remains out of scope to preserve the random-access guarantee. Build-time algorithm precomputation via `let` shadowing + `list_swap`/`list_set` + `if`/`match` is now supported, tested, and documented. |

---

## Completed Backlog (2026-08-19)

All previously open extension/plugin backlog items are done. The remaining
source of truth for implementation details is
[Audit History](#audit-history).

- Plugin lifecycle and GUI runtime integration: `GL-01` through `GL-05`
- GUI plugin UX and discovery: `GUI-01` through `GUI-07`
- Native ABI and runtime polish: `EXT-01` through `EXT-07`

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
