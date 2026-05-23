# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 language first — incomplete syntax blocks tutorials and adoption.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge.

---

## ✅ P0 — Critical Language Gaps (Complete)

| Item | Status |
|------|--------|
| **P0.1** | Object field access (`p.x`) implemented |
| **P0.2** | Transition blending (fade/wipe) implemented for export and live preview |
| **P0.3** | Generic value parser activated; property_engine delegates to registry-driven parsing |

---

## P1 — GUI Completion

The canvas works; the chrome around it doesn't. Ordered by feasibility + impact.

### P1a — Quick Wins (small, high user impact)

| Item | What | Where |
|------|------|-------|
| **P1.1** | **Wire up command system** — Connect Reload, Undo/Redo, ScrollToLine, OpenTransitionEditor, RequestRepaint to actual handlers | `app/commands.rs`, `app/command_handlers.rs` |
| **P1.2** | **Hotkey wiring** — Keyboard shortcuts for Undo/Redo, Reload, scene jumps (1/2/3) | `app/mod.rs`, `app/preview/` |
| **P1.3** | **Diagnostics click-to-navigate** — Click a diagnostic message to jump to line/col in editor | `app/panels/inspector/` |

### P1b — Medium Features

| Item | What | Where |
|------|------|-------|
| **P1.4** | **Tree-sitter grammar update** — Add `# SceneName` and `play` syntax highlighting | `tree-sitter-animatix/` |
| **P1.5** | **Scene list panel** — Show `# SceneName` blocks in a sidebar list | `app/panels/` |
| **P1.6** | **Transition editor UI** — Visual transition picker/timeline | `app/shell/` |
| **P1.7** | **NL command bar dispatch** — Parse natural language input and emit actual commands | `app/shell/nl_command_bar.rs` |

### P1c — Large Features

| Item | What | Where |
|------|------|-------|
| **P1.8** | **Composition timeline** — Show `play` edges and scene durations in the timeline panel | `app/panels/timeline_panel.rs` |
| **P1.9** | **Integrate agent suggestion UI** — Wire toast, inline suggestion, diff card components | `app/components/agent_suggestions.rs` |

---

## P2 — Language Features

Nice-to-have syntax expansions. Medium user impact, well-scoped.

| Item | What | Where |
|------|------|-------|
| **P2.1** | **Action parameters** — `pulse btn [200ms, scale: 1.2]` instead of fixed bodies | parser, spec §12 |
| **P2.2** | **Multi-target action invocation** — `pulse btn, icon` | parser, timeline build |
| **P2.3** | **Module-scoped actions** — `action Foo() { ... }` at file level, not just inside components | parser, spec §12 |
| **P2.4** | **SVG import enhancements** — `viewBox`, `<defs>`, gradients, `polyline`/`polygon`, `stroke-dasharray` | `timeline/svg_import.rs` |
| **P2.5** | **Plot tick labels** — `tick_labels: true` on `PlotAxes` | `renderer/plot.rs`, `primitives.md` |

---

## P3 — Polish & Runtime

Small runtime improvements and export quality.

| Item | What | Where |
|------|------|-------|
| **P3.1** | **Audio multi-segment muxing** — Concatenate multiple audio files via ffmpeg | `renderer/encode/mod.rs` |
| **P3.2** | **Morph fade strategy** — Implement `MorphStrategy::Fade` (currently a placeholder) | `timeline/morph.rs` |

---

## Deferred / Large Architectural

Not justified at current scale. Revisit when the language surface is complete.

| Item | Effort | Blocker |
|------|--------|---------|
| **Green tree (rowan)** | 3–6 months | Need stable syntax first |
| **Trivia-inspired AST** | 2–3 months | Depends on green tree |
| **Web canvas / wasm** | Very high | Alternative renderer backend |
