# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 1 — GUI Bug Fixes

Fix critical UX bugs and eliminate duplication that directly impacts users.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1.1 | **Fix `S` key collision** | Assign unique hotkeys: `Y` for editor sync, keep `S` for scale tool. Remove dead scale tool assignment. | `app/mod.rs:348–364` | 1 hour | — |
| 1.2 | **Integrate or remove transport bar** | Either wire `transport_bar_ui()` into `mod.rs` and remove duplicate timeline controls, or delete `transport_bar.rs`. | `app/shell/transport_bar.rs`, `app/panels/timeline_panel.rs:128–310` | 1 day | — |
| 1.3 | **Replace all silent failures** | Replace `let _ =` with `if let Err(e)` + `tracing::warn!` + toast notifications for user-facing ops (save, reload, rebuild, persistence). | `app/mod.rs`, `app/command_handlers.rs` | 1 day | — |
| 1.4 | **Extract property collection iterator** | Merge `collect_kf_props!` and `collect_kf!` macros into a single `TrackKeyframeIter` in core timeline. | `app/panels/timeline_panel.rs:643–677,813–841` | 2 days | — |
| 1.5 | **Standardize empty states** | Replace all inline empty state renders with `components::empty_state()`. | `app/panels/mod.rs`, `app/panels/inspector/mod.rs`, `app/panels/inspector/keyframe_table.rs` | 1 day | — |
| 1.6 | **Increase minimum hit targets** | Expand hit rects to 16×16px for keyframe diamonds, 12×20px for range handles, 18×18px for inspector KF button. | `app/panels/timeline_panel.rs`, `app/panels/inspector/property_groups.rs` | 1 day | — |
| 1.7 | **Tokenize hardcoded action colors** | Replace raw RGB in `action_category_color()` with existing design tokens. Increase block opacity from 0.4 to 0.6. | `app/panels/timeline_panel.rs:40–50,621` | 2 hours | — |

---

## Phase 2 — Structural Decoupling

Decompose god objects and restructure state before feature work continues.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 2.1 | **Extract `DocumentController`** | Move all AST mutation methods (`handle_duplicate_actor`, `paste_actors`, `handle_set_*`, etc.) out of `GuiShell` into a dedicated controller. | `app/mod.rs:750–1160` | 3 days | — |
| 2.2 | **Extract `PlaybackController`** | Move time scrubbing, play/pause, keyframe jumping, loop logic out of `GuiShell`. | `app/mod.rs:150–210,621–644` | 2 days | — |
| 2.3 | **Split `PreviewPaneState`** | Decompose into `PlaybackState`, `ViewportState`, `GuideState`, `SnapState`, `OverlayState`. | `app/mod.rs:75–112` | 2 days | 2.2 |
| 2.4 | **Split `UiStore`** | Group into `SelectionStore`, `InteractionStore`, `ClipboardStore`, `ViewStore`. | `app/stores/ui_store.rs` | 2 days | — |
| 2.5 | **Replace custom timeline scroll** | Swap manual scroll impl for `egui::ScrollArea::vertical().show_rows()`. | `app/panels/timeline_panel.rs:88–106,992–1007` | 2 days | — |
| 2.6 | **Extract scene block renderer** | Deduplicate scene block rendering between transport bar and timeline panel. | `app/shell/transport_bar.rs:466–498`, `app/panels/timeline_panel.rs:461–496` | 1 day | 1.2 |
| 2.7 | **Move AST utilities out of app module** | Relocate `find_keyframes_for_actor`, `shift_keyframe_times`, etc. to `source_edit` or `animatix::ast`. | `app/mod.rs:1226–1307` | 1 day | — |
| 2.8 | **Move `collect_all_keyframe_times`** | Relocate from inspector to `animatix::timeline` to fix preview→inspector illegal dependency. | `app/panels/inspector/`, `app/panels/preview_canvas/mod.rs:373` | 1 day | — |
| 2.9 | **Decompose `WorkspaceViewer`** | Split 25-field struct into per-panel contexts (`SidebarContext`, `InspectorContext`, `TimelineContext`, `PreviewContext`). | `app/panels/mod.rs:65–100` | 3 days | 2.4 |

---

## Phase 3 — Quality & Polish

Unify components, enforce command compliance, and cache hot-path allocations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 3.1 | **Unify button components** | Refactor transport bar / timeline to use `toolbar_action_button` / `toolbar_toggle_button` from `components/mod.rs`. Remove manual button construction. | `app/shell/transport_bar.rs`, `app/panels/timeline_panel.rs` | 2 days | 1.2 |
| 3.2 | **Extract `TransportScrubber` struct** | Replace 290-line `paint_transport_scrubber` with a struct holding state + `.show(ui)` method. | `app/shell/transport_bar.rs:440–733` | 1 day | 1.2 |
| 3.3 | **Extract `play_pause_button()`** | Deduplicate play/pause icon logic across toolbar, transport, timeline, preview. | `app/components/mod.rs`, 4 call sites | 2 hours | — |
| 3.4 | **Extract `default_actor_type()`** | Move duplicate definition to shared `app::utils` or `animatix::primitives`. | `app/panels/mod.rs:7`, `app/panels/inspector/mod.rs:30` | 1 hour | — |
| 3.5 | **Introduce `CommandResult`/`Effect`** | Make side effects explicit: commands mutate state, a separate effect system handles toasts/status updates. | `app/commands.rs`, `app/command_handlers.rs` | 3 days | 2.1 |
| 3.6 | **Cache hot-path allocations** | Cache `actor_labels` and keyframe collections on `DocumentStore`, invalidate on rebuild. | `app/stores/document_store.rs`, `app/panels/timeline_panel.rs` | 2 days | — |
| 3.7 | **Timeline command compliance** | Ensure all time changes emit `Command::ScrubTo` instead of direct `preview.current_time_s` mutation. | `app/panels/timeline_panel.rs` | 1 day | — |
| 3.8 | **Move egui temp data to stores** | Persist sidebar tab, property view mode, keyframe view mode in `UiStore` instead of egui temp. | `app/panels/mod.rs`, `app/panels/inspector/mod.rs` | 1 day | 2.4 |
| 3.9 | **Fix flat input affordance** | Add subtle bottom border or background to inspector input widgets. | `app/panels/inspector/property_groups.rs:366–378` | 1 day | — |
| 3.10 | **Consolidate preview HUDs** | Merge context + hover HUDs into one adaptive overlay. Move property popup to inspector or make dismissable. | `app/panels/preview_canvas/mod.rs:501–630` | 2 days | — |
| 3.11 | **Add sticky section headers** | Make inspector card headers sticky during scroll so users don't lose context. | `app/panels/inspector/mod.rs:135–346` | 1 day | — |
| 3.12 | **Add track collapse/expand** | Add chevron to collapse individual actor tracks in timeline. | `app/panels/timeline_panel.rs:560–883` | 1 day | 2.5 |
| 3.13 | **Add explorer search/filter** | Filter text box for file tree in sidebar explorer. | `app/panels/mod.rs:217–267` | 2 days | — |
| 3.14 | **Tokenize hardcoded spacing** | Replace raw `ui.add_space(N.N)` calls across all panels with `SPACE_*` tokens. | All panel files (~40 locations) | 1 day | — |
| 3.15 | **Standardize empty state sizing** | Create `EMPTY_STATE_ICON_SIZE` token, apply consistently. | `app/components/mod.rs`, `app/panels/` | 2 hours | — |

---

## Phase 4 — Testing

Add unit tests for extracted pure functions and store mutations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 4.1 | **Unit tests for pure functions** | Test `nice_tick_interval`, `clamp_pan`, `PreviewPaneState::clamp_time`, `timeline_fraction`, `time_from_pointer_x`. | `app/panels/mod.rs`, `app/preview/` | 2 days | — |
| 4.2 | **Command handler tests** | Test command dispatch using a test harness with mock stores. | `app/command_handlers.rs` | 2 days | 3.5 |
| 4.3 | **Store state tests** | Test store mutations (undo/redo, dirty tracking, persistence round-trip). | `app/stores/` | 2 days | 2.3, 2.4 |

---

## Phase 5 — Multi-Viewport / PiP

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 5.1 | **Explicit `Viewport` type** | New AST node + primitive for viewport rectangles with position, size, opacity, border, mask, and scene assignment. | `ast.rs`, `primitives/` | 1 week | — |
| 5.2 | **Viewport tracks in timeline** | Timeline shows viewport tracks with scene blocks (like current scene row but for viewports). | `timeline/build.rs`, `timeline/track.rs` | 2 weeks | 5.1 |
| 5.3 | **Composite rendering** | Renderer composites multiple viewport scenes into a single frame. Each viewport renders its assigned scene at its rectangle. | `renderer/core.rs`, `renderer/offscreen.rs` | 2–3 weeks | 5.2 |
| 5.4 | **Viewport selection + gizmo** | Click viewport border → select, show move/resize gizmo. Double-click → enter scene editing inside. | `app/panels/preview_canvas/` | 1 week | 5.3 |

---

## Phase 6 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 6.1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 6.2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 6.1 |
| 6.3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |

---

## Phase 7 — Agent / NL Integration

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 7.1 | **NL command bar dispatch** | Send NL input to an external AI service, parse structured response into `Command` queue. | `app/shell/nl_command_bar.rs`, `app/commands.rs` | 1 week | External AI service |
| 7.2 | **Agent suggestion UI** | Inline suggestion widget that proposes edits (e.g. "Add fade-in to Circle_1"). User accepts/rejects with keyboard shortcut. | `app/components/agent_suggestions.rs` | 3 days | 7.1 |

---

## Phase 8 — Audio

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 8.1 | **Audio multi-segment muxing** | Concatenate multiple audio files via ffmpeg into final output. Support per-scene audio tracks. | `export/ffmpeg.rs` | 3 days | — |

---

## Order

1. **Phase 1** (no blockers — start immediately; user-facing bugs)
2. **Phase 2** (after Phase 1; architectural foundation)
3. **Phase 3** (after Phase 2; quality consolidation)
4. **Phase 4** (after Phase 3; validate refactors)
5. **Phase 8** (no blockers — can do anytime)
6. **Phase 5** (after Phase 4)
7. **Phase 7** (external AI service required)
8. **Phase 6** (start after syntax stabilizes)
