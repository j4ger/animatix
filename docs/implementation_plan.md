# Animatix Implementation Plan

This plan starts from the runtime that exists today and reorders the roadmap around **user experience impact first**, then **architectural dependency order**. It is intentionally grounded in the shipped runtime and the current spec so the roadmap does not ask us to re-implement work that already landed.

---

## 1. Shipped Baseline

The following are already part of the current baseline and should not be treated as active roadmap items:

- Core scene primitives: `Text`, `Math`, `Code`, `Svg`, `Image`, `Circle`, `Rect`, `Line`, `Ellipse`, `Arc`, `Polygon`, and `Path`
- Primitive breadth already shipped on top of that baseline: `Dot`, `Square`, `Arrow`, and `RegularPolygon`
- Plotting: `Graph`, `CartesianPlot`, `PolarPlot`, `ParametricPlot`, and `ImplicitPlot`
- Layout/container foundation: `Row`, `Col`, `Grid`, `Stack`, `Group`, root layout defaults, scene-relative placement, and manual child placement within layout containers
- Reactive model: stateless `always`, compile-time `for`, and random-access frame evaluation
- Component MVP: imported `pub component` instantiation, parameter binding, dotted nested-label assignment targets, and rhs sampled property lookup
- Colorschemes v1: built-in scene selection, semantic color roles, `stroke_role`, and deterministic actor-cycle defaults layered on top of explicit color authoring
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

---

## 3. Current UX Assessment

From a user perspective, the most important remaining gaps are no longer the basic timing modifiers. The biggest friction is that common animation intent is still harder to express than it should be.

### Urgent UX gaps

1. **Richer reveal and exit actions**
   - Users need stronger built-in verbs than the current shipped set of `fade-in`, `draw-in`, `wipe-in`, `fade-out`, and `wipe-out`.
   - Reveal-by-drawing, reveal-out, and other lightweight action variants are high-value because they fit explanatory animation directly.

2. **Action-target diagnostics that never pretend unsupported behavior works**
   - As the action surface grows, action/target mismatches must fail honestly.
   - This is part of Animatix's product quality, not just implementation hygiene.

3. **Component and diagnostic contract tightening**
   - Nested-label targeting, reusable authoring patterns, and runtime diagnostics should stay sharper than the growing surface area around them.
   - This now matters more than another broad convenience feature because the shipped runtime already spans components, composition helpers, plots, and colorschemes.

### Useful, but not first

- Colorscheme follow-up work: file-backed schemes, inheritance, and broader reusable scheme sharing
- Better GUI/editor affordances and discovery

Colorscheme follow-up work now means **broadening** the shipped v1 surface rather than inventing it from scratch. The design and rollout docs remain the place for future loadable schemes, inheritance, and broader authoring integration: [`colorscheme_design.md`](colorscheme_design.md) and [`colorscheme_implementation_plan.md`](colorscheme_implementation_plan.md).

### Explicitly later

- Camera framing / pan / zoom systems
- `strategy: fade` and other compositing-heavy transition models
- Hot reload and richer visual editor workflows
- Voiceover/audio-style production tooling

---

## 4. Roadmap Overview

The roadmap is divided into one sync milestone and six implementation phases.

### Milestone 0 — Contract Sync and Plan Cleanup

**Goal:** Ensure the roadmap reflects the actual shipped baseline.

**Why this exists:**
- `docs/spec.md` is ahead of the older roadmap in a few places.
- Planning around stale assumptions causes us to spend time “landing” already-shipped features.

**Includes:**
- align the roadmap with the current shipped timing contract
- treat `delay` as shipped, not as upcoming work
- describe morph modifier support accurately as scoped runtime-real behavior
- keep deferred items visible without implying they are nearly done

**Exit criteria:**
- the roadmap, spec, examples, and current runtime status no longer disagree about the baseline

---

## 5. Phase 1 — Reveal Actions v1 + Honest Action Diagnostics

**Urgency:** Critical

**Status:** effectively shipped. The reveal-actions family now includes `fade-in`, `draw-in`, `wipe-in`, `fade-out`, `wipe-out`, `reveal-out`, and `draw-out`, with focused demo/spec coverage and explicit unsupported-target diagnostics for vector-only reveal verbs.

**Goal:** Make common explanatory reveal/exit animation easier to author without adding a new runtime model.

**Why this is first:**
- It provides immediate user-visible value.
- It reuses the existing action registry, timing modifiers, track system, and diagnostics infrastructure.
- It avoids the layout/transform ambiguity that broader motion ergonomics would introduce too early.

**Includes:**
- add a small set of new built-in reveal/exit actions that lower onto existing track behavior
- keep the existing verb-first action model; do not introduce a new composition syntax yet
- add explicit diagnostics for unsupported action/target combinations so new actions never silently no-op
- ship one focused runnable demo that teaches the new action surface
- update the spec and examples alongside the runtime

**Guardrails:**
- no new global transition/compositing layer
- no camera model changes
- no “mini-language” inside action args or modifiers
- no large action catalog; only ship actions we can support honestly on current actor kinds

**Exit criteria:**
- the new actions are test-backed, documented, and demonstrated in one runnable example
- unsupported action/target combinations report deliberate diagnostics
- the feature feels like a vertical slice, not a collection of undocumented verbs

---

## 6. Phase 2 — Component and Diagnostic Contract Tightening

**Urgency:** High

**Goal:** Sharpen reusable authoring rules and keep failure modes precise as the language/runtime surface grows.

**Why second:**
- The runtime now spans nested label targeting, scoped composition helpers, and colorscheme defaults, so contract sharpness matters more than adding another broad feature family immediately.
- This work compounds the value of the already-shipped surface instead of expanding ambiguity.

**Includes:**
- clearer namespace/reachability rules for nested labels
- stronger diagnostics around ambiguous or unintended component access
- action/property diagnostics that stay honest as the supported surface grows
- better runnable examples for reusable component authoring patterns

**Guardrails:**
- do not widen into custom component actions or a richer runtime object model yet
- do not build editor workflows on top of ambiguous contract edges

**Exit criteria:**
- reusable component authoring is documented and testable without ambiguity
- diagnostics consistently tell the user what is unsupported and why

---

## 7. Phase 3 — Colorscheme Follow-Up: Loadable Schemes and Inheritance

**Urgency:** High

**Goal:** Broaden the shipped colorschemes v1 surface into a reusable project-level theming workflow without changing the explicit-color precedence model.

**Why third:**
- Colorschemes v1 already landed and removed a large amount of palette boilerplate.
- The next meaningful colorscheme value is shareability across projects, but it is safer after the current runtime contract and diagnostics stay honest.

**Includes:**
- file-backed/loadable colorschemes
- scheme inheritance / extension
- diagnostics for missing schemes, invalid files, and missing roles
- docs/examples that distinguish built-in-only v1 from the broader reusable scheme story

**Guardrails:**
- preserve the current precedence stack where explicit `color`, `stroke`, timed assignments, and `always` overrides beat scheme defaults
- do not jump to executable/plugin theming

**Exit criteria:**
- users can reuse colorschemes across projects without copy-pasting palette blocks into every `.amx` file

---

## 8. Phase 4 — Breadth Expansions: Host-Specific Effects and Remaining Practical Surface

**Urgency:** High

**Goal:** Expand capability after the current authoring contract and colorscheme follow-up work are both stable.

**Includes:**
- host-specific effect controls that map cleanly onto real runtime hooks
- any remaining practical primitives/plot helpers that still have clear value after the current shipped breadth
- one focused example and one focused spec section per newly widened surface

**Exit criteria:**
- new breadth features improve authoring range without reintroducing contract ambiguity

---

## 9. Phase 5 — Tooling and Authoring Workflow Refinement

**Urgency:** Medium

**Goal:** Improve discovery, feedback, and day-to-day editing on top of the stabilized runtime contract.

**Includes:**
- continue improving the egui GUI shell
- richer action/component discovery based on the real shipped registries
- bridge `tree-sitter-animatix` into the GUI/editor workflow more completely
- better example/tutorial structure
- keyboard transport shortcuts and other workflow polish

**Guardrail:**
- do not build richer editor workflows on top of ambiguous language/runtime behavior

---

## 11. Deferred Architectural Work

These remain valuable, but they should stay out of the near-term critical path because they imply broader model changes.

- camera framing, pan, zoom, and other viewport-state features
- `strategy: fade` and other compositing-heavy transition models
- hot reload / file watching driven authoring workflows
- scene inspectors, property panels, visual timeline editors, and other larger GUI systems
- native embedded rendering surfaces in the GUI
- multi-file project management UX

These should only move forward once the action/motion authoring surface is stable enough that we are not redesigning the foundation underneath them.

---

## 12. What We Should Not Do Next

- treat parser acceptance as proof of runtime support
- widen the action catalog before defining honest target coverage and diagnostics
- start camera or viewport work before local motion semantics are settled
- mix layout semantics, transform semantics, and composition semantics into one oversized phase
- over-optimize for Manim parity when Animatix can provide a clearer declarative workflow
- build richer GUI/editor workflows on top of shifting runtime behavior

---

## 13. Recommended Near-Term Execution Order

1. **Complete Milestone 0: roadmap/spec contract sync**
2. **Implement Phase 1: reveal actions v1 + honest action diagnostics**
3. **Move immediately to Phase 2: component and diagnostic contract tightening**
4. **Then Phase 3: colorscheme follow-up with loadable schemes and inheritance**
5. **Expand host-specific breadth only after those contracts are stable**
6. **Keep tooling/editor refinement after the runtime surface stops shifting underneath it**

This ordering keeps the roadmap aligned with the current engine and with what users feel most acutely: first make common animation intent and diagnostics more honest, then broaden reusable authoring only after the current contract is sharp enough to support it.
