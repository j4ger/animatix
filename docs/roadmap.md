# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 6 — Multi-Viewport / PiP

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 6.1 | **Explicit `Viewport` type** | New AST node + primitive for viewport rectangles with position, size, opacity, border, mask, and scene assignment. | `ast.rs`, `primitives/` | 1 week | — |
| 6.2 | **Viewport tracks in timeline** | Timeline shows viewport tracks with scene blocks (like current scene row but for viewports). | `timeline/build.rs`, `timeline/track.rs` | 2 weeks | 6.1 |
| 6.3 | **Composite rendering** | Renderer composites multiple viewport scenes into a single frame. Each viewport renders its assigned scene at its rectangle. | `renderer/core.rs`, `renderer/offscreen.rs` | 2–3 weeks | 6.2 |
| 6.4 | **Viewport selection + gizmo** | Click viewport border → select, show move/resize gizmo. Double-click → enter scene editing inside. | `app/panels/preview_canvas/` | 1 week | 6.3 |

---

## Phase 7 — Audio

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 7.1 | **Audio multi-segment muxing** | Concatenate multiple audio files via ffmpeg into final output. Support per-scene audio tracks. | `export/ffmpeg.rs` | 3 days | — |

---

## Phase 8 — Agent / NL Integration

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 8.1 | **NL command bar dispatch** | Send NL input to an external AI service, parse structured response into `Command` queue. | `app/shell/nl_command_bar.rs`, `app/commands.rs` | 1 week | External AI service |
| 8.2 | **Agent suggestion UI** | Inline suggestion widget that proposes edits (e.g. "Add fade-in to Circle_1"). User accepts/rejects with keyboard shortcut. | `app/components/agent_suggestions.rs` | 3 days | 8.1 |

---

## Phase 9 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 9.1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 9.2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 9.1 |
| 9.3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |

---

## Order

1. **Phase 7** (audio — no blockers, can parallelize with other work)
2. **Phase 6** (multi-viewport)
3. **Phase 8** (external AI service required)
4. **Phase 9** (start after syntax stabilizes)

(End of file - total 69 lines)
