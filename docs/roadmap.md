# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).  
> For detailed implementation plans, see [`typing-plan.md`](typing-plan.md).

---

## Bug Fixes

No open bugs at this time.

## Example Cleanup

| Item | What |
|------|------|
| **06-components.amx** — Use a `Row` container for cards instead of manual `anchor`/`offset`. |

## Colors & polish

| Item | What |
|------|------|
| **P7** | **Add missing accent colors** — `accent.secondary` and `accent.info` do not exist in any built-in colorscheme. Several old and new examples naturally need 5–6 distinct accent colors. |
| **P8** | **`check --render-smoke`** — optionally export 1 frame to catch renderer bugs (Graph mapping, multi-scene crashes) that `check` currently misses. |

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
