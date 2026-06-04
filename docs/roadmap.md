# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).
> For design specifications, see [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Order

1. **Phase 4** PiP / Multi-Viewport
2. **Phase 5** Editor Infrastructure (green tree, WASM)
3. **Phase 6** QoL & Polish

---

## Phase 4 — PiP / Multi-Viewport

> **Deferred.** The current viewport system has been removed. PiP will be implemented as an actor-level `Scene` primitive, not statement-level declarations.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Design `Scene` primitive** | Actor type whose content is another scene's timeline. Position, size, opacity are animatable properties (keyframes). `scene` property names the scene to render. | `primitives/`, `timeline/track.rs` | 3 days | Stable syntax |
| 2 | **Scene reference rendering** | Renderer evaluates referenced scene timeline at current time, clips to actor bounds, transforms to actor position, applies actor opacity. | `timeline/scene_eval.rs`, `renderer/` | 1 week | 1 |
| 3 | **Inspector + timeline support** | Scene actors show up in timeline tracks, inspector panel, and gizmo selection like any other actor. | `app/panels/` | 3 days | 2 |

---

## Phase 5 — Editor Infrastructure

> Long-term foundational work. Blocked on syntax stabilization.

| # | Item | What | Files | Effort | Blocker |
|---|------|------|-------|--------|---------|
| 1 | **Green tree (rowan)** | Immutable syntax tree with cheap clones. Enables lossless source manipulation, reliable formatting, and incremental parsing. | New crate `animatix-green` | 2–3 months | Stable syntax |
| 2 | **Trivia-inspired AST** | Whitespace and comment preservation in AST. Enables formatter and non-destructive edits. | `animatix-green/` | 2–3 months | 1 |
| 3 | **Web canvas / WASM** | Alternative renderer backend for browser export. Uses same timeline but renders to HTML canvas or WebGPU. | New crate `animatix-web` | Very high | Alternative renderer |
| 4 | **Snippet AST parsing** | Parse snippet text into `Vec<Stmt>` and insert via `SourceEdit` instead of raw text surgery. Requires lossless parsing (green tree) to preserve formatting. | `app/insertion.rs`, `animatix-green/` | 2 days | 2 |

---

## Phase 6 — QoL & Polish

> Small quality-of-life improvements that add up to a polished experience.

| # | Item | What | Files | Effort |
|---|------|------|-------|--------|
| 6.5 | **Export format expansion** | Added WebM (VP9), MOV (H.264), and WebP. APNG requires a new animated PNG encoder backend, not yet implemented. | `app/shell/export_dialog.rs`, `renderer/encode/` | 1 week |

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| **APNG export** | Requires animated PNG encoder backend (frames → APNG). The `image` crate does not support APNG encoding out of the box. | Post-6 |
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 5 / green tree) | 5 |

---

## Deferred (not on critical path)

| Item | Why deferred | Likely phase |
|------|--------------|--------------|
| `animatix-cli lint` / `format` | Requires trivia-aware AST (Phase 5 / green tree) | 5 |
| `let` variable animation | Superseded by easing functions in `always` blocks. Keyframed `let` tracks would need new timeline infrastructure; `always` lerp covers the same use cases statelessly. | Post-5 |
| **Pre-compile plot closures** | Compile `func` AST bodies to closures/bytecode once per build instead of tree-walking thousands of times per curve. Would give 10–50× sampling speedup but requires a stable closure compilation API. | Post-5 or when plot count becomes a bottleneck again |
| **Unify duplicate PropertyValue types** | Two separate `PropertyValue` enums exist: `animatix::timeline::property_engine::PropertyValue` (engine-level) and `animatix_gui::app::commands::PropertyValue` (GUI-level). Different variant names (`F32` vs `Float`, `String` vs `Text`) force conversion logic in `apply_property_edit_to_track`. Unify into one canonical type. | When touching property dispatch again |
| **Replace `node_local_bounds` with trait-based bounds** | `node_local_bounds` takes `&[VelloPath]` forcing callers to materialize paths just for bounds computation. A `trait HasLocalBounds` on `VelloPath`/`TextPath`/`SceneImage` would be cleaner and allow lazy evaluation. | When touching scene_eval bounds logic |
| **Zero-readback filter compositing (end-to-end)** | Infrastructure is complete: `FullscreenBlitPipeline` supports alpha, `GpuFilterBackend` exposes `render_and_filter_scene_to_view()` and `take_last_filtered_view()`. Remaining work: modify `scene_eval.rs` to not draw filtered images into the Vello scene, and update `PreviewSurface`/`OffscreenRenderer` to blit the GPU texture after the base Vello render. `FilteredSource` tracking should be simplified to avoid fragile pointer comparison. | When filter performance matters |
| **Audio playback in preview** | Audio segments are collected for export muxing but not played back during GUI preview. Requires an audio output backend (rodio/cpal). | Post-1 or separate feature |
| **Variable track UI** | `let` declarations inside keyframes create `VariableTrack` entries. No GUI to view or edit these. Advanced feature, low demand. | When needed |
| **Module dependency graph** | Visual graph of imports between `.amx` files. Internal tooling feature. | When needed |
| **Scene duration editing** | Add `duration` property to scene declarations (currently implicit). Inspector shows editable duration field. Requires `Stmt::Scene` AST extension. | Phase 5 |
| **Scene block drag in timeline** | Drag scene blocks in the composition timeline to change start times. Start times are derived from walk order + durations; needs design. | Phase 5 |
| **AssetCache ↔ timeline cross-reference** | `AssetCache` and timeline tracks store asset data in parallel with no cross-references. The asset manager cannot show "which actors reference this asset" without AST re-scanning. | When touching asset system |
| **Validate `CreateActor` props** | `DocumentController::handle_create_actor` blindly appends `props` to the actor declaration with no type checking, duplicate detection, or required-field validation. | When touching actor creation |
