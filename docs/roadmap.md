# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

**Principles**
- P0 language first — incomplete syntax blocks tutorials and adoption.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge.

---

## P0 — Critical Language Gaps

These block real-world usage and should be tackled before GUI polish.

| Item | What | Where |
|------|------|-------|
| **P0.1** | **Object field access** — `p.x` read / write for `Value::Object` | `timeline/env.rs`, parser, spec §15 |
| **P0.2** | **Transition blending** — Cross-scene dissolve/wipe instead of hard cuts | `composition.rs`, renderer, spec §17 |
| **P0.3** | **Activate generic value parser** — Replace `property_engine.rs` match blocks with registry-driven `value_parser.rs` | `timeline/value_parser.rs` (placeholder) |

---

## P1 — GUI Completion

The canvas works; the chrome around it doesn't. These are the highest-impact GUI items.

| Item | What | Where |
|------|------|-------|
| **P1.1** | **Scene list panel + composition timeline** — Show `# SceneName` blocks and `play` edges in the GUI | `app/panels/`, spec §17 |
| **P1.2** | **Tree-sitter grammar update** — Add `# SceneName` and `play` syntax highlighting | `tree-sitter-animatix/` |
| **P1.3** | **Wire up command system** — Reload, Undo/Redo, ScrollToLine, OpenTransitionEditor, RequestRepaint | `app/commands.rs`, `app/command_handlers.rs` |
| **P1.4** | **Diagnostics click-to-navigate** — Click a diagnostic to jump to line/col in editor | `app/panels/inspector/` |
| **P1.5** | **Transition editor UI** — Visual transition picker/timeline | `app/shell/` |
| **P1.6** | **Integrate agent suggestion UI** — Toast, inline suggestion, diff card components exist but aren't wired | `app/components/agent_suggestions.rs` |
| **P1.7** | **Hotkey wiring** — 1/2/3 hotkeys for scene slice quick-jump | `app/preview/scene_slices.rs` |
| **P1.8** | **NL command bar dispatch** — Parse natural language input and emit actual commands | `app/shell/nl_command_bar.rs` |

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
| **P3.3** | **Live preview for multi-scene** — `animatix render` currently shows only scene 1 | `renderer/`, `composition.rs` |

---

## Deferred / Large Architectural

Not justified at current scale. Revisit when the language surface is complete.

| Item | Effort | Blocker |
|------|--------|---------|
| **Green tree (rowan)** | 3–6 months | Need stable syntax first |
| **Trivia-inspired AST** | 2–3 months | Depends on green tree |
| **Web canvas / wasm** | Very high | Alternative renderer backend |
