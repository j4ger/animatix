# GUI UI/UX Audit

## Architecture Overview

`crates/animatix-gui` is no longer a simple three-file GUI. It is a substantial egui/eframe app with a tiled workspace, document/session model, source editor, preview surface, sidebar tabs, inspector, timeline panel, export modal, command palette, and source-edit pipeline.

The runtime entry point is thin: `crates/animatix-gui/src/main.rs:1` initializes tracing and calls `animatix_gui::run_gui`. `crates/animatix-gui/src/app/runtime.rs:14` creates the native eframe window, and `crates/animatix-gui/src/app/runtime.rs:52` requires `wgpu_render_state`, meaning the GUI hard-depends on eframe's WGPU backend. `AnimatixApp` owns the `GuiShell`, `PreviewSurface`, egui texture id, screenshot state, and audio engine in `crates/animatix-gui/src/app/runtime.rs:38`.

The shell is the central coordinator. `GuiShell` owns `DocumentStore`, `WorkspaceStore`, `PreviewStore`, `UiStore`, `ExportStore`, and `InsertionPalette` in `crates/animatix-gui/src/app/mod.rs:269`. This is a useful split, but the app still passes large mutable context structs through panels, especially `WorkspaceBehavior` in `crates/animatix-gui/src/app/panels/behavior.rs:14`.

The widget tree is:
- Top toolbar in `crates/animatix-gui/src/app/mod.rs:445`.
- Optional bottom diagnostics panel in `crates/animatix-gui/src/app/mod.rs:455`.
- Central tiled workspace in `crates/animatix-gui/src/app/mod.rs:472`.
- Tile panes routed by `WorkspaceBehavior::pane_ui` in `crates/animatix-gui/src/app/panels/behavior.rs:38`.
- Sidebar tabs, including Explorer/Layers/Scenes/Components/Assets/Editor, in `crates/animatix-gui/src/app/panels/sidebar.rs:98`.
- Preview canvas in `crates/animatix-gui/src/app/panels/preview_panel.rs:27`.
- Inspector in `crates/animatix-gui/src/app/panels/inspector/mod.rs:41`.
- Timeline in `crates/animatix-gui/src/app/panels/timeline_panel.rs:47`.
- Modals/overlays for settings/export/command palette/find-replace/insertion palette in `crates/animatix-gui/src/app/mod.rs:512`.

The data pipeline is:
- Editor edits update `EditorBuffer`, then `Command::EditorChanged` is queued from `crates/animatix-gui/src/app/panels/sidebar.rs:209`.
- `handle_editor_changed` copies editor text into `DocumentSession.source_text`, clears diagnostics, and schedules a debounced rebuild in `crates/animatix-gui/src/app/handlers/playback.rs:49`.
- `DocumentSession::rebuild` parses/modules/typechecks/builds timeline or composition in `crates/animatix-gui/src/document.rs:171`.
- `sync_preview_surface` renders either a composition or a single timeline in `crates/animatix-gui/src/app/runtime.rs:299` and `crates/animatix-gui/src/app/runtime.rs:307`.
- Hit regions are pulled from the render surface and copied into source/UI caches in `crates/animatix-gui/src/app/runtime.rs:364`.
- Inspector/preview/timeline commands mutate source through `SourceEdit` or direct AST operations, then schedule a rebuild.

The app has real animation-workflow ambition: timeline scrubbing, keyframe diamonds, action blocks, scene tracks, loop ranges, onion-skin-like ghost overlays, snapping, rulers/guides, inline text editing, export progress, audio sync, and scene management all exist. The weak point is consistency: many features work for single-scene `Timeline`, but degrade or fail for `Composition`.

## State Management Findings

The main state split is sensible but overloaded:
- `DocumentSession` holds canonical source text, raw AST, expanded AST, source index, timeline, composition, active scene, diagnostics, duration, scene dimensions, timeline index, caches, and module graph in `crates/animatix-gui/src/document.rs:18`.
- `SourceStore` holds the document, editor, and hot-path caches in `crates/animatix-gui/src/app/stores/source_store.rs:8`.
- `HistoryStore` holds undo/redo and render/runtime diagnostics in `crates/animatix-gui/src/app/stores/history_store.rs:7`.
- `PreviewStore` owns playback, pending rebuild, dirty flags, and rebuild status in `crates/animatix-gui/src/app/stores/preview_store.rs:5`.
- `UiStore` owns selection, interaction, clipboard, tiled view state, editor sync, keyframe mode, UI defaults, pending actions, and modal query strings in `crates/animatix-gui/src/app/stores/ui_store.rs:96`.

Undo/redo is source-snapshot based. `HistoryStore::snapshot` stores a `Command` plus `source_before` and clears redo in `crates/animatix-gui/src/app/stores/history_store.rs:31`. Undo/redo replace `document.source_text` and `editor` text, mark dirty, and schedule rebuild in `crates/animatix-gui/src/app/handlers/ui.rs:41` and `crates/animatix-gui/src/app/handlers/ui.rs:66`. This is robust for text-backed edits, but it does not restore UI state such as selected actors, active scene, pivot offsets, tool mode, loop region, or timeline zoom.

There is too much duplicated state:
- `DocumentSession.source_text` and `EditorBuffer.text` both represent the editable document.
- `raw_statements`, `source_index`, `timeline_index`, `keyframe_lines`, `timeline`, and `composition` are all derived from source.
- `PreviewStore.preview.playback.duration_s` duplicates `DocumentSession.duration_s`.
- `UiStore.selection.hit_regions`, `SourceStore.cached_hit_regions`, and `SourceStore.cached_actor_bounds` duplicate renderer output.
- `active_scene` is mutated both by time playback and scene selection.

The app does try to centralize invalidation with `SourceStore::invalidate_cache` in `crates/animatix-gui/src/app/stores/source_store.rs:31`, but cache validity is still fragile because direct source commits are scattered across `actions`, `document_controller`, insertion palette, settings, find/replace, and handlers.

`DocumentSession::rebuild` has useful optimizations: source hash short-circuit in `crates/animatix-gui/src/document.rs:181`, module graph cache in `crates/animatix-gui/src/document.rs:315`, component expansion cache in `crates/animatix-gui/src/document.rs:338`, and plot/modifier cache reuse in `crates/animatix-gui/src/document.rs:231`. But the component cache key only hashes component definitions, not arbitrary import/module value changes, so expansion reuse deserves scrutiny for imported constants/actions that affect expanded output.

## Real-World Workflow Gaps

Scrubbing exists but is not production-grade yet. Space toggles playback in `crates/animatix-gui/src/app/runtime.rs:139`, arrow keys scrub in `crates/animatix-gui/src/app/runtime.rs:202`, the ruler scrubs in `crates/animatix-gui/src/app/panels/timeline_panel.rs:491`, and a time lens exists in `crates/animatix-gui/src/app/panels/preview_panel.rs:293`. Missing: explicit frame stepping controls, timecode/frame-number display, editable FPS grid, snapping modes beyond hardcoded 60fps, and a playhead that can be dragged from every timeline row.

Keyframe visualization exists but is shallow. Keyframe diamonds are drawn in `crates/animatix-gui/src/app/panels/timeline_panel.rs:849`, moved via `Command::MoveKeyframe` in `crates/animatix-gui/src/app/panels/timeline_panel.rs:938`, and easing can be changed by context menu in `crates/animatix-gui/src/app/panels/timeline_panel.rs:873`. Missing: visible per-property lanes, easing curves directly on the global timeline, keyframe copy/paste, box select across tracks, ripple/scale timing edits, hold/step keyframes, and grouped keyframe handles.

Timeline controls exist but are minimal. Speed cycles only through `0.5x/1x/2x/4x` in `crates/animatix-gui/src/app/panels/timeline_panel.rs:406`. Loop toggles only whole-duration defaults in `crates/animatix-gui/src/app/panels/timeline_panel.rs:413`, with draggable range handles in `crates/animatix-gui/src/app/panels/timeline_panel.rs:1014`. Missing: custom speed input, reverse playback, ping-pong, frame stepping, "play selection," in/out markers independent of loop, and persistent work/export ranges.

Property editing exists in the inspector with semantic groups and a stream mode. Numeric properties use `DragValue` in `crates/animatix-gui/src/app/panels/inspector/property_groups.rs:483`, color editing uses `color_edit_button_srgba` in `crates/animatix-gui/src/app/panels/inspector/property_groups.rs:657`, and text/source uses single-line text edits in `crates/animatix-gui/src/app/panels/inspector/property_groups.rs:740`. Missing: a spreadsheet/property table for many actors, batch editing with mixed values, expression-aware fields, percent/anchor-aware position editors, and reliable multi-selection property editing.

Diagnostics exist as a bottom panel and editor focus. The list is shown in `crates/animatix-gui/src/app/mod.rs:455`, and clicking diagnostics triggers `Command::ScrollToLine` in `crates/animatix-gui/src/app/mod.rs:464`. Missing: inline squiggles in the cell editor, preview annotations for actor-specific runtime errors, phase filtering, grouped imported-file diagnostics, and non-disruptive stale-preview indicators while editing invalid source.

File workflow is basic. Welcome can create `untitled.amx` with `#0s` in `crates/animatix-gui/src/app/mod.rs:617`, open existing files in `crates/animatix-gui/src/app/mod.rs:643`, Explorer opens files in `crates/animatix-gui/src/app/panels/sidebar.rs:362`, and save writes the editor text in `crates/animatix-gui/src/app/handlers/file.rs:145`. Missing: Save As, New from template, unsaved-change prompt before opening another file, recent files list, workspace-wide search, and safe recovery/autosave.

Export exists but is single-scene only in the GUI. The dialog supports Image/Video/GIF/WebM/MOV/WebP in `crates/animatix-gui/src/app/shell/export_dialog.rs:9`, progress/cancel in `crates/animatix-gui/src/app/shell/export_dialog.rs:185`, and renderer background threads in `crates/animatix-gui/src/app/shell/export_dialog.rs:827`. But `start_export` clones only `document.timeline` and fails otherwise in `crates/animatix-gui/src/app/shell/export_dialog.rs:735`, so multi-scene composition export is unavailable from the GUI despite CLI support.

Performance work exists but remains risky. Rebuilds are debounced at `150ms` by default in `crates/animatix-gui/src/app/stores/ui_store.rs:133`, preview renders only when dirty in `crates/animatix-gui/src/app/runtime.rs:291`, and caches avoid repeated actor/keyframe collection in `crates/animatix-gui/src/app/stores/source_store.rs:61`. But all rebuilds are synchronous on the UI frame in `crates/animatix-gui/src/app/mod.rs:398`, and timeline rendering iterates every actor/action/keyframe in immediate mode in `crates/animatix-gui/src/app/panels/timeline_panel.rs:630`.

## Specific Flaws (with file:line references)

### Critical

- `crates/animatix-gui/src/app/shell/export_dialog.rs:735` — GUI export rejects multi-scene compositions because it only clones `document.timeline`; composition documents hit `No timeline to export` at `crates/animatix-gui/src/app/shell/export_dialog.rs:738`.
- `crates/animatix-gui/src/app/shell/export_dialog.rs:359` — export duration preview also reads only `document.timeline`, so even if export were fixed, auto-duration UI is wrong for compositions.
- `crates/animatix-gui/src/app/panels/behavior.rs:94` — preview panel receives `document.timeline.as_ref()` even for compositions, so many preview editing paths see `None` while the renderer is showing a composition.
- `crates/animatix-gui/src/app/preview/drag_handler.rs:30` — locked-state checks use only `ctx.timeline`; in compositions, locked actors can be selected/dragged because the active scene timeline fallback is not used.
- `crates/animatix-gui/src/app/preview/drag_handler.rs:42` — vertex editing uses only `ctx.timeline`, so polygon/path vertex workflows fail in multi-scene documents.
- `crates/animatix-gui/src/app/preview/drag_handler.rs:244` — motion-path keyframe hit-testing uses only `ctx.timeline`, so motion-path editing is disabled in compositions.
- `crates/animatix-gui/src/app/preview/drag_handler.rs:612` — layout reorder drag uses only `ctx.timeline`, so canvas drag-to-reorder does not work for composition scenes.
- `crates/animatix-gui/src/app/actions/mod.rs:173` — in-memory preview mutation for property edits only updates `document.timeline`, so composition property drags do not update the currently rendered scene immediately.
- `crates/animatix-gui/src/app/actions/mod.rs:93` — child-order drag mutation only targets `document.timeline`, so composition layout reordering cannot preview live.
- `crates/animatix-gui/src/app/document_controller.rs:70` — "insert into selected container" checks only `document.timeline`; creating actors inside containers fails in composition scenes.
- `crates/animatix-gui/src/app/document_controller.rs:756` — unique label generation checks only `document.timeline`, so composition documents can generate labels that collide in the active scene or across raw AST.
- `crates/animatix-gui/src/app/runtime.rs:299` — renderer supports compositions, which makes the above composition-edit/export gaps especially dangerous: the preview looks functional while editing/export commands are partially single-scene.

### Warnings

- `crates/animatix-gui/src/app/handlers/file.rs:145` — save writes `editor.text()` to disk but does not rebuild or update `raw_statements`; saving immediately after a text edit can persist source that the inspector/timeline still hasn't parsed.
- `crates/animatix-gui/src/app/handlers/playback.rs:49` — editor changes clear diagnostics immediately and schedule rebuild, causing a short "no errors" state even when the current source is invalid.
- `crates/animatix-gui/src/app/mod.rs:398` — pending rebuild runs synchronously in the UI frame; large imports, components, or heavy timeline builds can freeze typing/playback.
- `crates/animatix-gui/src/document.rs:181` — rebuild short-circuits on source hash and existing output; it ignores external imported file changes unless the module graph invalidates exactly as expected.
- `crates/animatix-gui/src/document.rs:338` — expanded component AST is reused when component definitions hash unchanged, but non-component module values/actions may still affect expansion/build behavior.
- `crates/animatix-gui/src/app/mod.rs:382` — playback ticks before pending rebuild; during rapid edits, audio/preview/playhead can advance against a stale or invalid timeline.
- `crates/animatix-gui/src/app/runtime.rs:430` — audio sync recomputes all audio segments each frame; this can become expensive in large compositions and has no visible waveform/timing UI.
- `crates/animatix-gui/src/app/runtime.rs:91` — global undo/redo shortcuts intentionally bypass `egui_wants_keyboard_input`, risking conflict with text-editor-native undo semantics.
- `crates/animatix-gui/src/app/handlers/ui.rs:41` — undo restores only source text, not selection, active scene, playhead time, panel state, loop region, or pending drag state.
- `crates/animatix-gui/src/app/handlers/ui.rs:55` — undo sets source/editor text but does not immediately clear stale `raw_statements`, `source_index`, or hit-region caches before rebuild.
- `crates/animatix-gui/src/app/panels/timeline_panel.rs:920` — keyframe dragging snaps to hardcoded 60fps unless Shift is held; projects targeting 24/25/30fps will get unexpected timing.
- `crates/animatix-gui/src/app/panels/timeline_panel.rs:406` — playback speed is a four-state cycle, not a real control; no custom speed, reverse, or reset affordance.
- `crates/animatix-gui/src/app/panels/timeline_panel.rs:475` — ruler tick interval is hardcoded by total duration, not by zoom level, so zoomed timelines can show too few/misleading ticks.
- `crates/animatix-gui/src/app/panels/timeline_panel.rs:955` — track bar comments say scrubbing was removed except ruler; this makes empty timeline space non-interactive, unlike most animation tools.
- `crates/animatix-gui/src/app/panels/preview_panel.rs:54` — preview canvas size is allocated to fitted scene size, not all available space; large panes can contain unused space and small panes clamp to minimums.
- `crates/animatix-gui/src/app/panels/preview_panel.rs:51` — preview uses hard minimums of `200x180`, which can break very small tile layouts.
- `crates/animatix-gui/src/app/runtime.rs:19` — initial window size is hardcoded to `1440x960`; there is no restoration of last native window size.
- `crates/animatix-gui/src/app/mod.rs:559` — welcome card max width is fixed at `280px`, cramped for translated/long paths and not adaptive.
- `crates/animatix-gui/src/app/shell/export_dialog.rs:72` — export dialog dimensions are percentage-clamped but not scroll-backed at the outer level; small screens can hide settings/action controls.
- `crates/animatix-gui/src/app/shell/export_dialog.rs:244` — cancel sets a flag and hides the dialog but does not join or expose cleanup state; failed/cancelled background work can become invisible.
- `crates/animatix-gui/src/app/panels/sidebar.rs:351` — opening another file from Explorer has no unsaved-change prompt, despite `DocumentSession.is_dirty`.
- `crates/animatix-gui/src/app/mod.rs:279` — hot reload blocks external reload when dirty, but manual open/reload flows do not provide equivalent conflict UX.
- `crates/animatix-gui/src/app/preview/context.rs:586` — motion paths render only when `self.timeline` is `Some`; selected actors in compositions lose motion-path overlays.
- `crates/animatix-gui/src/app/preview/context.rs:691` — vertex handles use only `self.timeline`; composition scene vertex handles are not drawn.
- `crates/animatix-gui/src/app/preview/context.rs:735` — prev/next ghost overlays use only `self.timeline`; composition scenes lose onion-skin guidance.
- `crates/animatix-gui/src/app/preview/context.rs:285` — locked filtering uses only `self.timeline`; in compositions, context-menu selection does not honor locks.
- `crates/animatix-gui/src/app/panels/sidebar.rs:540` — layers tab correctly falls back to active scene timeline; this inconsistency with preview/timeline/edit handlers is a design smell.
- `crates/animatix-gui/src/app/panels/inspector/mod.rs:80` — inspector has custom composition fallback logic, confirming timeline resolution is duplicated across panels instead of centralized.
- `crates/animatix-gui/src/app/panels/inspector/mod.rs:641` — color display uses hex strings even though `.amx` does not support hex source colors; this teaches users invalid DSL syntax.
- `crates/animatix-gui/src/app/panels/inspector/property_groups.rs:657` — color editor edits raw RGBA but does not preserve semantic tokens like `accent.primary`, `auto`, or named colors.
- `crates/animatix-gui/src/app/panels/inspector/property_groups.rs:740` — text/source fields are single-line, inadequate for `Code`, `Math`, Typst, long strings, and asset paths.
- `crates/animatix-gui/src/app/panels/inspector/mod.rs:1043` — pivot offsets are UI-only; they are useful for transform manipulation but not persisted into source or timeline semantics.
- `crates/animatix-gui/src/app/panels/timeline_panel.rs:849` — keyframe diamonds are deduped per actor/property time, so multiple properties at the same timestamp collapse visually and cannot be independently understood without hover/context.
- `crates/animatix-gui/src/app/stores/history_store.rs:21` — undo limit is fixed at 100 and not configurable; large animation sessions can exceed it quickly.
- `crates/animatix-gui/src/app/runtime.rs:364` — hit regions are derived from the last rendered frame; selection/zoom commands immediately after source edit may operate on stale geometry until preview render completes.
- `crates/animatix-gui/src/preview_surface.rs:135` — preview allocates multiple full-resolution GPU textures per document resolution; 4K compositions/transitions/filters can consume large VRAM without visible budget feedback.
- `crates/animatix-gui/src/preview_surface.rs:246` — transition preview merges hit regions from both scenes during blends; selection can target outgoing/incoming actors without clear layer/scene disambiguation.

### Suggestions

- `crates/animatix-gui/src/app/panels/behavior.rs:140` — timeline panel already resolves active-scene timeline; extract this into a shared `DocumentSession::editable_timeline()` / `editable_timeline_mut()` API and use it everywhere.
- `crates/animatix-gui/src/app/preview/context.rs:44` — `get_actor_props_at_time` has the right timeline fallback pattern; reuse this pattern for locks, vertex handles, motion paths, ghost overlays, and drag handlers.
- `crates/animatix-gui/src/app/shell/export_dialog.rs:735` — support both `BuildTarget::SingleScene` and `Composition` in GUI export, mirroring CLI export behavior.
- `crates/animatix-gui/src/app/mod.rs:398` — move rebuild to a background job with generation tokens so typing, playback, diagnostics, and preview remain responsive.
- `crates/animatix-gui/src/app/stores/ui_store.rs:130` — make frame rate/grid/snap settings project-level and use them for keyframe drag snapping instead of hardcoded 60fps.
- `crates/animatix-gui/src/app/panels/timeline_panel.rs:955` — restore click/drag scrubbing on the full track area, with modifiers for selection vs scrub.
- `crates/animatix-gui/src/app/panels/timeline_panel.rs:849` — add expandable per-property lanes and stacked diamonds for same-time multi-property keyframes.
- `crates/animatix-gui/src/app/panels/inspector/mod.rs:1003` — replace UI-only pivot with explicit transform-origin/source support or label it as a temporary manipulation handle.
- `crates/animatix-gui/src/app/panels/sidebar.rs:351` — add dirty-file confirmation before open/reload/switch workspace.
- `crates/animatix-gui/src/app/shell/export_dialog.rs:185` — keep cancelled export visible until worker confirms cancellation, and show final state in the dialog/toast.
- `crates/animatix-gui/src/app/components/diagnostics.rs:1` — add inline editor squiggles and preview badges for diagnostics instead of relying only on a bottom panel.
- `crates/animatix-gui/src/app/design_tokens.rs:183` — expose density/accessibility scaling; many row heights and fonts are optimized for dense desktop use but not accessibility.
- `crates/animatix-gui/src/app/runtime.rs:19` — persist native window size and maximized state alongside tile layout.
- `crates/animatix-gui/src/app/panels/preview_panel.rs:293` — make Time Lens discoverable with a visible affordance; Space-drag hidden gestures are not enough.
- `crates/animatix-gui/src/app/panels/sidebar.rs:98` — split Editor out of Sidebar or make the layout default reflect the DSL-first workflow: source left, preview center, inspector right, timeline bottom.
- `crates/animatix-gui/src/app/panels/inspector/keyframe_table.rs:1` — evolve keyframe list/curve modes into a real dope sheet/F-curve editor with interpolation handles and batch operations.

## Bold Redesign Proposals

1. **Source-first After Effects for layout animation.** Keep the DSL as the primary artifact, but add a visual dope sheet where every edit maps to a clear source diff. The killer feature should be bidirectional source/visual editing with no hidden binary project format.

2. **Real time ruler + dope sheet as the bottom panel.** Rows: scene, actor groups, actors, expandable property lanes. Keyframes: draggable, box-selectable, copy/pasteable, scalable in time, colored by property category. Action blocks and keyframes coexist but are visually distinct.

3. **Property spreadsheet view.** For layout-first animation, users often tune many actors at once: positions, sizes, gaps, alignments, colors, timings. A spreadsheet with rows=actors, columns=properties would make Animatix stand out from canvas-only tools.

4. **Source Diff Preview sidecar.** When dragging an actor or changing inspector values, show the `.amx` statement that will be written. Turns source normalization from a surprise into a trust-building workflow.

5. **Node/layer compositing view for scene flow only.** A node graph for scenes, transitions, imports, components, filters, and render/export targets complements the DSL without undermining the layout-first model.

6. **Layout debugger overlay.** Since Animatix is layout-first, show container bounds, layout slots, intrinsic sizes, taffy-admitted/excluded children, padding/gaps, and child order tracks directly on the canvas.

7. **Animation "map" view.** Show every animated property as a compact heatmap by time, actor, and property category. Ideal for large generated `.amx` files.

8. **Component authoring tools.** Components are a major language feature; expose component instances, params, slots, and nested labels as first-class editable structures with jump-to-definition and safe parameter editing.

9. **Scene/storyboard mode.** Multi-scene composition should have a storyboard strip with thumbnails, transition badges, durations, and drag-to-reorder. Current scene list and timeline blocks are not enough for production editing.

10. **Performance HUD and render quality controls.** Show rebuild time, render time, GPU texture memory estimate, dropped frames, filter cost, and draft/full preview quality toggle. Large animation workflows need feedback before they feel broken.

## Recommended Priorities

### P0 — Fix correctness and trust

1. Make all editing/export paths composition-aware. Centralize active timeline resolution and mutation so preview, inspector, layers, timeline, drag handlers, creation, labels, source edits, and export all work on the same scene model.
2. Fix GUI export for compositions by routing to composition export APIs and using composition duration in the export dialog.
3. Add unsaved-change prompts for open/reload/workspace switch and visible dirty state in the toolbar/title.
4. Prevent stale-state edits after undo/editor changes by invalidating AST/source indexes/hit caches immediately before the debounced rebuild.
5. Stop clearing diagnostics optimistically on edit; instead mark them stale until rebuild succeeds or fails.

### P1 — Make the animation workflow credible

1. Build a real timeline/dope sheet with property lanes, box selection, keyframe copy/paste, frame-rate-aware snapping, and visible easing.
2. Make playback controls frame-accurate: FPS setting, frame step, timecode, reverse/ping-pong, loop in/out markers, and play selection.
3. Improve inspector editing with mixed-value multi-select, semantic color/token preservation, multiline text/code/math editors, and expression-aware fields.
4. Add inline diagnostics/squiggles and preview badges so errors are contextual.
5. Add storyboard/scene thumbnails and transition editing for multi-scene workflows.

### P2 — Improve architecture and performance

1. Move rebuilds off the UI thread with cancellation/generation tokens and stale-preview indication.
2. Normalize state management around derived-state caches: source is canonical; AST/timeline/composition/hit regions are derived and versioned.
3. Replace ad hoc egui temp state with explicit persistent `egui::State`/store structs for timeline selections, drag operations, dialogs, and editor modes.
4. Add performance instrumentation and a preview quality toggle for heavy filters, plots, imports, and high-resolution scenes.
5. Persist window size, workspace layout, timeline zoom/scroll, and project playback settings.

### P3 — Differentiators

1. Add layout debugger and property spreadsheet.
2. Add component/slot authoring UI.
3. Add source-diff preview for visual edits.
4. Add graph/storyboard view for scene composition.
5. Add animation heatmap for generated/large projects.
