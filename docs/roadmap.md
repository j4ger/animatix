# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 0 — Multi-Scene GUI & Transitions

> **Active.** The CLI and core already support multi-scene composition, transition blending, and `play` edges. The GUI only exposes the active scene's timeline. This phase surfaces composition-level controls in the IDE.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Scene list sidebar tab** | New `SidebarTab::Scenes` that lists all scenes in a composition by declaration order. Clicking a scene switches `active_scene`. Shows scene name, duration hint, and transition icon on `play` edges. | `app/panels/sidebar.rs` | 2 days | — |
| 2 | **Composition timeline** | Mini timeline above the actor timeline showing scene blocks with start/end times, `play` edge arrows, and transition labels (e.g. "fade, 300ms"). Clicking a block seeks to that scene's start. | `app/panels/timeline_panel.rs` | 3 days | 1 |
| 3 | **Scene-level inspector** | When no actor is selected and the user clicks a scene block, the inspector shows scene properties: `background_color`, implicit duration (last keyframe time), and `play` target. Edits mutate the AST scene declaration. | `app/panels/inspector/` | 2 days | 1 |
| 4 | **Scene reordering** | Drag scenes in the scene list to reorder declaration order. Emits `SourceEdit` that reorders `# SceneName` blocks in the AST. | `app/panels/sidebar.rs`, `source_edit/` | 2 days | 1 |

**Status:** `PreviewSurface::render_composition()` already evaluates `Composition`, renders transition blends via `TransitionCompositor`, and composites dual scenes. The runtime already calls it when `document.composition` is present. This phase is purely GUI chrome.

---

## Phase 1 — PiP / Multi-Viewport

> **Deferred.** The current viewport system has been removed. PiP will be implemented as an actor-level `Scene` primitive, not statement-level declarations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Design `Scene` primitive** | Actor type whose content is another scene's timeline. Position, size, opacity are animatable properties (keyframes). `scene` property names the scene to render. | `primitives/`, `timeline/track.rs` | 3 days | Stable syntax |
| 2 | **Scene reference rendering** | Renderer evaluates referenced scene timeline at current time, clips to actor bounds, transforms to actor position, applies actor opacity. | `timeline/scene_eval.rs`, `renderer/` | 1 week | 1 |
| 3 | **Inspector + timeline support** | Scene actors show up in timeline tracks, inspector panel, and gizmo selection like any other actor. | `app/panels/` | 3 days | 2 |

---

## Phase 2 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 1 |
| 3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |
| 4 | **Snippet AST parsing** | Parse snippet text into `Vec<Stmt>` and insert via `SourceEdit` instead of raw text surgery. Requires lossless parsing (green tree) to preserve formatting. | `app/insertion.rs`, `animatix-green/` | 2 days | 2 |

---

## Order

1. **Phase 0** (Multi-Scene GUI — surfaces existing backend in the IDE)
2. **Phase 1** (PiP — after syntax and renderer are stable)
3. **Phase 2** (start after syntax stabilizes)

---

## Current Sprint — Phase 0.1

> Planned work for the next agent session.

| # | Task | Files | Est. |
|---|------|-------|------|
| 0.1.1 | Add `SidebarTab::Scenes` variant and tab bar entry | `app/panels/sidebar.rs`, `app/panels/mod.rs` | 2 h |
| 0.1.2 | Implement `scenes_content_ui` — list scenes from `Composition`, show active indicator, click to switch `active_scene` | `app/panels/sidebar.rs` | 4 h |
| 0.1.3 | Add scene context menu — "Set as active", "Duplicate scene", "Delete scene" | `app/panels/sidebar.rs`, `app/commands.rs` | 3 h |
| 0.1.4 | Wire `Scenes` tab into `SidebarContext` and app routing | `app/mod.rs`, `app/runtime.rs` | 2 h |
| 0.1.5 | Tests for scene list rendering and selection commands | `app/tests.rs` | 3 h |

**Total: ~14 hours (2 days)**

**Acceptance criteria:**
- Opening a multi-scene `.amx` file shows a "Scenes" tab in the sidebar
- Clicking a scene name switches the preview to that scene's timeline
- Active scene is visually highlighted
- Scene switch does not re-parse source (uses existing `Composition` in `Document`)

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 2 / green tree) | 2 |
| `let` variable animation | Superseded by easing functions in `always` blocks (6.8.3). Keyframed `let` tracks would need new timeline infrastructure; `always` lerp covers the same use cases statelessly. | Post-2 |
| **AI / NL Integration** | Requires external AI service (OpenAI, Claude, local LLM). No runtime dependency on AI should be mandatory. Includes: NL command bar, agent suggestion UI, agent_suggestions component. | Post-2 or separate product |
| **Row double-click / right-click** | No defined user story. Fields were wired to egui events but no caller consumed them. Re-add when a feature needs them. | When needed |
| **Badge button component** | Fully implemented but no caller. Re-add when the UI needs count badges (e.g. "Errors: 3"). | When needed |
| **Pre-compile plot closures** | Compile `func` AST bodies to closures/bytecode once per build instead of tree-walking thousands of times per curve. Would give 10–50× sampling speedup but requires a stable closure compilation API. | Post-2 or when plot count becomes a bottleneck again |
| **Amber flash on rewritten timestamps** | Visual polish: when `adjust_following_relative_keyframe` rewrites a relative offset, flash the timestamp label amber for ~300ms. Nice-to-have UX feedback. | When needed |
| **Unify duplicate PropertyValue types** | Two separate `PropertyValue` enums exist: `animatix::timeline::property_engine::PropertyValue` (engine-level) and `animatix_gui::app::commands::PropertyValue` (GUI-level). Different variant names (`F32` vs `Float`, `String` vs `Text`) force conversion logic in `apply_property_edit_to_track`. Unify into one canonical type. | When touching property dispatch again |
| **Replace `node_local_bounds` with trait-based bounds** | `node_local_bounds` takes `&[VelloPath]` forcing callers to materialize paths just for bounds computation. A `trait HasLocalBounds` on `VelloPath`/`TextPath`/`SceneImage` would be cleaner and allow lazy evaluation. Also simplifies the `command_local_bounds` helper (Phase 10b.1). | When touching scene_eval bounds logic |
| **Zero-readback filter compositing (end-to-end)** | Infrastructure is complete: `FullscreenBlitPipeline` supports alpha, `GpuFilterBackend` exposes `render_and_filter_scene_to_view()` and `take_last_filtered_view()`. Remaining work: modify `scene_eval.rs` to not draw filtered images into the Vello scene, and update `PreviewSurface`/`OffscreenRenderer` to blit the GPU texture after the base Vello render. `FilteredSource` tracking should be simplified to avoid fragile pointer comparison. | When filter performance matters |
| **Scene duration inference + editing** | Scene duration is currently implicit (last keyframe time). A dedicated `duration` property on scene declarations would enable trimming and looping. Requires syntax + AST + timeline changes. | Phase 0 or 1 |
| **Transition editor UI** | Visual editing of `play` edge transitions (type, duration, easing) in the composition timeline. Currently only editable in source. | Phase 0 |
