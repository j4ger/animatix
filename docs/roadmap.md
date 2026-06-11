# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md); for architecture, see [`architecture.md`](architecture.md); for GUI architecture, see [`contributing.md` §GUI Data Flow](contributing.md#gui-data-flow).

---

## P0 — GUI Correctness

*(All P0 tasks are complete.)*

## P1 — GUI Architecture Integration

*(All P1 tasks are complete.)*

## P2 — Animation Workflow

*(All P2 tasks are complete.)*

| Task | Effort | What was built |
|------|--------|----------------|
| **Frame-accurate playback controls** | 2-3d | `PlaybackController`: `ping_pong` mode + direction reversal, `timecode_string()` (HH:MM:SS:FF), `fps` field. Timeline toolbar: frame-step forward/backward buttons (⏪/⏩), ping-pong toggle, timecode + FPS display. `Command::FrameStepForward`/`FrameStepBackward` + handlers. |
| **Inline editor diagnostics (squiggles)** | 2d | `draw_wavy_underlines()` in `cell_editor/render.rs` — zigzag wavy lines for errors (red) and warnings (amber) using exact font metrics. Integrated into both code cell and keyframe cell body rendering. |
| **Property spreadsheet view** | 3-5d | `inspector/spreadsheet.rs` — `egui::Grid` with rows=all actors (sorted), columns=key properties. Value cells show current playhead values; right-click → Add keyframe / Open in Inspector. `PropertyViewMode::Spreadsheet` variant, cycles Semantic → Spreadsheet → Intensity. |
| **Dope sheet / per-property keyframe lanes** | 3-5d | Timeline panel: collapsible per-property sub-tracks under each actor. `PropertyGroup` enum (Transform/Style/Filter/Shape/Text/Layout) with color-coded diamond lanes. `expanded_properties` tracking + LIST toggle button per actor. |
| **Panel migration to view models** | 3-5d | Missing view models created: `SidebarModel`, `EditorModel`. 13 new `Command` variants for direct state mutations (zoom, scroll, loop region, collapse, tool mode, sidebar tab, view modes, pivot offsets). Handler functions added in `handlers/ui.rs`. `CommandBus` wired alongside `ActionQueue` in the frame pipeline. Full mutable-context cleanup deferred — incremental migration path established. |

## P3 — Polish & Performance

*(All P3 tasks are complete.)*

| Task | Effort | What was built |
|------|--------|----------------|
| **Performance HUD** | 2d | `PerformanceMetrics` (rolling FPS EMA, rebuild/render timing, GPU mem, stale flag, sparkline). `PreviewOverlay::show_performance_hud` toggle. HUD renders in top-right corner with FPS, timings, memory, stale badge + mini sparkline. Toggle via toolbar "Performance" button. `record_tick()`/`record_render()`/`set_stale()` wired in runtime. |
| **Window size persistence** | 1d | `WorkspacePersistence.window_size` + `window_maximized` with `#[serde(default)]`. Loaded in `run_gui()` to set `ViewportBuilder`. Captured each frame from `frame.info()`. Saved via `save_persistence()` on exit. Backward-compatible with old persistence files. |
| **Storyboard / scene thumbnails** | 3-4d | Scene blocks in timeline composition track enhanced with keyframe density strips (tiny vertical blue marks at keyframe positions) + duration labels. Edge arrows augmented with transition badges (F/W/C icons) and hover tooltips showing transition type, target, duration. `scene_keyframe_times` cache built in `behavior.rs`. |
| **Layout debugger overlay** | 2-3d | `DebugRenderOptions.draw_layout_debug` + `draw_spacing` fields. `render_layout_debug()` in overlay.rs draws container outlines with type labels (blue), child slot outlines (amber), intrinsic size labels, and gap/padding bands. Toggled via "Layout" / "Spacing" toolbar buttons. `PreviewContext.debug_layout/debug_spacing` wired through behavior. |
| **Component authoring UI** | 4-6d | Slots display in Components tab (`@slots: slotname` in cyan). Jump-to-definition button (arrow icon) searches source text for `component Name` and scrolls editor. Richer param form in insertion palette: `DragValue` for Num, checkbox for Bool, dual `DragValue` fields for Vec2, default text for others. `ParamInfo` struct stores type annotations. |

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
