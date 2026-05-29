# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 6.5 — Documentation Hotfixes

Quick wins from user report analysis. Target: AI/codegen compile rate 86.5% → 95%+.

| # | Item | What | Files | Effort |
|---|------|------|-------|--------|
| 6.5.1 | **Property registry export** | Generate a canonical property reference table from `PROPERTY_REGISTRY` (name, type, actor kinds, animated?). Include in `spec.md` or a new `properties.md`. | `property_registry.rs`, `docs/` | 2–3 hrs |
| 6.5.2 | **Element whitelist + anti-list** | Explicit list of all existing primitives + explicit "does not exist" list (`Circle`, `Arrow`, `Graph3D`, etc.) to reduce LLM hallucination. | `docs/spec.md` | 1 hr |
| 6.5.3 | **Color system consolidation** | Document: hex unsupported, RGBA is 0–1 float, complete `accent.*` / `text.*` / `stroke.*` / `surface.*` token list. | `docs/spec.md` | 3–4 hrs |
| 6.5.4 | **Typst vs LaTeX cheat sheet** | Quick reference table in `spec.md` for Math primitive: `frac(a,b)` not `\frac`, `lim_(x -> 1)` not `\lim`, etc. | `docs/spec.md` | 2–3 hrs |
| 6.5.5 | **3D support status** | Add a clear "No 3D support" callout to `spec.md` status matrix. | `docs/spec.md` | 30 min |
| 6.5.6 | **Spec/examples consistency audit** | Verify all snippets in `spec.md` compile against current parser. Fix any remaining `[]` vs `{}` mismatches. | `docs/spec.md`, `examples/` | 2 hrs |

---

## Phase 6.6 — Parser Robustness

| # | Item | What | Rationale | Files | Effort |
|---|------|------|-----------|-------|--------|
| 6.6.1 | **Comments inside brackets/lists** | `//` line comments inside `{}`, `[]`, `()` delimiters are currently rejected. Replace `.padded()` with a custom `whitespace_or_comment` skipper throughout the parser. | LLM and humans naturally comment inside blocks. Current error `expected '}', found '/'` is confusing. | `parser/mod.rs` | 1–2 days |
| 6.6.2 | **Parse error context** | Include enclosing grammar rule (e.g., "in property list of actor declaration") in parse errors. | Helps LLM/scripting identify which construct failed. | `parser/mod.rs` | 4–6 hrs |

---

## Phase 6.7 — CLI Tooling

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 6.7.1 | **`check --format json`** | Structured JSON output: `{"passed": bool, "errors": [{"line", "col", "message", "code"}]}` for IDE/scripting integration. | `src/main.rs` | 4–6 hrs | — |
| 6.7.2 | **stdin support** | `animatix-cli check -` or pipe support for scripting workflows. | `src/main.rs` | 2–3 hrs | — |
| 6.7.3 | **Clean ANSI output** | Add `--no-color` flag; ensure plain text mode strips ANSI escapes from `tracing` and `format_diagnostic`. | `src/main.rs`, `diagnostics.rs` | 2–3 hrs | — |

---

## Phase 6.8 — Language Features & Examples

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 6.8.1 | **Graph nested element animation** | Support `g.vec.to = (5, 2)` dotted path syntax for assignments targeting children inside containers. Currently only `actor.property` works. | `parser/mod.rs`, `timeline/build/` | 3–5 days | — |
| 6.8.2 | **Arrow primitive** | Dedicated `Arrow` actor with `from`, `to`, `head_size` properties (vs. using `Line` with manual arrowheads). | `primitives/`, `timeline/track.rs` | 1–2 days | — |
| 6.8.3 | **Easing functions in `always` blocks** | Expose named easing functions (`ease_in`, `ease_out`, `bounce`, `elastic`, etc.) as `num1` builtins so users can compose eased interpolation inside `always` blocks. Enables `let x = lerp(0, 100, ease_out(t / 2.0))`. | `builtins.rs` | 2–3 hrs | — |
| 6.8.4 | **Underused element examples** | Add dedicated examples for `ContourSet` (currently 0), `Path` (1), `VectorField` (1), `Heatmap` (1). | `examples/` | 2–3 days | — |

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

1. **Phase 6** (architecture cleanup — completed)
2. **Phase 6.5–6.8** (DX & documentation — user report findings; do before feature work)
3. **Phase 7** (audio — no blockers, can parallelize with 6.x)
4. **Phase 8** (PiP — after syntax and renderer are stable)
5. **Phase 9** (external AI service required)
6. **Phase 10** (start after syntax stabilizes)

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 10 / green tree) | 10 |
| `let` variable animation | Superseded by easing functions in `always` blocks (6.8.3). Keyframed `let` tracks would need new timeline infrastructure; `always` lerp covers the same use cases statelessly. | Post-10 |
