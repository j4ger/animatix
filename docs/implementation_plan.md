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
| Containers | Partially implemented | `Row`, `Col`, `Group` are usable; `Grid`, `Stack` are not |
| Actions | Partially implemented | Built-ins currently: `fade-in`, `wipe-in`, `fade-out` |
| Components | Parser-only | AST/parser exist; runtime instantiation does not |

### Known Gaps

These are the major holes between the documented language surface and the runtime:

1. **Primitive coverage is too narrow.** The runtime only exposes `Circle` and `Rect` as scene shape actors.
2. **Layout is incomplete.** `Grid` and `Stack` are still missing.
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

### Phase 1 — Expand Core Runtime Primitives
**Priority:** Highest next implementation phase

**Goal:** Close the most obvious runtime surface gap by adding more real scene primitives.

**Recommended implementation order:**
1. `Line`
2. `Ellipse`
3. `Arc`
4. `Polygon`
5. `Path`
6. `Image`
7. `Code`

**Why this phase comes first:**
- It directly reduces the parser/runtime mismatch
- It increases expressive power immediately
- It unlocks more meaningful demos and lowers pressure to over-promise future features

**Suggested scope discipline:**
- Add each primitive end-to-end: parser support (if needed), timeline handling, rendering, docs, and one demo
- Do not bundle all primitives into one risky change

**Exit criteria:**
- At least the first two added primitives are fully runnable and documented
- `docs/primitives.md` can be expanded without caveats for those primitives

---

### Phase 2 — Complete Layout Containers
**Priority:** High

**Goal:** Finish the scene layout model by implementing `Grid` and `Stack`.

**Scope:**
- `Grid`: row/column placement, `gap`, and predictable child flow
- `Stack`: overlapping placement with transform inheritance and simple alignment rules

**Why after primitives:**
- Layout is more valuable when there are more primitives to place
- The current engine already has enough container infrastructure to make this a focused extension

**Exit criteria:**
- `Grid` and `Stack` are runtime-real, not just documented names
- A dedicated layout demo can show all five containers: `Row`, `Col`, `Group`, `Grid`, `Stack`

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

---

## 5. What We Should Not Do Next

The following would be premature before Phases 1–3 are complete:

- Expanding the spec with more aspirational syntax
- Adding more broken or future-only demos to the main example set
- Building an editor/UI on top of an unstable user-facing language surface

---

## 6. Immediate Next Implementation Recommendation

If implementation starts right after this planning rework, the best next move is:

1. **Implement one additional runtime primitive family**
2. **Update docs and demos for that primitive immediately**
3. **Then move on to layout completion**

The cleanest starting candidates are `Line` and `Ellipse`, because low-level geometry support already exists and they meaningfully expand the scene language without forcing the component system to land first.
