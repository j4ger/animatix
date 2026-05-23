# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).  
> For detailed implementation plans, see [`typing-plan.md`](typing-plan.md).

---

## P3 — Polish & Runtime

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
