# Animatix Roadmap

Keep track of what is yet to be done here, when a segment is fully done, remove the items from here.

---

## Planned

| # | Task | Notes |
|---|------|-------|
| 1 | **Plot function transitions** | Animate `PlotCurve.func` between two functions over time (keyframe-driven). Limited to plots. Design needed: interpolation strategy (linear blend of sampled points? parametric morph? domain warping?). |
| 3 | **Scene persistence (`persist` / `remove`)** | Carry actors across `play` transitions. Opt-in `persist actor` at a keyframe; explicit `remove actor` to drop. Persist-until-removed model — survives multiple transitions until explicitly removed. Design needed: interaction with morphing, re-declaration in new scene, and scene-level config inheritance. |
| 4 | **`at:` + `anchor:` conflict diagnostic** | Emit a clear warning when both `at:` and `anchor:` are specified on the same actor. Currently anchor silently wins and `at:` is dropped, causing visual bugs with no obvious cause. Consider unifying `at:` and `offset:` semantics to prevent the mistake entirely. |
| 5 | **Graph coordinate mapping** | `graph.map(math_x, math_y) → screen_coords` method on `Graph` for use in `always` blocks. Eliminates manual magic-number coordinate conversion (`400 + mx * 70`). Needs further design — may get complicated to implement (interaction with padding, insets, axis transforms). |
| 6 | **Missing scheme tokens audit** | Audit all colorschemes for undefined tokens referenced in examples and specs (e.g., `text.muted` in `editorial-dark`). Define missing tokens or remove references. |
| 7 | **`draw-in` for PlotCurve and Text** | PlotCurve: entrance action as a documented alias for `stroke_progress = 1.0 [duration]` animation. Text: typewriter/type-on effect revealing characters progressively. |
| 8 | **`for` loop: tuple destructuring + closure capture** | Two verified gaps: (a) tuple destructuring `for (a, b) in ...` not supported — parser (`parser/inline.rs`) only accepts simple identifiers via `ident()`; (b) closures in dynamic `PlotCurve`s (referencing `t`) don't capture loop variables — `Value::Closure` stores only args + AST body without an environment snapshot, and the render-time frame environment (`frame_env.rs`) doesn't include build-time loop vars. Static plots (no `t`) work fine since sampling happens at build time. Fix both: add destructuring to parser, and either snapshot the environment into closures or inject loop vars into plot params. |
| 9 | **Exclusive highlight groups** | `Equation` containers auto-unhighlight previous `Fragment` when a new one is highlighted — only one (or one group) highlighted at a time. Multi-target syntax: `highlight {f1, f2} [color: white, 800ms]`. Manual `unhighlight` preserved for clearing without activating a new target. |
| 10 | **Callout/annotation primitive** | `Callout { target: actor, text: "...", arrow: true }` for educational diagrams — labeled arrow pointing at a specific actor or plot element. Not yet designed or implemented. |

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
