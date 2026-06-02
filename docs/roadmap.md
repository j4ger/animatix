# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Phase 7 — Audio

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Audio multi-segment muxing** | Concatenate multiple audio files via ffmpeg into final output. Support per-scene audio tracks. | `export/ffmpeg.rs` | 3 days | — |

---

## Phase 9 — PiP / Multi-Viewport

> **Deferred.** The current viewport system has been removed. PiP will be implemented as an actor-level `Scene` primitive, not statement-level declarations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Design `Scene` primitive** | Actor type whose content is another scene's timeline. Position, size, opacity are animatable properties (keyframes). `scene` property names the scene to render. | `primitives/`, `timeline/track.rs` | 3 days | Stable syntax |
| 2 | **Scene reference rendering** | Renderer evaluates referenced scene timeline at current time, clips to actor bounds, transforms to actor position, applies actor opacity. | `timeline/scene_eval.rs`, `renderer/` | 1 week | 1 |
| 3 | **Inspector + timeline support** | Scene actors show up in timeline tracks, inspector panel, and gizmo selection like any other actor. | `app/panels/` | 3 days | 2 |

---

## Phase 11 — Editor Infrastructure

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 1 |
| 3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |
| 4 | **Snippet AST parsing** | Parse snippet text into `Vec<Stmt>` and insert via `SourceEdit` instead of raw text surgery. Requires lossless parsing (green tree) to preserve formatting. | `app/insertion.rs`, `animatix-green/` | 2 days | 2 |

---

## Order

1. **Phase 7** (audio — no blockers)
2. **Phase 9** (PiP — after syntax and renderer are stable)
3. **Phase 11** (start after syntax stabilizes)

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
| **Drag batching loses list-property intermediates** | `pending_drag_source_edits` is keyed by `(actor, property)` as a `HashMap`, so modifying the same list property twice during a drag overwrites the first value. Only the final state is flushed. Scalar properties are fine; `child_order` and `points` are affected. | When drag-to-reorder for lists becomes a feature |
| **EditorBuffer::text() returns stale data during cell edits** | When `cells_dirty` is true, `text()` returns the cached `self.text` instead of reconstructing from cells, because it can't return a reference to a temporary. Callers during a drag may see pre-edit source. Requires reconciling cell/text ownership. | When a feature needs read access to mid-edit source |
| **Split frame_env.rs into env + modifier execution** | The file mixes frame environment construction (`build_frame_env`) with modifier execution (`apply_modifier_stmt`, IR, bytecode). They have different stability profiles and should be separate modules. | When modifier runtime changes again |
| **Bench code duplication** | Every bench file has its own `build_test_timeline()` with inline source strings. API changes (e.g. adding `filter_backend` to `evaluate()`) require touching 7+ files. Extract a shared `bench_utils` module. | When the next API change touches benches |
| **Amber flash on rewritten timestamps** | Visual polish: when `adjust_following_relative_keyframe` rewrites a relative offset, flash the timestamp label amber for ~300ms. Nice-to-have UX feedback. | When needed |
| **Unify duplicate PropertyValue types** | Two separate `PropertyValue` enums exist: `animatix::timeline::property_engine::PropertyValue` (engine-level) and `animatix_gui::app::commands::PropertyValue` (GUI-level). Different variant names (`F32` vs `Float`, `String` vs `Text`) force conversion logic in `apply_property_edit_to_track`. Unify into one canonical type. | When touching property dispatch again |
| **Replace `node_local_bounds` with trait-based bounds** | `node_local_bounds` takes `&[VelloPath]` forcing callers to materialize paths just for bounds computation. A `trait HasLocalBounds` on `VelloPath`/`TextPath`/`SceneImage` would be cleaner and allow lazy evaluation. Also simplifies the `command_local_bounds` helper (Phase 10b.1). | When touching scene_eval bounds logic |
| **Zero-readback filter compositing (end-to-end)** | Infrastructure is complete: `FullscreenBlitPipeline` supports alpha, `GpuFilterBackend` exposes `render_and_filter_scene_to_view()` and `take_last_filtered_view()`. Remaining work: modify `scene_eval.rs` to not draw filtered images into the Vello scene, and update `PreviewSurface`/`OffscreenRenderer` to blit the GPU texture after the base Vello render. `FilteredSource` tracking should be simplified to avoid fragile pointer comparison. | When filter performance matters |
