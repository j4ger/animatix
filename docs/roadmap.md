# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Bug Fixes

No open bugs at this time.

---

## Phase 3.5 — GUI Redesign Completion

Finish the remaining surface-level GUI redesign items that require no backend infrastructure changes.

| Item | What | Files | Effort |
|------|------|-------|--------|
| **Unified gizmo visual** | Replace 8 square handles with unified transform gizmo: move arrow on body, scale corner handles, rotation ring near corners. Measurement lines already done. | `app/preview/mod.rs` | 1 session |
| **Breadcrumb in top bar** | Show `Intro → Diagram → Outro` breadcrumb when composition is active. Click to switch scene. | `app/shell/toolbar.rs`, `app/mod.rs` | ½ session |
| **Toast notification system** | Replace status-bar spam with transient toast notifications (auto-dismiss, stacked). Used for auto-keyframe confirmation, undo/redo, build status. | `app/components/`, `app/mod.rs` | 1 session |
| **Panel transition animations** | Smooth expand/collapse for inspector, diagnostics panel, sidebar tabs. | `app/panels/mod.rs`, `app/components/` | 1 session |
| **Drag values in property popup** | Left/right drag on numeric values to change (like Figma). | `app/preview/property_popup.rs` | ½ session |
| **Remove sidebar Scenes tab** | Scenes managed in timeline scene row; remove redundant tab. | `app/panels/mod.rs` | ½ session |

---

## Phase 4 — Keyframe & Action Infrastructure

Backend changes required before timeline keyframes and actions can be directly manipulated.

### 4a — Source Editing Primitives

| Item | What | Files | Effort |
|------|------|-------|--------|
| **Keyframe time-shift source edit** | `SourceEdit::MoveKeyframeTime { actor, property, old_time_s, new_time_s }` — finds keyframe block, updates time expression. | `source_edit/keyframe_edits.rs` | 1 session |
| **Per-property keyframe query API** | `TrackAccessor::has_keyframe_at(property, time_ms) -> bool` and `list_keyframes(property) -> Vec<u64>`. | `timeline/track.rs` | ½ session |
| **Action metadata in timeline** | Store action blocks (verb, target, duration, easing, start_time) in `AnimationTrack` or a new `ActionTrack` so the GUI can render them. | `timeline/build.rs`, `timeline/track.rs` | 2 sessions |

### 4b — GUI Features (unblocked by 4a)

| Item | What | Files | Effort |
|------|------|-------|--------|
| **Draggable keyframe diamonds** | Click-drag keyframe diamonds left/right in timeline. Snap to other KFs, ruler marks, 0.1s increments. Visual feedback: lifted diamond, vertical guide, tooltip (`2.0s → 3.2s`). | `app/panels/timeline_panel.rs` | 2 sessions |
| **Action blocks in timeline** | Colored horizontal bars on actor tracks (entrance=green, motion=blue, exit=red, effect=amber). Drag edges to resize duration, drag body to move start time. | `app/panels/timeline_panel.rs` | 2 sessions |
| **Per-property diamond toggles** | In property popup: filled ◆ = keyframe exists at playhead, hollow ○ = no keyframe. Click to toggle. Hover tooltip shows value + easing. | `app/preview/property_popup.rs` | 1 session |
| **Auto-keyframe on canvas drag** | When canvas drag ends, check if property changed; if so and no keyframe exists at current time, create one automatically + show undo toast. | `app/panels/preview_canvas/input.rs` | 1 session |
| **Multi-select keyframes** | `Shift+click` multiple diamonds, box-select by dragging empty area. Drag selection together. `Alt+drag` to duplicate. | `app/panels/timeline_panel.rs` | 1 session |

---

## Phase 5 — Multi-Viewport / PiP

Enable picture-in-picture and explicit viewport containers, as laid out in the design spec §7.

| Item | What | Files | Effort | Blocker |
|------|------|-------|--------|---------|
| **Explicit `Viewport` type** | New AST node + primitive for viewport rectangles with position, size, opacity, border, mask, and scene assignment. | `ast.rs`, `primitives/` | 1 week | — |
| **Viewport tracks in timeline** | Timeline shows viewport tracks with scene blocks (like current scene row but for viewports). | `timeline/build.rs`, `timeline/track.rs` | 2 weeks | Explicit Viewport |
| **Composite rendering** | Renderer composites multiple viewport scenes into a single frame. Each viewport renders its assigned scene at its rectangle. | `renderer/core.rs`, `renderer/offscreen.rs` | 2–3 weeks | Viewport tracks |
| **Viewport selection + gizmo** | Click viewport border → select, show move/resize gizmo. Double-click → enter scene editing inside. | `app/panels/preview_canvas/` | 1 week | Composite rendering |

---

## Phase 6 — Editor Infrastructure

Long-term foundational work to enable reliable source editing, formatting, and external tooling.

| Item | What | Files | Effort | Blocker |
|------|------|-------|--------|---------|
| **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | Green tree |
| **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |

---

## Phase 7 — Agent / NL Integration

Connect natural-language input to an actual backend service.

| Item | What | Files | Effort | Blocker |
|------|------|-------|--------|---------|
| **NL command bar dispatch** | Send NL input to an external AI service, parse structured response into `Command` queue. | `app/shell/nl_command_bar.rs`, `app/commands.rs` | 1 week | External AI service |
| **Agent suggestion UI** | Inline suggestion widget that proposes edits (e.g. "Add fade-in to Circle_1"). User accepts/rejects with keyboard shortcut. | `app/components/agent_suggestions.rs` | 3 days | NL dispatch |

---

## Phase 8 — Audio

| Item | What | Files | Effort | Blocker |
|------|------|-------|--------|---------|
| **Audio multi-segment muxing** | Concatenate multiple audio files via ffmpeg into final output. Support per-scene audio tracks. | `export/ffmpeg.rs` | 3 days | — |

---

## Implementation Order

1. **Phase 3.5** — Finish GUI polish (4–5 sessions, no blockers)
2. **Phase 4a** — Source editing primitives (2–3 sessions, no blockers)
3. **Phase 4b** — Draggable keyframes + action blocks (4–5 sessions, blocked by 4a)
4. **Phase 8** — Audio muxing (3 days, no blockers — can parallelize with 4b)
5. **Phase 5** — Multi-viewport (6–8 weeks, major feature)
6. **Phase 7** — Agent/NL integration (1–2 weeks, requires external service)
7. **Phase 6** — Green tree + trivia (4–6 months, foundational — start after syntax stabilizes)
