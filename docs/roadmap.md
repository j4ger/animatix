# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).  
> For detailed implementation plans, see [`typing-plan.md`](typing-plan.md).

**Principles**
- P0 language first — incomplete syntax blocks tutorials and adoption.
- The canvas is the hero. Any operation doable on canvas should not require panel input.
- Time is a HUD, not a permanent panel. Scrubbing is frequent but brief.
- Visual edits must write back to source. Otherwise text and visuals diverge.

---

## P2 — Language Features

Nice-to-have syntax expansions. Medium user impact, well-scoped.

| Item | What | Where | Depends |
|------|------|-------|---------|
| **P2.1** | **Action parameters** — `pulse btn [200ms, scale: 1.2]` instead of fixed bodies | parser, spec §12 | — |
| **P2.2** | **Multi-target action invocation** — `pulse btn, icon` | parser, timeline build | — |
| **P2.3** | **Module-scoped actions** — `action Foo() { ... }` at file level, not just inside components | parser, spec §12 | P2.1 |
| **P2.4** | **SVG import enhancements** — `viewBox`, `<defs>`, gradients, `polyline`/`polygon`, `stroke-dasharray` | `timeline/svg_import.rs` | — |
| **P2.5** | **Plot tick labels** — `tick_labels: true` on `PlotAxes` | `renderer/plot.rs`, `primitives.md` | — |

**Dependency chain:** P2.1 → P2.3.  
P2.2, P2.4, P2.5 are independent and can ship anytime.

---

## P3 — Polish & Runtime

Small runtime improvements and export quality.

| Item | What | Where |
|------|------|-------|
| **P3.2** | **Morph fade strategy** — Implement `MorphStrategy::Fade` (currently a placeholder) | `timeline/morph.rs` |

---

## Deferred

### AI Features

Tracked separately. Requires external AI service integration.

| Item | What | Where |
|------|------|-------|
| **P1.7** | **NL command bar dispatch** | `app/shell/nl_command_bar.rs` |
| **P1.9** | **Integrate agent suggestion UI** | `app/components/agent_suggestions.rs` |

### Audio / Export

| Item | What | Where |
|------|------|-------|
| **P3.1** | **Audio multi-segment muxing** — Concatenate multiple audio files via ffmpeg | `renderer/encode/mod.rs` |

### Large Architectural

Not justified at current scale. Revisit when the language surface is complete.

| Item | Effort | Blocker |
|------|--------|---------|
| **Green tree (rowan)** | 3–6 months | Need stable syntax first |
| **Trivia-inspired AST** | 2–3 months | Depends on green tree |
| **Web canvas / wasm** | Very high | Alternative renderer backend |
