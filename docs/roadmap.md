# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

Items are grouped into implementation batches based on dependencies and shared systems.

### Batch 1: Animation & Visual Effects
Enhance visual storytelling through actions and entrance animations.

| # | Task | Notes |
|---|------|-------|
| 1 | **`draw-in` for PlotCurve and Text** | PlotCurve: entrance action as a documented alias for `stroke_progress = 0 → 1.0 [duration]` animation. Text: typewriter/type-on effect revealing characters progressively. |
| 2 | **Exclusive highlight groups** | `Equation` containers auto-unhighlight previous `Fragment` when a new one is highlighted — only one (or one group) highlighted at a time. Multi-target syntax: `highlight {f1, f2} [color: white, 800ms]`. Manual `unhighlight` preserved for clearing without activating a new target. |

### Batch 2: Graph Coordinate System
Extend Graph's coordinate transformation system. Do #4 (padding) first as it affects the coordinate space.

| # | Task | Notes |
|---|------|-------|
| 3 | **Graph padding/insets** | Support configurable padding and insets within `Graph` containers. `graph.map()` should respect these when computing coordinate transforms. |
| 4 | **Graph inverse mapping** | `graph.map_inverse(screen_x, screen_y) → math_coords` — convert screen coordinates back to graph coordinates. Useful for interactive elements and hit-testing. Depends on padding (#3). |
| 5 | **Graph log scaling** | Support logarithmic axis scaling in `Graph` via `scale: "log"` property. `graph.map()` should apply log transforms when computing coordinates. Depends on padding (#3). |

### Batch 3: Language & Parser
Isolated parser and evaluation environment work.

| # | Task | Notes |
|---|------|-------|
| 6 | **`for` loop: tuple destructuring + closure capture** | Two verified gaps: (a) tuple destructuring `for (a, b) in ...` not supported — parser (`parser/inline.rs`) only accepts simple identifiers via `ident()`; (b) closures in dynamic `PlotCurve`s (referencing `t`) don't capture loop variables — `Value::Closure` stores only args + AST body without an environment snapshot, and the render-time frame environment (`frame_env.rs`) doesn't include build-time loop vars. Static plots (no `t`) work fine since sampling happens at build time. Fix both: add destructuring to parser, and either snapshot the environment into closures or inject loop vars into plot params. |

### Batch 4: Plot System Extensions
Extend the plot transition system. Do #7 first, then #8 after profiling.

| # | Task | Notes |
|---|------|-------|
| 7 | **Plot function transitions: implicit plots** | Extend func transitions to implicit plots (`f(x,y) = 0`). Scalar-field blend is conceptually clean but marching-squares interaction with moving zero-contour needs visual validation. |
| 8 | **Plot transitions: adaptive quality** | During func transitions (especially cascading), lower `max_depth` / raise `tolerance` to reduce per-frame eval cost. Nested blends cause 2^N evaluations per sample point. Measure first; add only if profiling demands. Depends on having transitions to measure (#7). |

### Batch 5: Educational Primitives
New primitive types for educational content.

| # | Task | Notes |
|---|------|-------|
| 9 | **Callout/annotation primitive** | `Callout { target: actor, text: "...", arrow: true }` for educational diagrams — labeled arrow pointing at a specific actor or plot element. Not yet designed or implemented. |

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
