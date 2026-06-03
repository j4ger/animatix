# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 0 — Multi-Scene GUI & Transitions

> **✅ Complete.** The CLI and core already supported multi-scene composition, transition blending, and `play` edges. This phase surfaced composition-level controls in the IDE.

| # | Item | What | Files | Effort | Status |
|---|------|------|-------|--------|--------|
| 1 | **Scene list sidebar tab** | `SidebarTab::Scenes` lists scenes by declaration order. Click to switch `active_scene`. Duration + transition hints. | `app/panels/sidebar.rs` | 2 days | ✅ |
| 2 | **Composition timeline** | Mini timeline with scene blocks, `play` edge arrows, transition labels. Click to seek. | `app/panels/timeline_panel.rs` | 3 days | ✅ (already existed) |
| 3 | **Scene-level inspector** | When no actor selected, inspector shows scene header, properties (duration, start, background), transition card with "Go to" button, and scene list. | `app/panels/inspector/mod.rs` | 2 days | ✅ |
| 4 | **Scene reordering** | Drag-and-drop in scene list reorders declaration order. Emits `Command::ReorderScenes` which reorders `Stmt::Scene` blocks in AST and re-serializes. | `app/panels/sidebar.rs`, `app/handlers/scene.rs` | 2 days | ✅ |

**Commits:** `7582b0b` (scene list), `66cf7db` (scene inspector), `6860744` (scene reordering).

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

1. **Phase 0** ✅ (Multi-Scene GUI — surfaces existing backend in the IDE)
2. **Phase 1** (PiP — after syntax and renderer are stable)
3. **Phase 2** (start after syntax stabilizes)

---

## Current Sprint — Phase 0.5 (Polish)

> Remaining Phase 0 work before moving to Phase 1.

| # | Task | Files | Est. |
|---|------|-------|------|
| 0.5.1 | **Transition editor UI** — visual editing of `play` edge transitions (type, duration, easing) in the composition timeline or inspector. Currently only editable in source. | `app/panels/inspector/`, `app/panels/timeline_panel.rs` | 1 day |
| 0.5.2 | **Scene duration editing** — add `duration` property to scene declarations (currently implicit). Inspector shows editable duration field. | `app/panels/inspector/`, `timeline/track.rs` | 1 day |
| 0.5.3 | **Scene duplicate / delete** — context menu items in scene list for duplicating (copy AST + rename) and deleting scenes. | `app/panels/sidebar.rs`, `app/commands.rs` | 1 day |

**Total: ~3 days**

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
