# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 7 — Audio

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 7.1 | **Audio multi-segment muxing** | Concatenate multiple audio files via ffmpeg into final output. Support per-scene audio tracks. | `export/ffmpeg.rs` | 3 days | — |

---

## Phase 8 — Filter System

> Post-processing primitive for blur, color correction, and compositing effects.
> See [`architecture.md`](architecture.md) §6 for full design, migration guide, and GPU shader plan.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 8.6 | **GPU shader filter pass** | Replace CPU blur + color matrix with WGSL compute shaders (blur H → blur V → color matrix) for 10–50× speedup on large scenes. Split into 8.6a (GPU filters + readback, 2–3 days) and 8.6b (zero-readback composite, +2–3 days). Full plan in [`architecture.md`](architecture.md) §6. | `renderer/shaders/`, `renderer/filter_backend.rs` | 1 week | When filter count becomes a bottleneck |
| 8.8 | **Deprecation diagnostics for removed properties** | Add a diagnostic when `shadow_blur`, `glow_radius`, `backdrop_blur`, `shadow_offset`, `shadow_color`, or `glow_color` are used, directing users to `Filter` containers. | `timeline/property_registry.rs`, `diagnostics.rs` | 0.5 days | — |

---

## Phase 9 — PiP / Multi-Viewport

> **Deferred.** The current viewport system has been removed. PiP will be implemented as an actor-level `Scene` primitive, not statement-level declarations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 9.1 | **Design `Scene` primitive** | Actor type whose content is another scene's timeline. Position, size, opacity are animatable properties (keyframes). `scene` property names the scene to render. | `primitives/`, `timeline/track.rs` | 3 days | Stable syntax |
| 9.2 | **Scene reference rendering** | Renderer evaluates referenced scene timeline at current time, clips to actor bounds, transforms to actor position, applies actor opacity. | `timeline/scene_eval.rs`, `renderer/` | 1 week | 9.1 |
| 9.3 | **Inspector + timeline support** | Scene actors show up in timeline tracks, inspector panel, and gizmo selection like any other actor. | `app/panels/` | 3 days | 9.2 |

---

## Phase 10b — Core Architecture Refactors

> Structural improvements discovered during implementation. These are large, risky changes
> that need dedicated testing time. Do not mix with feature work.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 10b.1 | **Fix integration test failures** | Triage and fix 37 failing tests in `parser_tests` and `module_tests`. `cargo test --lib` passes but integration tests fail — likely stale test expectations or changed parser behavior. | `tests/` | 1–2 days | — |
| 10b.2 | **Timeline should not own renderer resources** | `Timeline::evaluate()` clones the timeline to attach a `GpuFilterBackend`. The backend should be passed as a parameter instead. A compiled timeline should be a pure data structure with no knowledge of WGPU. | `timeline/mod.rs`, `renderer/offscreen.rs`, `timeline/scene_eval.rs` | 2 days | — |
| 10b.3 | **Atomic source-edit validation + drag batching** | All inspector edits (`handle_keyframe_edit`, `handle_property_edit`, `apply_child_order_edit`) must validate AST mutation and expression round-trip *before* touching the timeline. During drags, timeline mutates immediately but source is flushed once on drag end. Create `try_apply_source_edit` helper; move `PropertyValue → Expr` validation to `TryFrom`. | `actions/mod.rs`, `commands.rs`, `source_edit/` | 3 days | — |
| 10b.4 | **Crate split: `animatix-syntax`** | Extract parser, AST, `to_source`, diagnostics, easing, and module system into a new `animatix-syntax` crate. `animatix-analyzer` should depend only on syntax, not the full runtime engine. Eliminates WGPU/Vello from LSP compile graph. See [`architecture.md`](architecture.md) §17 for full plan. | New crate `animatix-syntax/` | 1 week | — |
| 10b.5 | **Vello external texture binding** | Vello's `Scene::draw_image` requires CPU-owned `peniko::ImageData`. For a zero-readback filter composite (8.6b), we need either upstream Vello changes to bind external `wgpu::TextureView`s, or a custom fullscreen render pass in `RendererCore`. | `renderer/core.rs`, `renderer/filter_backend.rs` | 3 days | 8.6a |
| 10b.6 | **Editor cell-type API** | `EditorBuffer` does not expose whether the focused cell is a keyframe or code cell. Needed for context-aware palette default modes (Actions vs Primitives). | `editor.rs`, `app/insertion.rs` | 1 day | — |

---

## Phase 11 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 11.1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 11.2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 11.1 |
| 11.3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |
| 11.4 | **Snippet AST parsing** | Parse snippet text into `Vec<Stmt>` and insert via `SourceEdit` instead of raw text surgery. Requires lossless parsing (green tree) to preserve formatting. | `app/insertion.rs`, `animatix-green/` | 2 days | 11.2 |

---

## Order

1. **Phase 7** (audio — no blockers)
2. **Phase 8** (filter system — 8.8 is small; 8.6 is self-contained)
3. **Phase 9** (PiP — after syntax and renderer are stable)
4. **Phase 10b** (architecture refactors — run in dedicated sprints, do not mix with feature work)
5. **Phase 11** (start after syntax stabilizes)

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 11 / green tree) | 11 |
| `let` variable animation | Superseded by easing functions in `always` blocks (6.8.3). Keyframed `let` tracks would need new timeline infrastructure; `always` lerp covers the same use cases statelessly. | Post-11 |
| **AI / NL Integration** | Requires external AI service (OpenAI, Claude, local LLM). No runtime dependency on AI should be mandatory. Includes: NL command bar, agent suggestion UI, agent_suggestions component. | Post-11 or separate product |
| **Row double-click / right-click** | No defined user story. Fields were wired to egui events but no caller consumed them. Re-add when a feature needs them. | When needed |
| **Badge button component** | Fully implemented but no caller. Re-add when the UI needs count badges (e.g. "Errors: 3"). | When needed |
| **Pre-compile plot closures** | Compile `func` AST bodies to closures/bytecode once per build instead of tree-walking thousands of times per curve. Would give 10–50× sampling speedup but requires a stable closure compilation API. | Post-11 or when plot count becomes a bottleneck again |
| **Amber flash on rewritten timestamps** | Visual polish: when `adjust_following_relative_keyframe` rewrites a relative offset, flash the timestamp label amber for ~300ms. Nice-to-have UX feedback. | When needed |
