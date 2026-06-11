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

## P4 — UI Audit & Hardening

| Priority | Task | Effort | Notes |
|----------|------|--------|-------|
| **P0** | **Fix dead feedback channel**: render `preview.status` in a visible status bar, or convert all 135 write sites to toasts | 1d | Every error/confirmation routed through `.status` is silently overwritten each frame by `live_preview_status()`. |
| **P0** | **Guard `T`-key time lens behind `egui_wants_keyboard_input()`** | 0.5d | Fires during text input in cell editor, rename fields, explorer filter. All other global shortcuts have this guard. |
| **P0** | **Fix spreadsheet "Add" actor type**: use `default_actor_type()` + `unique_label()` instead of `"rect"` + `"actor_N"` | 0.5d | `"rect"` (lowercase) doesn't match primitive registry "Rect"; hardcoded label may collide. |
| **P1** | **Deduplicate zoom/pan state**: remove `timeline_zoom`/`preview_zoom`/`preview_pan` from `UiStore::snapshot()`; read from `PreviewStore` directly | 1d | Undo snapshots record zoom=1.0/pan=0.0 because `UiStore` copies are never written after init. |
| **P1** | **Repopulate insertion palette on open**: don't gate on `items.is_empty()` | 1d | Components/actions added after first open are invisible; deleted ones remain insertable. |
| **P1** | **Fix inspector view-mode button positions**: use card-relative Y instead of scroll-viewport Y | 1d | View-mode and keyframe-toggle buttons overlap (both right-aligned at same absolute Y). |
| **P1** | **Fix or remove misleading shortcut tooltips**: G (grid toggle), Home/End (go to start/end), ⌘K/? (command palette) | 1d | Users try these keys and conclude the app is broken. Either wire handlers or correct tooltips. |
| **P1** | **Gate timeline zoom behind Ctrl/Cmd+wheel** | 0.5d | Plain wheel zooms AND scrolls the `ScrollArea` simultaneously — timeline with many tracks is nearly unscrollable. |
| **P1** | **Fix bulk keyframe delete**: iterate property tracks directly instead of `collect_actor_keyframes()` which dedups by time | 1d | Two properties keyframed at the same time: only one property's keyframe is deleted. |
| **P1** | **Include `track_idx` in action drag matching** | 0.5d | `is_action_drag` matches only `start_time_ms` — dragging one block highlights same-time blocks on all tracks. |
| **P1** | **Fix layer tree drop-to-root 100px halo**: show drop indicator and shrink expand region | 1d | Dropping near (but not on) the tree silently reparents. No feedback shown. |
| **P1** | **Multi-selection inspector: show all selected actors' common properties** instead of iterating `HashSet` | 1-2d | Currently shows a non-deterministic single actor's properties; edits target that arbitrary actor only. |
| **P2** | **Replace view-mode cycle button with segmented control** (Semantic / Spreadsheet / Intensity) | 1d | Cycle button shows current mode but advances to next — classic ambiguity. Same for toolbar zoom cycle. |
| **P2** | **Components/Assets tab: change single-click to double-click for instantiation** | 0.5d | Misclick while browsing mutates the document. Context menu "Instantiate" already exists. |
| **P2** | **Inspector: change single-click rename to double-click** | 0.5d | Easy to trigger when just trying to focus the inspector. |
| **P2** | **Diagnostics panel: show empty state when toggled with zero diagnostics** | 0.5d | Current: toggling on with no diagnostics highlights the button but shows nothing. |
| **P2** | **Update keyboard shortcut cheat sheet** to reflect actual bindings | 1d | Omits F/Shift+F zoom, 1/2/3 scene jump, Ctrl+G group, A/`/` palette, arrow nudge, Ctrl+D. |
| **P2** | **Replace hardcoded overlay colors with design tokens** | 1d | `overlay.rs` (perf HUD grays, layout debug colors), `insertion_palette.rs` (category colors) bypass `design_tokens.rs`. |
| **P2** | **Fix timeline click-to-scroll on already-visible row**: only scroll when row is outside viewport | 0.5d | Produces jarring scroll jump on every selection click. |
| **P2** | **Increase keyframe diamond hit area separation** for dense keyframe runs | 1d | Adjacent diamonds < 16px apart have overlapping interact regions; later-registered one wins. |

## Icebox

| Task | Reason |
|------|--------|
| **Audit finding: design tokens doc drift** | `design_tokens.rs` module doc advertises token groups (`color::SURFACE_BASE`) that don't exist. Actual names are `BG_BASE`, `FONT_SIZE_*`, `RADIUS_*`. |
| **Audit finding: stale time_lens doc comment** | `time_lens.rs:18` still says "Is Space currently held?" for the `T` key. |
| **Scene primitive / picture-in-picture** | Transition blending shipped; existing components and `Stack` cover most reuse cases. |
| **Export performance: pre-compiled plot closures** | Only matters for many plot actors or heavy sampled fields. |
| **Asset usage tracking** | Show which actors reference an asset; no strong user story yet. |
| **Variable track UI** | GUI for `let` variable tracks; `always` blocks cover most interactive cases. |
| **Module dependency graph** | Visual graph of `.amx` imports; internal tooling value only so far. |
| **Lossless whitespace/trivia preservation** | Current write-back pipeline correct for all normal use cases; comments roundtrip, formatting idempotent. |
| **APNG export** | Request-driven only; GIF covers lightweight previews, video/WebM covers higher-quality sharing. |
| **Source-diff preview sidecar** | Show the `.amx` diff when dragging actors or editing properties in the inspector. |
| **Animation heatmap view** | Heatmap of animated property density across time, actors, categories. Useful for large generated `.amx` files. |
