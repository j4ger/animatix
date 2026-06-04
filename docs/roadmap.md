# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Order

1. **Phase 4** PiP / Multi-Viewport
2. **Phase 5** Editor Infrastructure (green tree, WASM)
3. **Phase 6** QoL & Polish

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
| 6.11 | **Component parameter dialog** | When instantiating a parameterized component (e.g. `MetricCard(title: "Default")`), show a dialog to override params instead of always using defaults. | `app/panels/sidebar.rs`, `app/shell/insertion_palette.rs` | 2 days |
| 6.12 | **Preserve component registry on parse errors** | `DocumentSession::rebuild()` clears `components`/`module_actions` on any parse error. Keep last-known-good registry so the Components tab and palette stay usable while editing. | `document.rs` | 1 day |
| 6.13 | **Lossless config property edits** | `SetConfigProperty` normalizes unquoted identifiers into quoted strings (e.g. `editorial-dark` → `"editorial-dark"`). Preserve the user's original quoting style. | `source_edit/config_edits.rs`, `to_source.rs` | 1 day |

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
| **Scene duration editing** | Add `duration` property to scene declarations (currently implicit). Inspector shows editable duration field. Requires `Stmt::Scene` AST extension. | Phase 5 |
| **Scene block drag in timeline** | Drag scene blocks in the composition timeline to change start times. Start times are derived from walk order + durations; needs design. | Phase 5 |
| **Unify `load_program` return type** | Currently returns a 6-tuple. Replace with a dedicated `LoadedProgramResult` struct for readability and maintainability. | Phase 5 or when touching `document.rs` |
| **Split `SidebarContext` per tab** | `SidebarContext` carries ~20 fields but each tab only needs a subset. Split into focused contexts (e.g. `ExplorerContext`, `ComponentsContext`) to eliminate borrow conflicts and reduce god-struct surface. | When touching sidebar again |
| **AssetCache ↔ timeline cross-reference** | `AssetCache` and timeline tracks store asset data in parallel with no cross-references. The asset manager cannot show "which actors reference this asset" without AST re-scanning. | When touching asset system |
| **Actor property name consistency** | Programmatic insertions use `"path"` for Svg/Image actors, but the test suite sometimes uses `"url"`. Decide on canonical property names and enforce them in both parser and GUI. | When touching primitive schemas |
| **Validate `CreateActor` props** | `DocumentController::handle_create_actor` blindly appends `props` to the actor declaration with no type checking, duplicate detection, or required-field validation. | When touching actor creation |
| **Normalize `ToSource` API** | `stmts_to_source` is a free function but `Expr::to_source()` requires importing the `ToSource` trait. Expose a consistent free-function or trait-based API across all AST nodes. | Phase 5 or when touching `to_source.rs` |
