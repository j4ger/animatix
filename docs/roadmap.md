# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

Items are grouped into implementation batches based on dependencies and shared systems.

### Batch 1: Animation & Visual Effects ✅
Enhance visual storytelling through actions and entrance animations.

| # | Task | Status |
|---|------|--------|
| 1 | **`draw-in` for PlotCurve and Text** | ✅ Complete - PlotCurve uses stroke_progress, Text uses char_progress for typewriter effect |
| 2 | **Exclusive highlight groups** | ✅ Complete - Equation containers auto-unhighlight siblings when a new Fragment is highlighted |

### Batch 2: Graph Coordinate System ✅
Extend Graph's coordinate transformation system.

| # | Task | Status |
|---|------|--------|
| 3 | **Graph padding/insets** | ✅ Complete - Configurable padding via Vec4 property, affects all coordinate transforms |
| 4 | **Graph inverse mapping** | ✅ Complete - `graph.map_inverse(sx, sy)` converts screen coords back to math coords |
| 5 | **Graph log scaling** | ✅ Complete - `x_scale: "log"` and `y_scale: "log"` properties for logarithmic axes |

### Batch 3: Language & Parser ✅
Parser and evaluation environment improvements.

| # | Task | Status |
|---|------|--------|
| 6 | **`for` loop: tuple destructuring + closure capture** | ✅ Complete - Parser supports `for (a, b) in items`, closures capture loop variables for dynamic plots |

### Batch 4: Plot System Extensions ✅
Extend the plot transition system with implicit plots and performance optimizations.

| # | Task | Status |
|---|------|--------|
| 7 | **Plot function transitions: implicit plots** | ✅ Complete - Implicit plots (f(x,y)=0) support function transitions with scalar field blending |
| 8 | **Plot transitions: adaptive quality** | ✅ Complete - Adaptive quality reduction prevents frame drops during cascading transitions |

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
