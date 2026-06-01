# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 7 — Audio

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 7.1 | **Audio multi-segment muxing** | Concatenate multiple audio files via ffmpeg into final output. Support per-scene audio tracks. | `export/ffmpeg.rs` | 3 days | — |

---

## Phase 8.5 — Unified Insertion Palette

> Self-contained GUI refactor: replace the fragmented action palette, completion snippets, and inspector actor creation with one unified, semantic, keyboard-first insertion system.
> Full design: [`design/insertion-mechanism.md`](design/insertion-mechanism.md)

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 8.5.1 | **Foundation: `InsertAction` + shared helpers** | Extract keyframe helpers from `keyframe_edits.rs` to `ast_utils.rs`. Add `InsertAction` variant to `SourceEdit`. Implement `insert_action` with exact-time semantics, style inheritance, and all six timeline rules. Unit tests for Examples A–D. | `source_edit/` | 1 day | — |
| 8.5.2 | **Bridge: `InsertionRequest` + `InsertionContext`** | Create `app/insertion.rs` with the bridge layer. Extend `insert_actor` to support keyframe-body insertion. Extract `unique_label` to `app/utils/labels.rs`. Wire `handle_insertion` in `DocumentController`. Add `all_snippets()` to analyzer. | `app/insertion.rs`, `app/utils/labels.rs`, `app/document_controller.rs`, `source_edit/actor_edits.rs` | 1 day | 8.5.1 |
| 8.5.3 | **UI: `InsertionPalette`** | Build fuzzy-search palette with 3 submodules (`mod.rs`, `items.rs`, `render.rs`). Auto-populate from `PRIMITIVES`, `get_action_signatures()`, and `all_snippets()`. Context-aware default mode. Bind `/` and `Ctrl+Shift+P`. | `app/shell/insertion_palette/`, `app/commands.rs`, `app/command_handlers.rs`, `editor.rs` | 1–2 days | 8.5.2 |
| 8.5.4 | **Polish: visual feedback + cleanup** | Amber flash on rewritten timestamp labels. Delete `action_palette.rs`. Update `ui_store`. Full test suite + clippy. | `cell_editor/render.rs`, `app/shell/action_palette.rs`, `app/stores/ui_store.rs` | 1 day | 8.5.3 |

---

## Phase 8 — Filter System

> Post-processing primitive for blur, color correction, and compositing effects.
> Full design: [`design/filter-system.md`](design/filter-system.md)

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 8.1 | **Language surface** ✅ | Add `Filter` to `ActorKindId`, `PROPERTY_REGISTRY`, primitive registry, and docs. Properties: `blur`, `brightness`, `contrast`, `saturate`, `hue_rotate`, `sepia`. | `timeline/`, `primitives/`, `docs/` | 1 day | — |
| 8.2 | **Shared GPU filter backend** ✅ | Extract `GpuFilterBackend` into `renderer/filter_backend.rs`. Used by both `PreviewSurface` and `OffscreenRenderer` with dedicated renderer core + temporary targets. | `renderer/filter_backend.rs`, `renderer/offscreen.rs`, `preview_surface.rs` | 1 day | — |
| 8.3 | **Scene evaluation integration** ✅ | In `scene_eval.rs`, detect `Filter` actors, evaluate children into sub-scene, render via `FilterBackend`, apply CPU filters, draw result as image. | `timeline/scene_eval.rs`, `timeline/filter.rs` | 2 days | — |
| 8.4 | **Unified preview + export rendering** ✅ | GUI `PreviewSurface` and CLI `OffscreenRenderer` both attach a `GpuFilterBackend` before evaluation so Filter output is identical in preview and export. | `preview_surface.rs`, `renderer/offscreen.rs` | 1 day | 8.2 |
| 8.5 | **Remove stale property-based effects** ✅ | Removed `shadow_blur`, `glow_radius`, `backdrop_blur`, `shadow_offset`, `shadow_color`, `glow_color` from registry, tracks, property engine, and scene eval. No deprecation shim (POC). | `timeline/property_registry.rs`, `timeline/track.rs`, `timeline/property_engine.rs`, `timeline/scene_eval.rs`, `docs/` | 1 day | — |
| 8.6 | **GPU shader filter pass** | Replace CPU blur + color matrix with WGSL compute shaders (blur H → blur V → color matrix) for 10–50× speedup on large scenes. | `renderer/shaders/`, `renderer/filter_backend.rs` | 1 week | When filter count becomes a bottleneck |
| 8.7 | **Documentation update** | Update `spec.md`, `properties.md`, `architecture.md` with Filter primitive, filter properties, and migration examples. | `docs/` | 1 day | 8.5 |

---

## Phase 9 — PiP / Multi-Viewport

> **Deferred.** The current viewport system has been removed. PiP will be implemented as an actor-level `Scene` primitive, not statement-level declarations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 8.1 | **Design `Scene` primitive** | Actor type whose content is another scene's timeline. Position, size, opacity are animatable properties (keyframes). `scene` property names the scene to render. | `primitives/`, `timeline/track.rs` | 3 days | Stable syntax |
| 8.2 | **Scene reference rendering** | Renderer evaluates referenced scene timeline at current time, clips to actor bounds, transforms to actor position, applies actor opacity. | `timeline/scene_eval.rs`, `renderer/` | 1 week | 8.1 |
| 8.3 | **Inspector + timeline support** | Scene actors show up in timeline tracks, inspector panel, and gizmo selection like any other actor. | `app/panels/` | 3 days | 8.2 |

---

## Phase 10b — Core Architecture Refactors (deferred from cleanup)

> Structural improvements discovered during Phase 10. These are large, risky changes
> that need dedicated testing time. Do not mix with feature work.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 10b.3 | **Atomic source-edit validation + drag batching** | All inspector edits (`handle_keyframe_edit`, `handle_property_edit`, `apply_child_order_edit`) must validate AST mutation and expression round-trip *before* touching the timeline. During drags, timeline mutates immediately but source is flushed once on drag end. Create `try_apply_source_edit` helper; move `PropertyValue → Expr` validation to `TryFrom`. | `actions/mod.rs`, `commands.rs`, `source_edit/` | 3 days | — |

---

## Phase 11 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 10.1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 10.2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 10.1 |
| 10.3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |

---

## Order

1. **Phase 7** (audio — no blockers)
2. **Phase 8** (filter system — no blockers, self-contained)
3. **Phase 8.5** (unified insertion palette — self-contained, can run in parallel with 7 or 8)
4. **Phase 9** (PiP — after syntax and renderer are stable)
5. **Phase 11** (start after syntax stabilizes)

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 10 / green tree) | 10 |
| `let` variable animation | Superseded by easing functions in `always` blocks (6.8.3). Keyframed `let` tracks would need new timeline infrastructure; `always` lerp covers the same use cases statelessly. | Post-10 |
| **AI / NL Integration** | Requires external AI service (OpenAI, Claude, local LLM). No runtime dependency on AI should be mandatory. Includes: NL command bar, agent suggestion UI, agent_suggestions component. | Post-10 or separate product |
| **Row double-click / right-click** | No defined user story. Fields were wired to egui events but no caller consumed them. Re-add when a feature needs them. | When needed |
| **Badge button component** | Fully implemented but no caller. Re-add when the UI needs count badges (e.g. "Errors: 3"). | When needed |
| **Pre-compile plot closures** | Compile `func` AST bodies to closures/bytecode once per build instead of tree-walking thousands of times per curve. Would give 10–50× sampling speedup but requires a stable closure compilation API. | Post-10 or when plot count becomes a bottleneck again |
