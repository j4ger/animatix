# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 7 — Audio

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 7.1 | **Audio multi-segment muxing** | Concatenate multiple audio files via ffmpeg into final output. Support per-scene audio tracks. | `export/ffmpeg.rs` | 3 days | — |

---

## Phase 8 — PiP / Multi-Viewport

> **Deferred.** The current viewport system has been removed. PiP will be implemented as an actor-level `Scene` primitive, not statement-level declarations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 8.1 | **Design `Scene` primitive** | Actor type whose content is another scene's timeline. Position, size, opacity are animatable properties (keyframes). `scene` property names the scene to render. | `primitives/`, `timeline/track.rs` | 3 days | Stable syntax |
| 8.2 | **Scene reference rendering** | Renderer evaluates referenced scene timeline at current time, clips to actor bounds, transforms to actor position, applies actor opacity. | `timeline/scene_eval.rs`, `renderer/` | 1 week | 8.1 |
| 8.3 | **Inspector + timeline support** | Scene actors show up in timeline tracks, inspector panel, and gizmo selection like any other actor. | `app/panels/` | 3 days | 8.2 |

---

## Phase 9 — GUI Cleanup & Polish

> Code-quality overhaul of the GUI crate. No user-visible features; purely structural.
> See [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md) for the design spec.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 9.1 | **Merge timeline indirection** | `panels/timeline.rs` is a 25-line shim that only forwards to `timeline_panel.rs`. Delete the shim and move the entry point into `timeline_panel.rs`. | `panels/timeline.rs`, `panels/timeline_panel.rs` | 1 hr | — |
| 9.2 | **Delete inspector_panel.rs shim** | `panels/inspector_panel.rs` is a 54-line wrapper around `inspector/mod.rs`. Fold `panel_frame()` and context setup into `inspector/mod.rs` and delete the shim. | `panels/inspector_panel.rs`, `panels/inspector/mod.rs`, `panels/behavior.rs` | 1 hr | — |
| 9.3 | **Remove dead commands** | Eight commands (PrevScene, NextScene, AddScene, DeleteScene, RenameScene, ReorderScenes, ToggleKeyframeMode, RequestRepaint) are defined and dispatched but never emitted from the GUI. Remove them or wire them up. | `commands.rs`, `command_handlers.rs` | 2 hr | — |
| 9.4 | **Split command_handlers.rs** | The 1700-line monolithic dispatcher is a single point of fragility. Extract into `handlers/` subfolder: `file.rs`, `playback.rs`, `actor.rs`, `keyframe.rs`, `scene.rs`, `ui.rs`, `property.rs`. | `command_handlers.rs` → `handlers/*.rs` | 1 day | 9.3 |
| 9.5 | **Split components/mod.rs** | 15 unrelated components in one file. Split into `row.rs`, `button.rs`, `layout.rs`, `diagnostics.rs`, `timeline.rs`. | `components/mod.rs` | 1 day | — |
| 9.6 | **Extract preview drag handler** | `preview_panel.rs` is 1666 lines; ~700 are drag-handling match arms (Move, Scale, Rotate, Reorder, EditVertices, MovePivot). Extract to `preview/drag_handler.rs`. | `panels/preview_panel.rs`, `preview/drag_handler.rs` | 2 days | — |
| 9.7 | **Standardize UI function naming** | Mixed conventions: `toolbar_ui`, `settings_dialog_ui`, `preview_ui`, `timeline_panel_ui`. Standardize on `<noun>_panel_ui` for panels and `<noun>_ui` for dialogs/overlays. | `shell/*.rs`, `panels/*.rs` | 2 hr | — |
| 9.8 | **Remove duplicate panel_frame()** | `sidebar.rs` defines its own `panel_frame()` instead of using `panels/mod.rs::panel_frame()`. Deduplicate. | `panels/sidebar.rs`, `panels/mod.rs` | 30 min | — |
| 9.9 | **Unify time display format** | Toolbar shows `{:.2}s / {:.2}s`; timeline shows `MM:SS.mm`. Pick one format and use it everywhere. | `shell/toolbar.rs`, `panels/timeline_panel.rs` | 30 min | — |
| 9.10 | **Audit remaining magic numbers** | 94 raw `Stroke::new(1.0, ...)` calls and scattered hardcoded sizes. Move remaining literals into `design_tokens.rs`. | Entire `app/` tree | 1 day | — |
| 9.11 | **Consolidate GuiShell impl blocks** | `impl GuiShell` is split across 8 files (`shell/*.rs`, `actions/mod.rs`, `mod.rs`, `command_handlers.rs`). Move core lifecycle into `mod.rs`, keep shell UIs in `shell/`, and keep the thin dispatcher in `command_handlers.rs`. | `app/mod.rs`, `shell/*.rs` | 1 day | — |

---

## Phase 10 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 10.1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 10.2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 10.1 |
| 10.3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |

---

## Order

1. **Phase 7** (audio — no blockers)
2. **Phase 8** (PiP — after syntax and renderer are stable)
3. **Phase 9** (GUI cleanup — small, can be done anytime; ideally before Phase 10)
4. **Phase 10** (start after syntax stabilizes)

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 10 / green tree) | 10 |
| `let` variable animation | Superseded by easing functions in `always` blocks (6.8.3). Keyframed `let` tracks would need new timeline infrastructure; `always` lerp covers the same use cases statelessly. | Post-10 |
| **AI / NL Integration** | Requires external AI service (OpenAI, Claude, local LLM). No runtime dependency on AI should be mandatory. Includes: NL command bar, agent suggestion UI, agent_suggestions component. | Post-10 or separate product |
| **Row double-click / right-click** | No defined user story. Fields were wired to egui events but no caller consumed them. Re-add when a feature needs them. | When needed |
| **Badge button component** | Fully implemented but no caller. Re-add when the UI needs count badges (e.g. "Errors: 3"). | When needed |
| **Pre-compile plot closures** | Compile `func` AST bodies to closures/bytecode once per build instead of tree-walking thousands of times per curve. Would give 10–50× sampling speedup but requires a stable closure compilation API. | Post-10 or when plot count becomes a bottleneck again |
