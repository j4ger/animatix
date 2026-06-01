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
> Full design: [`design/filter-system.md`](design/filter-system.md)

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 8.1 | **Language surface** | Add `Filter` to `ActorKindId`, `PROPERTY_REGISTRY`, primitive registry, and docs. Properties: `blur`, `brightness`, `contrast`, `saturate`, `hue_rotate`, `sepia`. | `timeline/`, `primitives/`, `docs/` | 1 day | — |
| 8.2 | **Offscreen infrastructure** | `FilterTargetPool` for acquiring/releasing WGPU textures. Integrate into `PreviewSurface` and resize path. | `preview_surface.rs`, `renderer/` | 2 days | — |
| 8.3 | **Filter shaders** | Separable Gaussian blur (H/V passes) + color matrix shader (brightness/contrast/saturate/hue/sepia). Compile at init, manage pipeline state. | `renderer/shaders/`, `preview_surface.rs` | 2 days | 8.2 |
| 8.4 | **Scene evaluation integration** | In `scene_eval.rs`, detect `Filter` actors, render children offscreen, run filter chain, draw result as image. Handle empty/identity/no-op cases. | `timeline/scene_eval.rs` | 2 days | 8.3 |
| 8.5 | **CLI export support** | Wire `FilterTargetPool` into video/GIF/image export renderers. Share pool or create per-export instance. | `renderer/encode/` | 1 day | 8.4 |
| 8.6 | **Deprecate property-based effects** | Emit diagnostics for `shadow_blur`, `glow_radius`, `backdrop_blur`, etc. Hide from inspector. Document migration path. | `timeline/property_registry.rs`, `app/panels/inspector.rs`, `docs/` | 1 day | 8.1 |

---

## Phase 9 — PiP / Multi-Viewport

> **Deferred.** The current viewport system has been removed. PiP will be implemented as an actor-level `Scene` primitive, not statement-level declarations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 8.1 | **Design `Scene` primitive** | Actor type whose content is another scene's timeline. Position, size, opacity are animatable properties (keyframes). `scene` property names the scene to render. | `primitives/`, `timeline/track.rs` | 3 days | Stable syntax |
| 8.2 | **Scene reference rendering** | Renderer evaluates referenced scene timeline at current time, clips to actor bounds, transforms to actor position, applies actor opacity. | `timeline/scene_eval.rs`, `renderer/` | 1 week | 8.1 |
| 8.3 | **Inspector + timeline support** | Scene actors show up in timeline tracks, inspector panel, and gizmo selection like any other actor. | `app/panels/` | 3 days | 8.2 |

---

## Phase 10b — Core Architecture Refactors (deferred from cleanup)

> Structural improvements discovered during Phase 10. These are large, risky changes
> that need dedicated testing time. Do not mix with feature work.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 10b.3 | **Atomic source-edit validation + drag batching** | All inspector edits (`handle_keyframe_edit`, `handle_property_edit`, `apply_child_order_edit`) must validate AST mutation and expression round-trip *before* touching the timeline. During drags, timeline mutates immediately but source is flushed once on drag end. Create `try_apply_source_edit` helper; move `PropertyValue → Expr` validation to `TryFrom`. | `actions/mod.rs`, `commands.rs`, `source_edit/` | 3 days | — |

---

## Phase 11 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 10.1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 10.2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 10.1 |
| 10.3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |

---

## Order

1. **Phase 7** (audio — no blockers)
2. **Phase 8** (filter system — no blockers, self-contained)
3. **Phase 9** (PiP — after syntax and renderer are stable)
4. **Phase 11** (start after syntax stabilizes)

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 10 / green tree) | 10 |
| `let` variable animation | Superseded by easing functions in `always` blocks (6.8.3). Keyframed `let` tracks would need new timeline infrastructure; `always` lerp covers the same use cases statelessly. | Post-10 |
| **AI / NL Integration** | Requires external AI service (OpenAI, Claude, local LLM). No runtime dependency on AI should be mandatory. Includes: NL command bar, agent suggestion UI, agent_suggestions component. | Post-10 or separate product |
| **Row double-click / right-click** | No defined user story. Fields were wired to egui events but no caller consumed them. Re-add when a feature needs them. | When needed |
| **Badge button component** | Fully implemented but no caller. Re-add when the UI needs count badges (e.g. "Errors: 3"). | When needed |
| **Pre-compile plot closures** | Compile `func` AST bodies to closures/bytecode once per build instead of tree-walking thousands of times per curve. Would give 10–50× sampling speedup but requires a stable closure compilation API. | Post-10 or when plot count becomes a bottleneck again |
