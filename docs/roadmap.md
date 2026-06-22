# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Completed

### Layout & Typography Improvement Roadmap (7 phases)

All 7 phases of the layout/typography improvement roadmap are complete:

- **Phase 1:** Typography properties (`font_weight`, `font_style`, `line_height`, `letter_spacing`, `word_spacing`)
- **Phase 2:** (Container sizing — already shipped prior to roadmap)
- **Phase 3:** Per-axis gap (`gap: (row, col)`), per-side padding (`padding: (top, right, bottom, left)`), Stack `align`
- **Phase 4:** (Existing layout features — already shipped)
- **Phase 5:** Text wrapping (`text_max_width`, `text_align`, `overflow`), automatic container-to-child width propagation
- **Phase 6:** Baseline alignment (`vertical_align: "baseline"` on Row/Col)
- **Phase 7:** Percentage and intrinsic sizing (`"50%"`, `fill`, `auto`/`fit`, `min_width`, `max_height`)

### Track System Refactoring

Complete overhaul of the animation track system for correctness, maintainability, and performance:

- **Registry-driven field enumeration** — Single source of truth via `PROPERTY_REGISTRY` + `ActorField` enum; eliminated 5 hand-maintained field lists that caused silent data loss bugs
- **Correctness fixes** — Fixed `max_keyframe_time`, `has_any_keyframes`, `keyframe_times_s` to include all fields (transform, highlight, filter, metrics); fixed `TextMaxWidth` dispatch collision; completed `field_ref`/`field_mut` coverage for all 22 `ActorField` variants; fixed `is_currently_animating` semantics
- **Tier-based sub-structs** — Decomposed flat 56-field `AnimationTrack` into 6 focused sub-structs: `FilterTracks`, `HighlightTracks`, `ShapeTracks`, `TextTracks`, `StyleTracks`, `GeometryTracks`
- **Module split** — Split monolithic `track.rs` (~2500 lines) into 5 focused modules: `property_track.rs`, `animation_track.rs`, `dispatch.rs`, `actor_kind.rs`, `morph.rs`
- **Maintainability** — Collapsed 5×11-arm duplicated matches into `TrackFieldRef` inherent methods; unified evaluate logic via `interpolation_segment` helper; simplified `read_property_value_or_default` signature
- **Idioms** — Added `Debug` derives, manual `Clone` (drops memo cache), `Interpolate: Clone` supertrait, `pub(crate)` field visibility with accessor methods, cached beyond-last-keyframe results
- **Test coverage** — Added 90 regression tests (472 total): field_ref coverage, write/read round-trips, registry iteration, cache invalidation, interpolation semantics

### Auto Color Cycling Per Instance

`color: auto` now assigns distinct colors per instance (unique label), not per primitive type. Each actor gets its own slot in the deterministic palette cycle.

---

## Planned

### Architecture & Maintainability

### Primitives & Syntax

- [x] **Callout / annotation primitive** — A `Callout` primitive that draws a labeled arrow from a text label to a target coordinate. Implemented with `from`, `to`, `head_size`, `label`, and `label_at` properties. See `examples/callout_example.amx`.
- [x] **Legend primitive** — A `Legend` container that auto-generates color swatches + labels from scene content. Currently uses placeholder entries; scene scanning will be implemented in a future update.
- [ ] **Text property easing** — Smooth per-character morphing for `text` content changes (`Text.text`, `Typst.content`). Currently text path arrays support `Fade` morph strategy (cross-fade via opacity), but the `text_content` string itself snaps instantly. True per-character morphing (e.g., "Hello" → "World" letter-by-letter) would require character-level diffing and staggered interpolation. Workaround: multiple overlapping actors with staggered fade-in/out.

---

## Icebox

Not strictly needed, ones that require more design, or simply weird thoughts that came to mind. Should be ignored when planning for implementation, in most cases.

| Task | Reason |
|------|--------|
| **Scene primitive / picture-in-picture** | Transition blending shipped; existing components and `Stack` cover most reuse cases. |
| **Export performance: pre-compiled plot closures** | Only matters for many plot actors or heavy sampled fields. |
| **Asset usage tracking** | Show which actors reference an asset; no strong user story yet. |
| **Variable track UI** | GUI for `let` variable tracks; `always` blocks cover most interactive cases. |
| **Module dependency graph** | Visual graph of `.amx` imports; internal tooling value only so far. |
| **Lossless whitespace/trivia preservation** | Current write-back pipeline correct for all normal use cases; comments roundtrip, formatting idempotent. |
| **APNG export** | Request-driven only; GIF covers lightweight previews, video/WebM covers higher-quality sharing. |
| **Source-diff preview sidecar** | Show the `.amx` diff when dragging actors or editing properties in the inspector. |
| **Animation heatmap view** | Heatmap of animated property density across time, actors, categories. Useful for large generated `.amx` files. |
| **Auto-sorted property registry** | Keep manually sorted with `registry_is_sorted` guard; proc-macro adds more maintenance surface than it removes. |
| **Interactive step control (presentational mode)** | Manim-style `wait()` / `next_slide()`. Architecturally incompatible with Animatix's declarative deterministic playback model. GUI scrubbing covers most use cases. |
| **Auto-arrow layout** | Arrows that auto-connect actor positions. Niche use case; workaround via manual `Arrow` with hardcoded coords. |
| **Per-actor exit before scene transition** | Animate individual actors out before `play SceneName [fade, ...]`. Workaround: `fade-out` actions timed at scene end. Transition blending is already uniform. |
