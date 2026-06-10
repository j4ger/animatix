# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md); for architecture, see [`architecture.md`](architecture.md); for GUI architecture, see [`contributing.md` §GUI Data Flow](contributing.md#gui-data-flow).

---

## P0 — GUI Correctness

| Task | Effort | Notes |
|------|--------|-------|
| **Fix whole-composition export** | 2-3d | `export_dialog.rs` routes `ExportTargetOwned::Composition` to `render_video_composition_with_progress` / `render_gif_composition_with_progress` / `render_image_composition` instead of silently falling back to active scene. Fix duration preview scope mismatch. Add scope picker UI (Whole composition / Active scene). |
| **Make undo/redo epoch-aware** | 1-2d | `handlers/ui.rs` — route text restoration through `SourceStore::replace_text()` (epoch + cache invalidation). Capture real `source_after` / `ui_before` / `ui_after` in `DocumentStore::snapshot()`. Call `UiStore::restore_snapshot()` in `handle_undo` / `handle_redo`. |
| **Unsaved-change prompts** | 1d | `handlers/file.rs`, `sidebar.rs` — dirty confirmation modal before open/reload/workspace-switch. |

## P1 — GUI Architecture Integration

| Task | Effort | Notes |
|------|--------|-------|
| **Wire snapshots into rebuild path** | 2-3d | `file.rs`, `source_store.rs`, `runtime.rs` — publish `DocumentSnapshot` on every rebuild; mark stale on every source change; render preview from `last_good_snapshot()` when current build failed. Delivers "last-good preview + stale badge" user-visible promise. |
| **Activate background rebuild worker** | 2-3d | `rebuild.rs`, `mod.rs` — fix `Drop` deadlock; move debounce-expiry path onto `RebuildWorker` with cancellation. Depends on undo/redo epoch fixes. |
| **Close remaining composition-blind paths** | 1d | `behavior.rs:46` — give sidebar resolved active timeline + asset cache for compositions. |
| **SnapSettings for keyframe snapping** | 0.5d | Replace hardcoded 60fps in `timeline_panel.rs` with project-configurable FPS from `UiStore`. |

## P2 — Animation Workflow

| Task | Effort | Notes |
|------|--------|-------|
| **Panel migration to view models** | 3-5d | Convert timeline panel (already mostly emits commands) → preview panel → inspector → sidebar, using `CommandBus` + view models from `app/panels/*_model.rs`. |
| **Dope sheet / per-property keyframe lanes** | 3-5d | Expand timeline panel with collapsible property lanes, box-select, copy/paste keyframes. |
| **Frame-accurate playback controls** | 2-3d | FPS setting, timecode display, frame-step, reverse/ping-pong, loop in/out markers. |
| **Inline editor diagnostics (squiggles)** | 2d | Highlight errors inline in the cell editor instead of only in the diagnostics panel. |
| **Property spreadsheet view** | 3-5d | Rows = actors, columns = properties for bulk layout tuning. |

## P3 — Polish & Performance

| Task | Effort | Notes |
|------|--------|-------|
| **Performance HUD** | 2d | Show rebuild time, render time, GPU texture memory estimate, stale-preview badge. |
| **Window size persistence** | 1d | Save/restore native window size and maximized state alongside tile layout. |
| **Storyboard / scene thumbnails** | 3-4d | For multi-scene compositions: thumbnail strip, transition badges, drag-to-reorder. |
| **Layout debugger overlay** | 2-3d | Show container bounds, layout slots, intrinsic sizes, padding/gaps on canvas. |
| **Component authoring UI** | 4-6d | Component instances, params, slots, jump-to-definition in the GUI. |

## Icebox

| Task | Reason |
|------|--------|
| **Scene primitive / picture-in-picture** | Transition blending shipped; existing components and `Stack` cover most reuse cases. |
| **Export performance: pre-compiled plot closures** | Only matters for many plot actors or heavy sampled fields. |
| **Asset usage tracking** | Show which actors reference an asset; no strong user story yet. |
| **Variable track UI** | GUI for `let` variable tracks; `always` blocks cover most interactive cases. |
| **Module dependency graph** | Visual graph of `.amx` imports; internal tooling value only so far. |
| **Lossless whitespace/trivia preservation** | Current write-back pipeline correct for all normal use cases; comments roundtrip, formatting idempotent. |
| **APNG export** | Request-driven only; GIF covers lightweight previews, video/WebM covers higher-quality sharing. |
| **Source-diff preview sidecar** | Show the `.amx` diff when dragging actors or editing properties in the inspector. |
| **Animation heatmap view** | Heatmap of animated property density across time, actors, categories. Useful for large generated `.amx` files. |
