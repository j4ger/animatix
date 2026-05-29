# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 6.10 — CLI Error Messages

Current error output is insufficient for locating problems. Parse errors show only the message string; build diagnostics use a verbose ` • ` separator and rarely show file paths or line numbers. Goal: rustc-quality error reporting.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 6.10.1 | **Parse error location** | `ModuleError::ParseErrors` currently prints only `errors[0].message`. Include `line:column`, error context stack ("in actor declaration"), and source file path in the Display output. | `module.rs`, `parser/mod.rs` | 2–3 hrs | — |
| 6.10.2 | **Build diagnostic formatting** | `format_diagnostic` uses a ` • ` separator between location, severity, phase, code, message, subject, and path. Restructure to rustc-style: `file.rs:line:col [severity:code] message` on the first line, followed by `subject:` and `path:` on indented continuation lines if present. | `diagnostics.rs` | 2–3 hrs | — |
| 6.10.3 | **File path propagation** | Build diagnostics (`Diagnostic.location.path`) are rarely set because `BuildTarget::from_ast` receives no file path. Thread the source file path through the build pipeline so every diagnostic knows where it originated. | `timeline/build/`, `composition.rs` | 2–3 hrs | — |
| 6.10.4 | **Source snippets** | For diagnostics that have a byte span (`location.span`), extract the relevant source line and print it with a `^` underline pointing to the exact token. This requires keeping the original source text alongside the AST during build. | `diagnostics.rs`, `timeline/build/` | 1–2 days | — |

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

## Phase 9 — GUI Polish

Small interaction refinements discovered during dead-code cleanup. Each is low-effort and improves perceived quality.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 9.1 | **Precise diagnostic navigation** | `DiagnosticTarget` already captures `column` from parse errors but the GUI only jumps to the line. Wire the column into the source editor cursor position so clicking a diagnostic lands on the exact token. | `app/components/mod.rs`, `editor.rs` | 2–3 hrs | — |
| 9.2 | **Context menu hover highlighting** | `MenuItemResponse` already computes `rect` for each rendered item but never uses it. Add a subtle hover background to menu items. | `app/components/context_menu.rs` | 2–3 hrs | — |

---

## Phase 10 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 10.1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 10.2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 10.1 |
| 10.3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |

---

## Order

1. **Phase 6.10** (CLI error messages — small, improves daily DX immediately)
2. **Phase 7** (audio — no blockers)
3. **Phase 8** (PiP — after syntax and renderer are stable)
4. **Phase 9** (GUI polish — small, can be done anytime)
5. **Phase 10** (start after syntax stabilizes)

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 10 / green tree) | 10 |
| `let` variable animation | Superseded by easing functions in `always` blocks (6.8.3). Keyframed `let` tracks would need new timeline infrastructure; `always` lerp covers the same use cases statelessly. | Post-10 |
| **AI / NL Integration** | Requires external AI service (OpenAI, Claude, local LLM). No runtime dependency on AI should be mandatory. Includes: NL command bar, agent suggestion UI, agent_suggestions component. | Post-10 or separate product |
| **Row double-click / right-click** | No defined user story. Fields were wired to egui events but no caller consumed them. Re-add when a feature needs them. | When needed |
| **Badge button component** | Fully implemented but no caller. Re-add when the UI needs count badges (e.g. "Errors: 3"). | When needed |
