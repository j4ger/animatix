# Animatix Roadmap

> What's left to build. For the language spec, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## Phase 5 — Source Editor Foundation

### 5.1 Lossless AST (green tree)

Immutable syntax tree that preserves whitespace and comments. Enables reliable source editing without formatting loss.

**Current state:** AST has `Span`/`ByteSpan` for positions, `Property.trailing_comment`, `Stmt::Comment`. Parser (Chumsky) discards whitespace/comments. `to_source` normalizes formatting on re-serialize.

**Verdict:** Defer. 6–8 weeks of work. Current source_edit + formatter handles 90% of cases. Only matters for perfect round-trip fidelity (rare edge case). Revisit if users report formatting loss.

| # | Task | Description | Effort | Blocker |
|---|------|-------------|--------|---------|
| 5.1.1 | **Trivia data structure** | Define `Trivia` type (leading whitespace, trailing whitespace, comments). Add to AST nodes. | 1 week | — |
| 5.1.2 | **Parser trivia capture** | Modify Chumsky parser to capture whitespace/comments as trivia instead of discarding. | 2–3 weeks | 5.1.1 |
| 5.1.3 | **to_source trivia emission** | Update `ToSource` impl to emit trivia when present, fall back to normalized formatting when absent. | 1 week | 5.1.2 |
| 5.1.4 | **Source edit trivia preservation** | Update `source_edit` module to preserve trivia during AST mutations (property edits, keyframe inserts, etc.). | 1–2 weeks | 5.1.3 |
| 5.1.5 | **Round-trip tests** | Parse → serialize → parse → serialize produces identical output for all example files. | 1 week | 5.1.4 |

---

## Phase 6 — Web Export & Media

> Expanding output targets and preview fidelity.

| # | Feature | Description | Effort | Blocker |
|---|---------|-------------|--------|---------|
| 1 | **Web canvas / WASM export** | Render to HTML5 Canvas or WebGPU for browser-based playback. Standalone `animatix-web` crate. | 1–2 months | Renderer abstraction |
| 2 | **Audio playback in preview** | Play audio segments during GUI preview (currently only muxed on video export). | 1 week | Audio backend (rodio/cpal) |
| 3 | **APNG export** | Animated PNG output for lossless web animations. Requires an APNG encoder backend. | 3 days | APNG encoder |

**Verdict:** Defer all. WASM export needs renderer abstraction (big refactor). Audio needs new dependency. APNG needs encoder. None are quick wins.

---

## Code Quality & Performance (from audit)

> All items from the June 2026 audit have been completed or assessed. See git history for details.

### Remaining: Shape primitive incremental migration

The `evaluate_shape_render()` helper is available in `primitives/mod.rs`. Line, polygon, path, and arrow primitives can be migrated incrementally — no urgency, the helper is ready when touching those files.

---

## Icebox

> Blocked on external dependencies, solve niche problems, or lack user demand. No committed timeline.

| Feature | Blocker / Reason |
|---------|------------------|
| **Zero-readback GPU filters** | Blocked on Vello GPU filter support ([#1296](https://github.com/linebender/vello/issues/1296)). Phase 8.6a (GPU compute + readback) ships the perf win; wait for Vello to eliminate the readback. |
| **Scene primitive (PiP)** | Existing components + Stack cover reuse. Needs composition-level design for parallel playback. Revisit after transition blending. |
| **Export performance: pre-compiled plot closures** | Only matters for dozens of plot actors. `always` block easing covers simpler cases. |
| **Asset usage tracking** | Show which actors reference an asset. Low user demand. |
| **Variable track UI** | GUI for `let` variable tracks. `always` blocks cover most cases. |
| **Module dependency graph** | Visual graph of `.amx` imports. Internal tooling, no user stories. |

---

## Next Steps (prioritized)

1. **Phase 5.1 — Lossless AST** (6–8 weeks)
   - Defer until users report formatting loss
   - Current system handles 90% of cases

2. **Phase 6 — Web Export** (2+ months)
   - Defer until renderer abstraction is done
   - Big refactor, no quick wins
