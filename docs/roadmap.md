# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md); for architecture, see [`architecture.md`](architecture.md); for GUI architecture, see [`contributing.md` §GUI Data Flow](contributing.md#gui-data-flow).

---

## P0 — GUI Correctness

*(All P0 tasks are complete.)*

## P1 — GUI Architecture Integration

*(All P1 tasks are complete.)*

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
