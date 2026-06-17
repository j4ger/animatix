# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

### Architecture & Maintainability

#### Assessment Summary

| Observation | Priority | Fix scope |
|---|---|---|
| AST change propagation | Done | Shared walk layer + full migration of all 29 identified walker functions across 8 files |
| Parser monolith | Done | Split into 5 submodules (`common`, `expr`, `inline`, `stmt`, `top_level`) |
| `process_plot_actor` 13-tuple | Done | Replaced with `ProcessedPlotActor` named struct |
| Formatter boundary | Done | Module docs in `format_core.rs`/`to_source.rs` + architecture.md note |
| For-loop duplication | Done | Centralized via `process_for_loop_stmts` / `process_for_loop_inline_items` |
| GUI AST match duplication | Done | All 7 GUI source_edit walk functions migrated to shared layer |
| Variant coverage guardrails | Done | 4 guardrail tests + explanatory comments at incompatible sites |
| Pre-existing friction | Done | Verified: FFT example in CI, ffmpeg gated, no changes needed |
| Property registry sorting | Icebox | — |


### GUI Design Language Migration

Spec: [`docs/gui_design_language.md`](gui_design_language.md)

- [ ] **Phase 1: Token Refoundation** — Replace flat `design_tokens.rs` with 3-layer module system (primitive `pub(crate)` → semantic `pub` → component-level). Migrate ~50 files from `BG_BASE`/`ACCENT_BLUE` to `semantic::surface::BASE`/`semantic::accent::PRIMARY`. Delete `PAD_*` duplicates. Fix WCAG AA contrast violations.
- [ ] **Phase 2: Component Unification** — Replace 4 ad-hoc button functions with unified `Button` widget (`egui::Widget` trait). Migrate `FontId::new(size, ...)` to `TextRole` enum. Drop `to_uppercase()` in section headers.
- [ ] **Phase 3: Command System Split** — Split 50+ variant `Command` enum into domain packages (`document`, `actor`, `keyframe`, `scene`, `view`, `playback`). Separate undoable from non-undoable types. Refactor 804-line `command_handlers.rs` into domain handlers.
- [ ] **Phase 4: Interaction Layer Upgrade** — Introduce `Gesture` enum + `GestureHandler` trait to replace 763-line `drag_handler.rs`. Implement keyboard navigation framework. Unify scattered `animate_value_with_time` into `anim::transition()`.

### Primitives & Syntax

- [ ] Equation: bare string syntax sugar for anonymous Fragments (currently requires explicit `label: Fragment, content: "..."`)
- [ ] **Callout / annotation primitive** — An `Annotation` or `Callout` primitive that draws a labeled arrow/line from a text label to a target actor or coordinate. Currently requires manual `Arrow` + `Text` with hardcoded `from`/`to`. Useful for educational diagrams (e.g., "this is the 2 Hz component").
- [ ] **Legend primitive** — A `Legend` container that auto-generates color swatches + labels from child actors or an explicit data list. Currently requires manual `Rect` swatches + `Text` rows.
- [ ] **Auto color cycling per instance** — `color: auto` should cycle through a deterministic palette across multiple instances of the same kind (e.g., 3 `PlotCurve` actors get distinct colors). Currently `auto` assigns one color per primitive type, not per instance.
- [ ] **Text property easing** — Smooth interpolation for `text` content changes (`Text.text`, `Typst.content`). Currently text content changes are instantaneous; color/size changes already support easing. Workaround: multiple overlapping actors with staggered fade-in/out.

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
