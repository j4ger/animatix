# Animatix Implementation Plan

This plan is intentionally grounded in the runtime that exists today. It does not treat parser-only syntax, design sketches, or low-level Rust helpers as shipped user-facing features.

---

## 1. Current Baseline

### Shipped Runtime Surface

| Area | Status | Notes |
|---|---|---|
| Core scene primitives | Implemented | `Text`, `Math`, `Svg`, `Circle`, `Rect` |
| Reactive evaluation | Implemented | `always`, `loop`, `yield`, labeled loop state, compile-time `for` expansion |
| Plotting | Implemented | `Graph`, `CartesianPlot`, `PolarPlot`, `tolerance`, `max_depth`, discontinuity handling |
| Containers | Implemented for Phase 1 layout foundation | `Row`, `Col`, `Grid`, `Stack`, and `Group` are usable; root layout defaults, scene-relative placement, and manual child overrides are implemented |
| Actions | Partially implemented | Built-ins currently: `fade-in`, `wipe-in`, `fade-out` |
| Components | Parser-only | AST/parser exist; runtime instantiation does not |

### Known Gaps

These are the major holes between the documented language surface and the runtime:

1. **Primitive coverage is still too narrow.** The runtime’s basic shape-actor surface is still centered on `Circle` and `Rect`, even though `Text`, `Math`, `Svg`, and plotting are implemented.
2. **Layout fundamentals are shipped, but the higher-level authoring surface still needs maintenance.** The runtime now supports `Grid`, `Stack`, root layout defaults, scene-relative placement, and explicit manual child overrides; the remaining work is keeping docs/examples aligned and expanding the broader runtime surface.
3. **Components stop at parsing.** Imports work, but reusable component runtime behavior does not exist.
4. **Advanced authoring syntax is ahead of execution.** Morph strategy controls, richer query syntax, and planned plotting types are not implemented.

---

## 2. Planning Principles

The next implementation steps should follow these rules:

1. **Reduce parser/runtime mismatch first.** The highest-value work is closing gaps where the language appears larger than the runtime really is.
2. **Prefer vertical slices over broad promises.** Each phase should deliver runtime behavior, documentation, examples, and validation together.
3. **Ship current-facing features before future-facing syntax.** A smaller reliable language is better than a broader but misleading one.
4. **Keep examples honest.** User-facing demos should only showcase runnable features. Planned syntax belongs in a clearly separated planned section.

---

## 3. Recommended Execution Order

### Phase 0 — Surface Alignment
**Status:** In progress / immediate maintenance work

**Goal:** Make the docs, examples, and roadmap reflect the real engine.

**Includes:**
- Rewrite docs to distinguish runtime-supported vs parser-only features
- Curate examples into a small runnable set
- Move future syntax sketches into clearly marked planned examples

**Exit criteria:**
- No user-facing example depends on unimplemented runtime features
- Docs and roadmap agree on current capabilities

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
1. `Line`
2. `Ellipse`
3. `Arc`
4. `Polygon`
5. `Path`
6. `Image`
7. `Code`

**Why this phase follows layout:**
- It directly reduces the parser/runtime mismatch
- It increases expressive power immediately
- It becomes much more valuable once layout containers can place those primitives effectively

**Suggested scope discipline:**
- Add each primitive end-to-end: parser support (if needed), timeline handling, rendering, docs, and one demo
- Do not bundle all primitives into one risky change

**Exit criteria:**
- At least the first two added primitives are fully runnable and documented
- `docs/primitives.md` can be expanded without caveats for those primitives

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
- Interactive UI/editor
- Hot reload and file watching
- Richer action/component discovery for tooling
- More formal examples/tutorial structure

This phase should not begin until the runtime surface is trustworthy enough that an editor is not teaching unstable or unimplemented syntax.

---

## 4. Work That Is Already Done

These items should be treated as shipped foundations, not future roadmap bullets:

- Reactive `always` / `loop` evaluation model
- `yield`-driven loop state machine
- `for` expansion during timeline building
- Graph plotting with adaptive sampling
- Discontinuity detection for problematic functions like `1/x`
- Bounding-box culling for plotting work
- `Row` / `Col` auto-layout
- Absolute positioning as a working placement mechanism

---

## 5. What We Should Not Do Next

The following would be premature before Phases 2–3 are complete:

- Expanding the spec with more aspirational syntax
- Adding more broken or future-only demos to the main example set
- Building an editor/UI on top of an unstable user-facing language surface
- Jumping straight to a full general-purpose constraint solver

---

## 6. Immediate Next Implementation Recommendation

If implementation starts right after this planning rework, the best next move is:

1. **Implement one additional runtime primitive family**
2. **Update docs and demos for that primitive immediately**
3. **Keep component work behind the now-stable layout and primitive foundation**

The cleanest starting candidates are `Line` and `Ellipse`, because low-level geometry support already exists and they meaningfully expand the scene language without destabilizing the now-shipped layout foundation.
