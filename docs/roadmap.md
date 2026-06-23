# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

| # | Task | Notes |
|---|------|-------|
| 2 | **Scene persistence (`persist` / `remove`)** | Carry actors across `play` transitions. Opt-in `persist actor` at a keyframe; explicit `remove actor` to drop. Persist-until-removed model — survives multiple transitions until explicitly removed. Design needed: interaction with morphing, re-declaration in new scene, and scene-level config inheritance. |
| 4 | **`draw-in` for PlotCurve and Text** | PlotCurve: entrance action as a documented alias for `stroke_progress = 1.0 [duration]` animation. Text: typewriter/type-on effect revealing characters progressively. |
| 5 | **`for` loop: tuple destructuring + closure capture** | Two verified gaps: (a) tuple destructuring `for (a, b) in ...` not supported — parser (`parser/inline.rs`) only accepts simple identifiers via `ident()`; (b) closures in dynamic `PlotCurve`s (referencing `t`) don't capture loop variables — `Value::Closure` stores only args + AST body without an environment snapshot, and the render-time frame environment (`frame_env.rs`) doesn't include build-time loop vars. Static plots (no `t`) work fine since sampling happens at build time. Fix both: add destructuring to parser, and either snapshot the environment into closures or inject loop vars into plot params. |
| 6 | **Exclusive highlight groups** | `Equation` containers auto-unhighlight previous `Fragment` when a new one is highlighted — only one (or one group) highlighted at a time. Multi-target syntax: `highlight {f1, f2} [color: white, 800ms]`. Manual `unhighlight` preserved for clearing without activating a new target. |
| 8 | **Graph inverse mapping** | `graph.map_inverse(screen_x, screen_y) → math_coords` — convert screen coordinates back to graph coordinates. Useful for interactive elements and hit-testing. Deferred from #3 to keep initial scope minimal. |
| 9 | **Graph padding/insets** | Support configurable padding and insets within `Graph` containers. `graph.map()` should respect these when computing coordinate transforms. Deferred from #3 — Manim doesn't have this either, low priority. |
| 10 | **Graph log scaling** | Support logarithmic axis scaling in `Graph` via `scale: "log"` property. `graph.map()` should apply log transforms when computing coordinates. Deferred from #3 — separate feature, requires extending the mapping formula. |
| 11 | **Callout/annotation primitive** | `Callout { target: actor, text: "...", arrow: true }` for educational diagrams — labeled arrow pointing at a specific actor or plot element. Not yet designed or implemented. |
| 12 | **Plot function transitions: implicit plots** | Extend func transitions to implicit plots (`f(x,y) = 0`). Scalar-field blend is conceptually clean but marching-squares interaction with moving zero-contour needs visual validation. Deferred from #1. |
| 13 | **Plot transitions: adaptive quality** | During func transitions (especially cascading), lower `max_depth` / raise `tolerance` to reduce per-frame eval cost. Nested blends cause 2^N evaluations per sample point. Measure first; add only if profiling demands. Deferred from #1. |

---

## Technical Debt

Architectural issues, code quality problems, and infrastructure gaps that should be addressed but aren't blocking feature work. Organized by priority within each category.

### Parser & DSL Bugs

User-facing bugs that cause silent failures or confusion.

| # | Task | Notes |
|---|------|-------|
| 14 | **Brace-style property diagnostic** | `Actor { prop: val }` silently drops properties (braces are for children only). Add parser warning: `"property 'X' has no preceding actor to attach to; did you mean 'Type, X: val'?"`. Also warn when props attach to `SlotMarker`, `ForLoop`, or `SlotFill` (currently silent drop in `inline.rs:138-148`). Add parser test documenting this behavior. |
| 15 | **BarChart `data` format diagnostic** | `data: {10, 20, 30}` silently produces empty chart (parser expects `{("A", 10), ("B", 20)}` tuples). Add diagnostic when flat number list is detected: `"BarChart data expects (label, value) tuples, got flat numbers"`. Consider supporting auto-labeling for flat lists as a convenience feature. |

### Code Quality

Systemic issues that make the codebase fragile or error-prone.

| # | Task | Notes |
|---|------|-------|
| 16 | **Audit silent property dropping** | Multiple properties (`bar_colors`, `bar_width`, `gap`, `show_axis`, `max_value`, `points`) silently drop unrecognized values instead of evaluating through the environment. This is a recurring anti-pattern. Audit all property parsers, replace direct `Expr` matching with evaluation helpers (`evaluate_expr_with_lookup_diagnostic`), and establish a codebase convention: "always evaluate, never silently drop." Consider adding a lint or code review checklist item. |
| 17 | **`PropertyTrack::keyframes()` trait bounds** | The `keyframes()` method requires `T: Interpolate` even though reading keys from a `BTreeMap` doesn't need interpolation. This forced unnecessary trait bounds on GUI helper functions that only read keyframe timestamps. Add a `keyframes_raw()` method without the `Interpolate` bound for read-only access. |

### Architectural Debt

Design issues that create long-term maintenance burden.

| # | Task | Notes |
|---|------|-------|
| 18 | **Consolidate evaluation paths** | Two complete evaluation engines exist: tree-walker (`utils.rs`) and IR/VM (`ir/eval.rs`). Both need identical updates for every new feature (e.g., `NativeFn` dispatch had to be added to both `eval_method` implementations). This is a divergence risk. Long-term: consolidate into a single evaluation engine, or extract shared logic into a common module. |
| 19 | **Unify coordinate system conventions** | `math_to_screen` in `plot.rs:1266` returns centered offsets (for plot-curve geometry), while `build/property.rs:240` returns absolute screen coords (offset + `at`). Same mathematical concept, different conventions. This causes confusion and is a footgun for plot rendering work. Document the two conventions clearly, or unify them into a single API with explicit `relative: bool` parameter. |
| 20 | **`func` transitions as side-channel** | `func` is `BuildTimeOnly` in the property registry, but transitions use a parallel `func_transitions` field on `AnimationTrack`. The reason (closures can't implement `Interpolate`) is sound, but the result is that `func` behaves differently from every other animatable property. Future plot types (VectorField, Heatmap, ContourSet) will each need their own parallel transition system. Consider a more general "non-interpolatable property transition" framework, or accept the side-channel pattern and document it clearly. |

### Infrastructure & Tooling

Build system, CI, and tooling gaps.

| # | Task | Notes |
|---|------|-------|
| 21 | **Workspace-wide CI coverage** | The GUI crate had 134 compilation errors that went unnoticed because CI only runs `cargo check` (not `cargo check --workspace`). Update CI config and `AGENTS.md` pre-commit gates to include `--workspace` flag. This prevents GUI drift from recurring. |
| 22 | **Align tree-sitter grammar with PEG parser** | The tree-sitter `children_block` rule expects `repeat($._statement)` (full statements), but the PEG parser uses `inline_items` (flat comma-separated actor+property sequences). This causes syntax highlighting/LSP to diverge from actual accepted syntax. Update `grammar.js` to use an `inline_item` rule matching the PEG parser's behavior. |
| 23 | **`rusty_ffmpeg` as opt-in feature** | The default feature set includes `rusty_ffmpeg`, which requires system FFmpeg libraries and causes `cargo build` to fail without them. Most development doesn't need video export. Make `video` an opt-in feature (`--features video`) rather than default. Update `Cargo.toml` default features and document the change. |

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
