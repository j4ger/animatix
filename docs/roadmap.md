# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).  
> For detailed implementation plans, see [`typing-plan.md`](typing-plan.md).

---

## Bug Fixes

| Item | What | Where | Severity |
|------|------|-------|----------|
| **P3** | **Component slot-fill parser fails after 2 instances** — exactly 2 top-level `@slotname { ... }` fills parse; a 3rd produces `expected '.', ':', found 'c'`. | `module/expand.rs` or parser | High |
| **P4** | **Custom component actions can't resolve `self.*` nested paths** — `self.frame.color` expands to `instance.frame.color` but build reports "does not resolve to a declared actor". | `timeline/build/` | Medium |
| **P5** | **Comments in `always` blocks produce IR warnings** — `//` inside `always { }` yields "Unsupported IR statement: comment". | `timeline/modifier_runtime/ir/` | Low |
| **P6** | **`scene_width` / `scene_height` silently fail in `let`** — built-ins only work inside `always`, not top-level `let`. No diagnostic. | `timeline/env.rs` | Low |

## Example Cleanup

Blocked by bug fixes above. Once resolved, update examples to remove workarounds.

| Blocked by | What |
|------------|------|
| P3, P4 | **06-components.amx** — restore 3rd card instance (currently limited to 2 due to slot-fill parser bug). Add custom `action highlight` back once `self.*` resolution works. Use a `Row` container for cards instead of manual `anchor`/`offset`. |
| P5 | **05-reactive.amx** — re-add explanatory comments inside `always` blocks. Use `let orbit_radius_x = scene_width / 4` instead of hardcoded values. |
| P6 | **05-reactive.amx** — use `scene_width` / `scene_height` in top-level `let` declarations. |

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
