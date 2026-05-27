# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Bug Fixes

No open bugs at this time.

---

## Phase 3 — GUI Redesign: "Timeline-First Direct Manipulation"

**Status:** In progress. Full design specification: [`docs/design/gui-redesign-2026.md`](design/gui-redesign-2026.md)

| Phase | Item | Status | What | Files |
|-------|------|--------|------|-------|
| **P12** | **Kill the Bars** | ✅ Done | Delete transport bar, preview header, NL command bar. Simplify top bar to filename + play + settings + command palette. Move playback into timeline. | `app/shell/`, `app/panels/preview_canvas/mod.rs`, `app/mod.rs` |
| **P13** | **Unified Gizmo & Property Popup** | ✅ Partial | Measurement lines added. Property popup replaces floating card (4 essentials + tabs, diamond placeholders). Tool switching (V/M/G/S/R/E). Unified gizmo visual deferred. | `app/preview/`, `app/panels/preview_canvas/`, `app/components/` |
| **P14** | **Draggable Timeline & Actions** | ✅ Partial | Action palette implemented (A key). Draggable keyframes + action blocks deferred — requires source-editing infrastructure for keyframe time shifting. | `app/panels/timeline_panel.rs`, `app/commands.rs` |
| **P15** | **Multi-Scene Integration** | ✅ Partial | Context HUD on canvas (scene name, time). Scene blocks already in timeline. Breadcrumb + explicit global/local toggle deferred. | `app/panels/timeline_panel.rs`, `app/panels/mod.rs`, `app/mod.rs`, `app/preview/` |
| **P16** | **Polish** | ✅ Done | Preview hover HUD (overlays, zoom). Time lens trigger Space → T. Shortcut cheat sheet (`?`). Toast notifications deferred. | `app/design_tokens.rs`, `app/components/`, `app/preview/` |

---

## Deferred Work

| Item | Why Deferred | Unblocker |
|------|--------------|-----------|
| **Draggable keyframe diamonds** | Requires source-editing infrastructure to shift keyframe times in AST | Source edit support for keyframe time updates |
| **Action blocks in timeline** | Requires extracting action timing from AST (actions are expanded during timeline build, not stored at runtime) | Action metadata persistence in timeline |
| **Unified gizmo visual** | Current 8-handle gizmo works; full unified gizmo (move arrow + scale corner + rotation ring) is a large refactor | Time availability |
| **Per-property diamond keyframe toggles** | Requires tracking keyframe existence per property at current time | Inspector keyframe query API |
| **Toast notifications** | Status bar is sufficient for now | Design system for transient notifications |
| **Breadcrumb in top bar** | Scene switching works via timeline scrubber; explicit breadcrumb is polish | Phase 4 |

---

## Phase 4 — Future Features (Post-GUI Redesign)

| Phase | Item | What | Effort | Blocker |
|-------|------|------|--------|---------|
| **P17** | **Audio multi-segment muxing** | Concatenate multiple audio files via ffmpeg | 3 days | — |
| **P18** | **NL command bar dispatch** | Connect NL input to actual AI service backend | 1 week | External AI service |
| **P19** | **Agent suggestion UI** | Integrate agent suggestion widget | 3 days | External AI service |
| **P20** | **Multi-viewport / PiP** | Explicit `Viewport` type, viewport tracks in timeline, composite rendering | 2–3 months | GUI redesign |
| **P21** | **Green tree (rowan)** | Immutable syntax tree with cheap clones | 3–6 months | Stable syntax |
| **P22** | **Trivia-inspired AST** | Whitespace/comment preservation in AST | 2–3 months | Green tree |
| **P23** | **Web canvas / WASM** | Alternative renderer backend for browser export | Very high | Alternative renderer |

---

## Completion Criteria

**Phase 3 (GUI Redesign) is complete when:**
- [x] Only 28px top bar persists
- [x] Canvas has measurement lines
- [x] Property popup replaces floating card
- [ ] Timeline keyframes are draggable
- [x] Action palette works
- [x] Multi-scene context HUD works
