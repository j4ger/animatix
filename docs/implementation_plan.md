# Animatix Implementation Plan

This plan is intentionally grounded in the runtime that exists today. It does not treat parser-only syntax, design sketches, or low-level Rust helpers as shipped user-facing features.

---

## 1. Current Baseline

### Shipped Runtime Surface

| Area | Status | Notes |
|---|---|---|
| Core scene primitives | Implemented | `Text`, `Math`, `Svg`, `Circle`, `Rect`, `Line`, `Ellipse` |
| Reactive evaluation | Implemented | Stateless `always` evaluation and compile-time `for` expansion are the shipped reactive model |
| Plotting | Implemented | `Graph`, `CartesianPlot`, `PolarPlot`, `tolerance`, `max_depth`, discontinuity handling |
| Containers | Implemented for Phase 1 layout foundation | `Row`, `Col`, `Grid`, `Stack`, and `Group` are usable; root layout defaults, scene-relative placement, and manual child overrides are implemented |
| GUI authoring shell | Implemented as an MVP | `crates/animatix-gui` provides GPUI-based file loading, multiline code editing, save/reload, timeline scrubbing/playback, and cross-platform offscreen live preview |
| Actions | Partially implemented | Built-ins currently: `fade-in`, `wipe-in`, `fade-out` |
| Components | Partially implemented | Imported `pub component` instantiation, parameter binding, nested-label isolation, dotted nested-label assignment targets, and sampled rhs property lookup are runtime-real; lifecycle hooks and custom actions still are not |

### Known Gaps

These are the major holes between the documented language surface and the runtime:

1. **Primitive coverage is much closer to the documented surface.** The runtime now includes `Circle`, `Rect`, `Line`, `Ellipse`, `Arc`, `Polygon`, `Path`, `Image`, and a first v1 `Code` primitive rendered through the text pipeline.
2. **Layout fundamentals are shipped, but the higher-level authoring surface still needs maintenance.** The runtime now supports `Grid`, `Stack`, root layout defaults, scene-relative placement, and explicit manual child overrides; the remaining work is keeping docs/examples aligned and expanding the broader runtime surface.
3. **Component runtime is only partially landed.** Imported `pub component` instantiation now works, nested labels can be targeted and queried through dotted property paths, but lifecycle hooks, custom actions, and richer component scope remain unfinished.
4. **Advanced authoring syntax is ahead of execution.** Morph strategy controls, richer query syntax, and planned plotting types are not implemented.

---

## 2. Planning Principles

The next implementation steps should follow these rules:

1. **Reduce parser/runtime mismatch first.** The highest-value work is closing gaps where the language appears larger than the runtime really is.
2. **Prefer vertical slices over broad promises.** Each phase should deliver runtime behavior, documentation, examples, and validation together.
3. **Ship current-facing features before future-facing syntax.** A smaller reliable language is better than a broader but misleading one.
4. **Keep examples honest.** User-facing demos should only showcase runnable features. Future-facing ideas belong in docs until they are runnable.
5. **Prefer random-access semantics.** Preview, scrubbing, image export, and playback should agree on the frame at time `t` wherever possible.

---

## 3. Recommended Execution Order

### Phase 0 — Surface Alignment
**Status:** In progress / immediate maintenance work

**Goal:** Make the docs, examples, and roadmap reflect the real engine.

**Includes:**
- Rewrite docs to distinguish runtime-supported vs parser-only features
- Curate examples into a small runnable set
- Keep future syntax sketches out of the runnable example set

**Exit criteria:**
- No user-facing example depends on unimplemented runtime features
- Docs and roadmap agree on current capabilities

---

### Phase 0.5 — Reactive Model Simplification
**Status:** Completed

**Outcome:**
- `for` owns structural repetition
- `always` owns runtime reactive behavior
- the shipped evaluation contract is random-access and stateless for the requested time `t`

**Detailed design reference:** [`stateless_reactive_design.md`](stateless_reactive_design.md)

---

### Phase 1 — Layout Foundation for AI Authorship
**Priority:** Completed foundation phase

**Goal:** Make structured layout the default composition path while preserving absolute positioning as a first-class explicit escape hatch.

**Core design decision:**
- auto-layout-first for normal scene composition
- explicit absolute positioning remains fully supported
- parent-driven placement beats free-form constraint solving unless proven insufficient

**Scope:**
1. Fix current layout semantics (`Row` / `Col` behavior, explicit-vs-auto placement distinction)
2. Remove implicit sentinel behavior around `(0, 0)`-as-unset and replace it with explicit layout/manual placement semantics
3. Implement `Stack`
4. Implement `Grid`
5. Add optional container placement defaults
6. Add scene-relative placement primitives (anchors / percentages / scene-derived positions)

**Detailed design reference:** [`layout_design.md`](layout_design.md)

**Current progress note:**
- Phase 1 layout work is now implemented in the runtime: explicit child placement semantics, plot sentinel cleanup, root layout-container defaults, `Stack`, `Grid`, and scene-relative placement have all landed together with tests and a runnable demo.
- The next work after Phase 1 should focus on expanding primitive/runtime surface area rather than revisiting layout fundamentals.

**Why this phase comes first:**
- AI-authored scenes currently pay the highest cost in manual coordinate math
- layout quality has a larger effect on authoring ergonomics than adding more primitive types first
- better layout primitives reduce the need for brittle generated absolute coordinates

**Suggested scope discipline:**
- ship layout in vertical slices with docs and demos
- preserve existing absolute-position scenes
- avoid introducing a full constraint solver in this phase

**Exit criteria:**
- `Grid` and `Stack` are runtime-real
- containers can be authored without mandatory absolute placement in common cases
- absolute positioning still works cleanly for deliberate manual composition
- docs and demos show layout-first scenes as the preferred authoring style
- runtime precedence rules for layout-managed vs manual placement are explicit and tested

---

### Phase 2 — Expand Core Runtime Primitives
**Priority:** High

**Goal:** Close the most obvious runtime surface gap by adding more real scene primitives.

**Recommended implementation order:**
1. `Code` ✅

**Why this phase follows layout:**
- It directly reduces the parser/runtime mismatch
- It increases expressive power immediately
- It becomes much more valuable once layout containers can place those primitives effectively

**Suggested scope discipline:**
- Add each primitive end-to-end: parser support (if needed), timeline handling, rendering, docs, and one demo
- Do not bundle all primitives into one risky change
- Prefer explicit, low-risk v1 property surfaces over broader shorthand syntax

**Current progress note:**
- `Image` is now implemented, including runtime rendering and timeline-driven property animation.
- `Code` is now implemented as a conservative v1 primitive using the text-path pipeline, with parser/runtime/docs/demo coverage.

**Current v1 recommendation for the next remaining primitive work:**
- `Arc`: `radius_x`, `radius_y`, `start_angle`, `sweep_angle`; stroke-first rather than filled sector semantics
- `Polygon`: explicit `points` input rather than generated regular polygons
- `Path`: structured `commands` using existing expression call syntax instead of a new path-string parser

**Exit criteria:**
- `Arc`, `Polygon`, and `Path` are fully runnable and documented
- `docs/primitives.md` can describe the remaining primitive gaps without caveats for these three

---

### Phase 3 — Component Runtime
**Priority:** High, but after primitive/layout stabilization

**Goal:** Turn `pub component ...` from parser-only syntax into a usable runtime feature.

**Must include:**
- Component instantiation from imported files
- Parameter binding
- Local component scope rules
- Clear behavior for nested labels and exported names

**Should defer until later within this phase:**
- Lifecycle hooks
- Custom component actions
- `@config` support

**Why this phase is later than primitives/layout:**
- It is the largest semantic step in the language
- It multiplies complexity around scope, imports, and instantiation
- It benefits from having the base primitive/layout surface already stable

**Exit criteria:**
- A minimal imported component example renders successfully
- The spec can describe component behavior without parser-only disclaimers

**Current progress note:**
- A minimal runtime slice is now shipped: imported `pub component` definitions can be instantiated, params bind from instance props/defaults, and nested labels are isolated per instance during expansion.
- Dotted assignment targets now work against nested runtime labels, including those emitted by imported component expansion.
- Rhs dotted property lookup now resolves sampled actor and scene values through the same runtime label space, enabling composition such as copying `node.at` or `node.radius` into other properties.
- Lifecycle hooks, custom component actions, and richer component namespace rules still remain for later work inside this phase.

---

### Phase 4 — Advanced Plotting and Morph Controls
**Priority:** Medium

**Goal:** Build on the now-stable base language with higher-level expressive features.

**Candidate work:**
- `ParametricPlot`
- `ImplicitPlot`
- DSL-level morph controls such as `strategy`, `path_arc`, and `stretch`
- Better path/stroke effects such as trimming and dashing

**Why this is not earlier:**
- These features are valuable, but they are not the most harmful current gap
- They should be added once the basic runtime surface is less misleading

**Exit criteria:**
- Each new feature has one focused demo and one focused spec section

---

### Phase 5 — Authoring UX and Tooling
**Priority:** Later

**Goal:** Improve the creator experience after the language/runtime foundation is dependable.

**Candidate work:**
- Extend the shipped GUI beyond its MVP scope
- Hot reload and file watching
- Richer action/component discovery for tooling
- More formal examples/tutorial structure

**Current progress note:**
- A first GPUI-based GUI MVP now exists in `crates/animatix-gui`.
- It is intentionally narrow: multiline code editing, save/reload, timeline scrubbing/playback, and offscreen live preview presented inside GPUI.
- Future work in this phase is about improving that shell, not starting from zero.

---

## 4. Work That Is Already Done

These items should be treated as shipped foundations, not future roadmap bullets:

- Reactive `always` evaluation model
- `for` expansion during timeline building
- Random-access stateless frame evaluation contract
- Graph plotting with adaptive sampling
- Discontinuity detection for problematic functions like `1/x`
- Bounding-box culling for plotting work
- Runtime `Line` and `Ellipse` scene primitives
- Runtime `Arc`, `Polygon`, and `Path` scene primitives
- Runtime `Image` scene primitive and animated image properties
- `Row` / `Col` auto-layout
- Absolute positioning as a working placement mechanism
- GPUI-based GUI MVP for preview/scrubbing/editing

---

## 5. What We Should Not Do Next

The following would be premature before Phases 2–3 are complete:

- Expanding the spec with more aspirational syntax
- Adding more broken or future-only demos to the main example set
- Jumping straight from the shipped GUI MVP to a full visual editor on top of an unstable user-facing language surface
- Jumping straight to a full general-purpose constraint solver

---

## 6. Immediate Next Implementation Recommendation

Now that `Line`, `Ellipse`, `Arc`, `Polygon`, `Path`, `Image`, `Code`, and the first GUI MVP are shipped, the best next move is:

1. **Keep component work behind the now-stable layout and primitive foundation**
2. **Continue improving the GUI incrementally**
3. **Leave advanced plotting and morph controls for their dedicated later phase**

The cleanest next candidate is now deeper component runtime work, followed by targeted improvements to the GUI MVP.
