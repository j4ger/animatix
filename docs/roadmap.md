# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 6 — Architecture Cleanup

Fix structural debt discovered during Phases 2–5. These are prerequisites for reliable feature work.

| # | Item | What | Rationale | Files | Effort |
|---|------|------|-----------|-------|--------|
| 6.1 | **Remove statement-level viewport system** | The `viewport Name at ... scene "X"` statement syntax and `timeline::Viewport` are half-baked — parsed and stored but never rendered. They create confusion with any future PiP primitive. Strip `Stmt::ViewportDecl`, `timeline::Viewport`, and all downstream references. | Dead code. The correct PiP design will be an actor-level `Scene` primitive, not a statement. | `ast.rs`, `parser/mod.rs`, `timeline/mod.rs`, `timeline/build/process.rs`, `composition.rs` | 1 day |
| 6.2 | **Wrap Timeline heavy fields in `Arc`** | `Timeline::clone()` manually deep-copies `font_context` and `asset_cache` on every clone. In compositions with viewport/scene references this is expensive. Wrap both in `Arc<...>` so clones are cheap reference bumps. | `font_context` is heavy (font database); `asset_cache` holds loaded images/SVGs. Copying them per-scene during composition build is wasteful. | `timeline/mod.rs` | 4 hours |
| 6.3 | **Audit remaining per-frame allocations** | After 5.5's hit_regions fix, profile the inspector panel and preview canvas for remaining per-frame Vec/String allocations. Cache where patterns repeat. | Inspector `build_property_groups()` and timeline label truncation both allocate collections every frame. | `panels/inspector/`, `panels/timeline_panel.rs` | 1 day |

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

## Phase 9 — Agent / NL Integration

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 9.1 | **NL command bar dispatch** | Send NL input to an external AI service, parse structured response into `Command` queue. | `app/shell/nl_command_bar.rs`, `app/commands.rs` | 1 week | External AI service |
| 9.2 | **Agent suggestion UI** | Inline suggestion widget that proposes edits (e.g. "Add fade-in to Circle_1"). User accepts/rejects with keyboard shortcut. | `app/components/agent_suggestions.rs` | 3 days | 9.1 |

---

## Phase 10 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 10.1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 10.2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 10.1 |
| 10.3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |

---

## Order

1. **Phase 6** (architecture cleanup — do before any feature work)
2. **Phase 7** (audio — no blockers, can parallelize with 6)
3. **Phase 8** (PiP — after syntax and renderer are stable)
4. **Phase 9** (external AI service required)
5. **Phase 10** (start after syntax stabilizes)
