# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).  
> For detailed implementation plans, see [`typing-plan.md`](typing-plan.md).

---

## Bug Fixes

No open bugs at this time.



---

## AI Integration

Tracked separately. Requires external AI service integration.

| Item | What | Where |
|------|------|-------|
| **P9** | **NL command bar dispatch** | `app/shell/nl_command_bar.rs` |
| **P10** | **Integrate agent suggestion UI** | `app/components/agent_suggestions.rs` |

## Audio / Export

| Item | What | Where |
|------|------|-------|
| **P11** | **Audio multi-segment muxing** — Concatenate multiple audio files via ffmpeg | `renderer/encode/mod.rs` |

## Large Architectural

Not justified at current scale. Revisit when the language surface is complete.

| Item | Effort | Blocker |
|------|--------|---------|
| **P12** | **Green tree (rowan)** — Immutable syntax tree with cheap clones | 3–6 months | Need stable syntax first |
| **P13** | **Trivia-inspired AST** — Whitespace/comment preservation in AST | 2–3 months | Depends on green tree |
| **P14** | **Web canvas / WASM** — Alternative renderer backend for browser export | Very high | Alternative renderer backend |
