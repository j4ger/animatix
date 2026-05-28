# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 5 — Component System & Architecture Cleanup

Fix structural debt discovered during Phases 2–4. These are prerequisites for reliable feature work.

| # | Item | What | Rationale | Files | Effort |
|---|------|------|-----------|-------|--------|
| 5.1 | **Redesign component body grammar** | Components should be pure actor templates, not scene containers. Remove `config` and keyframe (`#0s`) support from component bodies; only actor declarations, actions, assignments, and control flow belong inside `component { ... }`. Update `spec.md` §12 to match parser reality. | Parser currently rejects `config`/`#` inside components but spec shows them. Components intervening global config is a stale design from pre-multi-scene era. | `spec.md`, `parser/mod.rs`, `examples/templates/` | 2 days |
| 5.2 | **Extract command handlers from GuiShell** | Refactor `handle_command(&mut self, Command)` into standalone functions taking only the stores they need (e.g., `handle_scrub(document_store, preview_store, time)`). Eliminates `make_test_shell` dependency on temp dirs + filesystem for simple command tests. | `handle_command` mutates 5+ stores inline and requires a full `GuiShell` with workspace root, file tree, and persistence path just to test scrubbing. This is the last god-method remaining after WorkspaceViewer decomposition. | `command_handlers.rs`, `mod.rs` | 3 days |
| 5.3 | **Audit block processors for comment transparency** | Ensure `Stmt::Comment` is silently skipped (not errored) in all block contexts: `sequence`, `stagger`, `always`, `drive`, `for`, `conditional`, `ComponentDef` body. Add a shared `skip_comments()` helper if patterns repeat. | We fixed stagger/sequence in Phase 9, but the same bug pattern exists elsewhere. Comments should be invisible to all semantic processors. | `timeline/sequence.rs`, `timeline/build/*.rs`, `timeline/modifier_runtime/` | 1 day |
| 5.4 | **Unify diagnostic formatting** | Create a single `format_diagnostic()` path used by both parse errors (chumsky Rich) and build errors (timeline Diagnostic). Ensure consistent line/column display, context snippets, and error codes. | Parse errors say "expected statement, '}', found 'c'"; build errors say "[build] ERROR: Stagger blocks support only actions...". Two different systems, two different looks. | `parser/mod.rs`, `diagnostics.rs`, `main.rs` | 2 days |
| 5.5 | **Cache additional hot paths** | Cache `hit_regions` and `actor_bounds` on `DocumentStore`, invalidate on rebuild. Profile timeline panel and preview canvas to identify remaining per-frame allocations. | We cached actor labels and keyframes in 3.6, but `preview_canvas` and `inspector` likely rebuild collections every frame too. | `stores/document_store.rs`, `panels/preview_canvas/`, `panels/inspector/` | 2 days |
| 5.6 | **Add examples to parser roundtrip tests** | Include all `examples/*.amx` files in `to_source.rs` roundtrip tests so parser regressions on real-world syntax are caught immediately. | Roundtrip tests use synthetic fixtures. `slot_demo.amx` failed to parse entirely yet roundtrip tests were green. | `to_source.rs` | 4 hours |
| 5.7 | **Document supported image formats** | Update spec and error messages to explicitly list supported formats: PNG, JPEG, SVG, GIF. Remove all PPM references (stale demo feature, already dropped in Phase 9). | `primitives.amx` used PPM for months without anyone noticing it was broken. Explicit format docs prevent this. | `spec.md`, `primitives/image.rs` | 2 hours |

---

## Phase 6 — Multi-Viewport / PiP

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 6.1 | **Explicit `Viewport` type** | New AST node + primitive for viewport rectangles with position, size, opacity, border, mask, and scene assignment. | `ast.rs`, `primitives/` | 1 week | — |
| 6.2 | **Viewport tracks in timeline** | Timeline shows viewport tracks with scene blocks (like current scene row but for viewports). | `timeline/build.rs`, `timeline/track.rs` | 2 weeks | 6.1 |
| 6.3 | **Composite rendering** | Renderer composites multiple viewport scenes into a single frame. Each viewport renders its assigned scene at its rectangle. | `renderer/core.rs`, `renderer/offscreen.rs` | 2–3 weeks | 6.2 |
| 6.4 | **Viewport selection + gizmo** | Click viewport border → select, show move/resize gizmo. Double-click → enter scene editing inside. | `app/panels/preview_canvas/` | 1 week | 6.3 |

---

## Phase 7 — Audio

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 7.1 | **Audio multi-segment muxing** | Concatenate multiple audio files via ffmpeg into final output. Support per-scene audio tracks. | `export/ffmpeg.rs` | 3 days | — |

---

## Phase 8 — Agent / NL Integration

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 8.1 | **NL command bar dispatch** | Send NL input to an external AI service, parse structured response into `Command` queue. | `app/shell/nl_command_bar.rs`, `app/commands.rs` | 1 week | External AI service |
| 8.2 | **Agent suggestion UI** | Inline suggestion widget that proposes edits (e.g. "Add fade-in to Circle_1"). User accepts/rejects with keyboard shortcut. | `app/components/agent_suggestions.rs` | 3 days | 8.1 |

---

## Phase 9 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 9.1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 9.2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 9.1 |
| 9.3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |

---

## Order

1. **Phase 5** (architectural foundation — do before any feature work)
2. **Phase 7** (audio — no blockers, can parallelize with 5)
3. **Phase 6** (after Phase 5)
4. **Phase 8** (external AI service required)
5. **Phase 9** (start after syntax stabilizes)
