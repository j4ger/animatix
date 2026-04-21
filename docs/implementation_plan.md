# Animatix Roadmap

This roadmap starts from the runtime that exists today. It is intentionally grounded in the shipped baseline so the repo does not keep planning documents for work that already landed.

This file is the repository's **master planning document** for product/runtime priorities. Related planning docs should support this roadmap rather than compete with it:

- `docs/colorscheme_design.md` is the detailed design document for the current active roadmap phase
- `docs/architecture_refactor_plan.md` tracks the internal refactor/support lane that should reduce delivery risk without redefining roadmap priority
- execution checklists under `docs/superpowers/plans/` are historical implementation notes once the corresponding phase is complete

---

## 1. Shipped Baseline

The following are already part of the current baseline and should not be treated as active roadmap items:

- Core scene primitives: `Text`, `Math`, `Code`, `Svg`, `Image`, `Circle`, `Rect`, `Line`, `Ellipse`, `Arc`, `Polygon`, and `Path`
- Primitive breadth already shipped on top of that baseline: `Dot`, `Square`, `Arrow`, and `RegularPolygon`
- Plotting: `Graph`, `CartesianPlot`, `PolarPlot`, `ParametricPlot`, and `ImplicitPlot`
- Layout/container foundation: `Row`, `Col`, `Grid`, `Stack`, `Group`, root layout defaults, scene-relative placement, and manual child placement within layout containers
- Reactive model: stateless `always`, compile-time `for`, and random-access frame evaluation
- Component MVP: imported `pub component` instantiation, parameter binding, dotted nested-label assignment targets, and rhs sampled property lookup
- Colorschemes v1: built-in scene selection, semantic color/stroke aliases, and deterministic `color: auto` defaults layered on top of explicit color authoring
- Tooling foundation: CLI renderer, egui-based GUI shell, and `tree-sitter-animatix`
- Shared timing vocabulary already shipped in the runtime contract: duration shorthand, named `delay`, named `ease`, deterministic duplicate-key handling, and explicit instant-change semantics
- Reveal actions v1 now shipped in the runtime contract: `fade-in`, `draw-in`, `wipe-in`, `fade-out`, `wipe-out`, `reveal-out`, `draw-out`, plus honest unsupported-target diagnostics for vector-only reveal verbs
- Motion ergonomics already shipped in the current runtime contract: `move`, `shift`, `rotate`, and `scale`
- Composition ergonomics already shipped in scoped form: `sequence` and `stagger` blocks for actions/property assignments with deliberate diagnostics for unsupported contents
- Scoped morph modifier support already shipped for timed path-morphing re-declarations: `strategy: auto|match`, `path_arc`, and `stretch`

The roadmap below begins after that baseline.

---

## 2. Planning Principles

1. **Optimize for authoring UX, not surface-area parity.** We should not mirror Manim APIs mechanically when Animatix can express the same intent more cleanly.
2. **Keep the shipped contract honest.** Runtime, docs, examples, and tests must agree before we widen the surface again.
3. **Prefer vertical slices over horizontal ambition.** A phase is not done until runtime behavior, examples, docs, and tests all land together.
4. **Preserve random-access semantics.** New animation features must remain compatible with preview, scrubbing, image export, and video export.
5. **Exploit the vector-first architecture.** Features that map cleanly onto tracks, path rendering, diagnostics, and scene-graph traversal should come before architecture-heavy subsystems.
6. **Defer new global models until they are necessary.** Camera systems, compositing-heavy transitions, and rich editor workflows should not outrun the current runtime contract.
7. **Improve feedback loops before richer editor infrastructure.** Diagnostic UX, unsupported-surface explanations, and contract clarity should improve before the GUI depends on another syntax integration layer.

---

## 3. Active Roadmap Overview

The active roadmap begins with **Phase 1 — Colorscheme Follow-Up** (loadable schemes and inheritance). The layout contract and diagnostic UX phases have been completed.

### Internal architecture note

The repository's internal structural cleanup and primitive-system refactor are tracked separately in `docs/architecture_refactor_plan.md`.

That plan is intentionally scoped to internal architecture and refactoring discipline. It should not be treated as a language-surface or shipped-feature roadmap item unless and until it materially changes roadmap sequencing.

### Active supporting design note

The detailed design for the current active phase lives in `docs/colorscheme_design.md`.

That document should be treated as the implementation design for Phase 1, not as a separate competing roadmap.

### Current priority order

1. **Phase 1** — Colorscheme Follow-Up: Loadable Schemes and Inheritance
2. **Phase 2** — Breadth Expansions: Host-Specific Effects and Remaining Practical Surface
3. **Phase 3** — Tooling and Authoring Workflow Refinement
4. Tree-sitter GUI integration only after its authoring value justifies the extra synchronization and maintenance cost

---

## 4. Phase 1 — Colorscheme Follow-Up: Loadable Schemes and Inheritance

**Urgency:** High

**Goal:** Broaden the shipped colorschemes v1 surface into a reusable project-level theming workflow without changing the explicit-color precedence model.

**Why first:**
- Colorschemes v1 already landed and removed a large amount of palette boilerplate.
- The next meaningful colorscheme value is shareability across projects.
- Loadable schemes enable project-level theming without copy-pasting palette blocks.

**Includes:**
- Colorscheme primitive: `Colorscheme "name" { extends: "base", ... }` declarations using standard AMX grammar
- scheme inheritance / extension via the `extends` property
- diagnostics for missing schemes, invalid data, inheritance cycles, and unresolved external tokens
- docs/examples that distinguish built-in-only v1 from the broader reusable scheme story
- optional expression-environment exposure only if it still earns its complexity after the primitive lands

**Guardrails:**
- preserve the current precedence stack where explicit `color`, `stroke`, timed assignments, and `always` overrides beat scheme defaults
- keep the model declarative and load-time/build-time oriented
- do not jump to executable/plugin theming
- do not make GUI work a dependency for shipping the runtime feature

**Exit criteria:**
- users can reuse colorschemes across projects without copy-pasting palette blocks into every `.amx` file
- invalid loads fail honestly and fall back safely
- docs reflect only the broadened behavior that is actually backed by runtime/tests/examples

---

## 5. Phase 2 — Breadth Expansions: Host-Specific Effects and Remaining Practical Surface

**Urgency:** High

**Goal:** Expand capability after the current authoring contract and colorscheme follow-up work are both stable.

**Includes:**
- host-specific effect controls that map cleanly onto real runtime hooks
- any remaining practical primitives/plot helpers that still have clear value after the current shipped breadth
- one focused example and one focused spec section per newly widened surface

**Exit criteria:**
- new breadth features improve authoring range without reintroducing contract ambiguity

---

## 6. Phase 3 — Tooling and Authoring Workflow Refinement

**Urgency:** Medium

**Goal:** Improve discovery, feedback, and day-to-day editing on top of the stabilized runtime contract.

**Includes:**
- continue improving the egui GUI shell
- better diagnostic UX in the GUI: clearer summary surfaces, more actionable contract feedback, and stronger visibility for parse/build/runtime mismatches
- richer action/component discovery based on the real shipped registries
- better example/tutorial structure
- keyboard transport shortcuts and other workflow polish
- use the lowest-maintenance editor feedback path that still reflects the real parser/runtime contract

**Guardrails:**
- do not build richer editor workflows on top of ambiguous language/runtime behavior
- do not introduce a second syntax-maintenance loop unless it clearly improves authoring feedback beyond simpler diagnostic/UI work

---

## 7. Deferred Architectural Work

These remain valuable, but they should stay out of the near-term critical path because they imply broader model changes.

- camera framing, pan, zoom, and other viewport-state features
- `strategy: fade` and other compositing-heavy transition models
- sampled relayout / animated-size-triggered container recomputation beyond the current declaration-time measure/place contract
- hot reload / file watching driven authoring workflows
- scene inspectors, property panels, visual timeline editors, and other larger GUI systems
- native embedded rendering surfaces in the GUI
- multi-file project management UX
- Tree-sitter-backed GUI integration beyond the standalone grammar package

These should only move forward once the action/motion authoring surface is stable enough that we are not redesigning the foundation underneath them.

For Tree-sitter specifically, the standalone grammar package remains valuable and shipped, but GUI consumption should stay out of the near-term critical path until a concrete authoring-feedback gap cannot be solved well through parser/runtime diagnostics, examples, or lighter editor feedback.

---

## 8. Completed Phases

### Phase 1 — Layout Contract Honesty and Narrowed Container Layout (COMPLETED)

**Completed:** 2026-04-20

Aligned `docs/layout_design.md`, `docs/spec.md`, `docs/primitives.md`, `docs/architecture.md`, and runnable examples around the declaration-time measure/place contract. Made clear that containers are a deterministic composition scaffold, not a promise of full CSS-style flex behavior. Documented per-container supported semantics honestly and identified which primitives provide truthful layout size.

Key commits: `c73423d`, `5ea7367`, `ae794ad`, `d521a11`

### Phase 2 — Diagnostic UX and Contract-Surface Feedback (COMPLETED)

**Completed:** 2026-04-21

Sharpened reusable authoring rules and made failure modes clearer and more actionable. Clarified namespace/reachability rules for nested labels, strengthened diagnostics around ambiguous component access, and documented the three diagnostic cases (UnknownTargetPath, UnknownLookupPath, UnsupportedAssignmentProperty) with explicit message templates.

Key commits: `94d3265`, `01815dc`, `e92621d`

---

## 9. What We Should Not Do Next

- treat parser acceptance as proof of runtime support
- widen the action catalog before defining honest target coverage and diagnostics
- start camera or viewport work before local motion semantics are settled
- mix layout semantics, transform semantics, and composition semantics into one oversized phase
- treat the current layout system as if it already promises full flexbox-style or per-frame reflow semantics
- over-optimize for Manim parity when Animatix can provide a clearer declarative workflow
- build richer GUI/editor workflows on top of shifting runtime behavior
- treat Tree-sitter GUI integration as the default next tooling step before the authoring-feedback gap justifies the extra synchronization cost
