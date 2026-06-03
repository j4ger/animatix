# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Order

1. **Phase 0.5** Multi-Scene Polish
2. **Phase 1** Inspector Core Completeness — *users can't do basic things*
3. **Phase 2** Animation Power Tools — *power-user differentiators*
4. **Phase 3** Asset & Component Integration
5. **Phase 4** PiP / Multi-Viewport
6. **Phase 5** Editor Infrastructure (green tree, WASM)
7. **Phase 6** QoL & Polish

---



## Phase 0.5 — Multi-Scene Polish

> Small remaining Phase 0 work before moving to deeper feature gaps.

| # | Item | What | Files | Effort |
|---|------|------|-------|--------|
| 0.5.1 | **Transition editor UI** | Visual editing of `play` edge transitions (type, duration, easing) in the inspector transition card. Dropdown for transition type + duration field. | `app/panels/inspector/mod.rs` | 1 day |
| 0.5.2 | **Scene duration editing** | Add `duration` property to scene declarations (currently implicit). Inspector shows editable duration field. | `app/panels/inspector/`, `timeline/` | 1 day |
| 0.5.3 | **Scene duplicate / delete** | Context menu items in scene list for duplicating (copy AST + rename) and deleting scenes. | `app/panels/sidebar.rs`, `app/commands.rs` | 1 day |
| 0.5.4 | **Scene block drag in timeline** | Drag scene blocks in the composition timeline to change start times. | `app/panels/timeline_panel.rs` | 2 days |

---

## Phase 1 — Inspector Core Completeness

> **Theme: "Users can't do basic things."** Backend support exists but has zero GUI exposure. These are the highest-impact gaps.

| # | Item | What | Files | Effort | Backend? |
|---|------|------|-------|--------|----------|
| 1.1 | **Filter actor properties** | Add blur, brightness, contrast, saturate, hue-rotate, sepia to `property_groups.rs` so Filter actors are editable in the inspector. | `app/panels/inspector/property_groups.rs`, `timeline/property_registry.rs` | 1 day | ✅ |
| 1.2 | **Audio actor properties** | Audio source path, volume, start time, duration fields in inspector. | `app/panels/inspector/property_groups.rs` | 1 day | ✅ |
| 1.3 | **Per-actor visibility toggle** | Eye icon in sidebar layers list toggles a visibility flag (not just opacity keyframe). Inspector shows visibility checkbox. | `app/panels/sidebar.rs`, `app/panels/inspector/` | 1 day | ⚠️ needs flag |
| 1.4 | **Actor lock** | Lock icon in sidebar prevents selection/drag in preview. Inspector shows lock toggle. | `app/panels/sidebar.rs`, `app/preview/selection.rs` | 1 day | ⚠️ needs flag |
| 1.5 | **Runtime diagnostics panel** | Wire `Timeline::runtime_diagnostics()` into the existing diagnostics component. Show modifier/runtime errors per frame, not just build/parse errors. | `app/components/diagnostics.rs`, `app/runtime.rs` | 1 day | ✅ |
| 1.6 | **Parenting drop-down** | Inspector shows current parent label + dropdown to reparent to another actor or "None" (root). | `app/panels/inspector/mod.rs` | 1 day | ✅ |

---

## Phase 2 — Animation Power Tools

> **Theme: "Power-user differentiators."** Standard animation editor features that Animatix lacks.

| # | Item | What | Files | Effort | Backend? |
|---|------|------|-------|--------|----------|
| 2.1 | **Easing curve editor** | Custom egui widget: interactive bezier handle editor for easing curves. Replaces the 8-preset dropdown. Store custom curves in a user-defined easing registry. | `app/components/`, `app/panels/inspector/` | 2–3 days | ⚠️ needs registry |
| 2.2 | **Multi-property graph editor** | Show multiple F-curves (e.g. position X + Y) in the same graph view with color-coded lines. Toggle per-property visibility. | `app/panels/inspector/graph_editor.rs` | 2 days | ✅ |
| 2.3 | **Motion path editing** | Render position keyframes as an editable spatial path on the preview canvas. Drag control points to move keyframes in 2D space. | `app/preview/overlay.rs`, `app/preview/drag_handler.rs` | 2–3 days | ✅ |
| 2.4 | **Keyframe bulk operations** | Multi-select keyframes (Shift+click) → delete, move, copy/paste, set easing for all selected. | `app/panels/timeline_panel.rs` | 2 days | ✅ |
| 2.5 | **Graph editor for Vec2/Color** | Extend graph editor to show Vec2 components (X/Y lines) and RGBA channels. | `app/panels/inspector/graph_editor.rs` | 2 days | ✅ |
| 2.6 | **Snap guides magnet snapping** | When dragging actors, snap to edges/center of other actors, not just grid and guides. | `app/preview/drag_handler.rs` | 1 day | ✅ |

---

## Phase 3 — Asset & Component Integration

> **Theme: "The system is bigger than the editor."** Components, colorschemes, modules, and assets have full backend support but no GUI.

| # | Item | What | Files | Effort | Backend? |
|---|------|------|-------|--------|----------|
| 3.1 | **Colorscheme picker** | Dropdown in settings or inspector to select from built-in schemes (`default-dark`, `default-light`, `editorial-dark`). Live preview of scheme tokens. | `app/shell/settings.rs`, `app/panels/inspector/` | 1 day | ✅ |
| 3.2 | **Component instantiation palette** | Palette mode or sidebar section listing available components from loaded modules. Click to instantiate with default props. | `app/shell/insertion_palette.rs`, `app/panels/sidebar.rs` | 2 days | ✅ |
| 3.3 | **Component definition browser** | Read-only tree of `pub component` definitions from all imported modules. Shows params and body preview. | `app/panels/sidebar.rs` | 1 day | ✅ |
| 3.4 | **SVG import UI** | Drag-and-drop or file picker to import SVG files. Creates an SVG actor with the imported paths. | `app/panels/preview_panel.rs`, `timeline/svg_import.rs` | 1 day | ✅ |
| 3.5 | **Image asset manager** | Visual grid of loaded images from `AssetCache`. Shows filename + thumbnail. Click to create an Image actor with that asset. | `app/panels/sidebar.rs` | 2 days | ✅ |
| 3.6 | **Module import UI** | "Import module" button in sidebar that opens a file picker, inserts `import "path"` into AST, and rebuilds. | `app/panels/sidebar.rs`, `app/commands.rs` | 1 day | ✅ |

---

## Phase 4 — PiP / Multi-Viewport

> **Deferred.** The current viewport system has been removed. PiP will be implemented as an actor-level `Scene` primitive, not statement-level declarations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Design `Scene` primitive** | Actor type whose content is another scene's timeline. Position, size, opacity are animatable properties (keyframes). `scene` property names the scene to render. | `primitives/`, `timeline/track.rs` | 3 days | Stable syntax |
| 2 | **Scene reference rendering** | Renderer evaluates referenced scene timeline at current time, clips to actor bounds, transforms to actor position, applies actor opacity. | `timeline/scene_eval.rs`, `renderer/` | 1 week | 1 |
| 3 | **Inspector + timeline support** | Scene actors show up in timeline tracks, inspector panel, and gizmo selection like any other actor. | `app/panels/` | 3 days | 2 |

---

## Phase 5 — Editor Infrastructure

> Long-term foundational work. Blocked on syntax stabilization.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 1 |
| 3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |
| 4 | **Snippet AST parsing** | Parse snippet text into `Vec<Stmt>` and insert via `SourceEdit` instead of raw text surgery. Requires lossless parsing (green tree) to preserve formatting. | `app/insertion.rs`, `animatix-green/` | 2 days | 2 |

---

## Phase 6 — QoL & Polish

> Small quality-of-life improvements that add up to a polished experience.

| # | Item | What | Files | Effort |
|---|------|------|-------|--------|
| 6.1 | **Command palette** | `Cmd+Shift+P` searchable palette for all commands (undo, redo, export, save, scene switch, etc.). Uses existing `Command` enum. | `app/shell/`, `app/mod.rs` | 1 day |
| 6.2 | **Zoom-to-selection** | `F` key frames selected actors in view. `Shift+F` fit all actors. | `app/preview/mod.rs`, `app/runtime.rs` | 1 day |
| 6.3 | **Font preview in picker** | Font dropdown shows a small glyph preview next to each family name. | `app/panels/inspector/property_groups.rs` | 1 day |
| 6.4 | **Snippets implementation** | Complete the stubbed snippet insertion in the palette. Parse snippet text and insert via `SourceEdit`. | `app/shell/insertion_palette.rs`, `animatix-analyzer/` | 1 day |
| 6.5 | **Export format expansion** | Expose WebM, MOV, APNG, WebP in export dialog. Encoder already supports these codecs. | `app/shell/export_dialog.rs` | 1 day |
| 6.6 | **Export quality presets** | "720p / 30fps", "1080p / 60fps", "4K / 60fps" quick-select buttons in export dialog. | `app/shell/export_dialog.rs` | 1 day |
| 6.7 | **Alignment tools** | Align left/center/right, distribute horizontally/vertically toolbar buttons. Multi-select required. | `app/preview/drag_handler.rs` | 1 day |
| 6.8 | **Group / Ungroup** | `Ctrl+G` groups selected actors into a `Group` container. `Ctrl+Shift+G` ungroups. | `app/handlers/actor.rs` | 1 day |
| 6.9 | **Amber flash on rewritten timestamps** | When `adjust_following_relative_keyframe` rewrites a relative offset, flash the timestamp label amber for ~300ms. | `app/panels/timeline_panel.rs` | 1 day |
| 6.10 | **Find / Replace** | `Ctrl+F` find/replace in source editor with regex support. | `app/panels/editor.rs` | 1 day |

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 5 / green tree) | 5 |
| `let` variable animation | Superseded by easing functions in `always` blocks (6.8.3). Keyframed `let` tracks would need new timeline infrastructure; `always` lerp covers the same use cases statelessly. | Post-5 |
| **AI / NL Integration** | Requires external AI service (OpenAI, Claude, local LLM). No runtime dependency on AI should be mandatory. Includes: NL command bar, agent suggestion UI, agent_suggestions component. | Post-5 or separate product |
| **Row double-click / right-click** | No defined user story. Fields were wired to egui events but no caller consumed them. Re-add when a feature needs them. | When needed |
| **Badge button component** | Fully implemented but no caller. Re-add when the UI needs count badges (e.g. "Errors: 3"). | When needed |
| **Pre-compile plot closures** | Compile `func` AST bodies to closures/bytecode once per build instead of tree-walking thousands of times per curve. Would give 10–50× sampling speedup but requires a stable closure compilation API. | Post-5 or when plot count becomes a bottleneck again |
| **Unify duplicate PropertyValue types** | Two separate `PropertyValue` enums exist: `animatix::timeline::property_engine::PropertyValue` (engine-level) and `animatix_gui::app::commands::PropertyValue` (GUI-level). Different variant names (`F32` vs `Float`, `String` vs `Text`) force conversion logic in `apply_property_edit_to_track`. Unify into one canonical type. | When touching property dispatch again |
| **Replace `node_local_bounds` with trait-based bounds** | `node_local_bounds` takes `&[VelloPath]` forcing callers to materialize paths just for bounds computation. A `trait HasLocalBounds` on `VelloPath`/`TextPath`/`SceneImage` would be cleaner and allow lazy evaluation. | When touching scene_eval bounds logic |
| **Zero-readback filter compositing (end-to-end)** | Infrastructure is complete: `FullscreenBlitPipeline` supports alpha, `GpuFilterBackend` exposes `render_and_filter_scene_to_view()` and `take_last_filtered_view()`. Remaining work: modify `scene_eval.rs` to not draw filtered images into the Vello scene, and update `PreviewSurface`/`OffscreenRenderer` to blit the GPU texture after the base Vello render. `FilteredSource` tracking should be simplified to avoid fragile pointer comparison. | When filter performance matters |
| **Audio playback in preview** | Audio segments are collected for export muxing but not played back during GUI preview. Requires an audio output backend (rodio/cpal). | Post-1 or separate feature |
| **Variable track UI** | `let` declarations inside keyframes create `VariableTrack` entries. No GUI to view or edit these. Advanced feature, low demand. | When needed |
| **Module dependency graph** | Visual graph of imports between `.amx` files. Internal tooling feature. | When needed |
