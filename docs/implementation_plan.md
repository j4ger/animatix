# Animatix Implementation Plan

This plan starts from the runtime that exists today and reorders the roadmap around **user experience impact first**, then **architectural dependency order**. It is intentionally grounded in the shipped runtime and the current spec so the roadmap does not ask us to re-implement work that already landed.

---

## 1. Shipped Baseline

The following are already part of the current baseline and should not be treated as active roadmap items:

- Core scene primitives: `Text`, `Math`, `Code`, `Svg`, `Image`, `Circle`, `Rect`, `Line`, `Ellipse`, `Arc`, `Polygon`, and `Path`
- Plotting: `Graph`, `CartesianPlot`, and `PolarPlot`
- Layout/container foundation: `Row`, `Col`, `Grid`, `Stack`, `Group`, root layout defaults, scene-relative placement, and manual child placement within layout containers
- Reactive model: stateless `always`, compile-time `for`, and random-access frame evaluation
- Component MVP: imported `pub component` instantiation, parameter binding, dotted nested-label assignment targets, and rhs sampled property lookup
- Tooling foundation: CLI renderer, egui-based GUI shell, and `tree-sitter-animatix`
- Shared timing vocabulary already shipped in the runtime contract: duration shorthand, named `delay`, named `ease`, deterministic duplicate-key handling, and explicit instant-change semantics
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
   - Users need stronger built-in verbs than only `fade-in`, `wipe-in`, and `fade-out`.
   - Reveal-by-drawing, reveal-out, and other lightweight action variants are high-value because they fit explanatory animation directly.

2. **Action-target diagnostics that never pretend unsupported behavior works**
   - As the action surface grows, action/target mismatches must fail honestly.
   - This is part of Animatix's product quality, not just implementation hygiene.

3. **Motion ergonomics**
   - Users need easier authored motion such as move, shift, rotate, and scale.
   - These belong soon, but they are broader than reveal actions because they touch transform semantics, layout interaction, and inheritance.

4. **Animation composition ergonomics**
   - Users need simple sequencing, staggering, overlap, and grouped timing.
   - This is important, but it should follow a stronger base action set.

### Useful, but not first

- Additional primitives such as `Arrow`, `Dot`, `Square`, and `RegularPolygon`
- Deeper plotting parity such as `ParametricPlot` and `ImplicitPlot`
- Reusable colorschemes and actor auto-color ergonomics on top of the existing explicit color model
- Better GUI/editor affordances and discovery

Colorscheme work now has its own dedicated design and implementation docs: [`colorscheme_design.md`](colorscheme_design.md) and [`colorscheme_implementation_plan.md`](colorscheme_implementation_plan.md). It should be treated as an authoring-UX enhancement layered on top of the existing property/track system rather than as a replacement for explicit `color:` authoring.

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

## 6. Phase 2 — Motion Ergonomics v1

**Urgency:** High

**Goal:** Add first-class authored motion ergonomics on top of the existing scene graph and track model.

**Why second:**
- This is a major UX need, but it is broader than reveal actions.
- It touches local transforms, inherited transforms, and the relationship between layout placement and authored motion.

**Includes:**
- first-class support for common authored motion such as move / shift / rotate / scale
- clear rules for how local motion composes with layout-managed placement
- docs/examples that teach when to use layout containers versus manual motion

**Guardrails:**
- do not blur layout semantics and local transform semantics
- do not widen into camera or viewport state in this phase

**Exit criteria:**
- authored motion can be expressed directly without awkward property-level workarounds
- layout-managed nodes and manually placed nodes follow one teachable motion contract

---

## 7. Phase 3 — Animation Composition v1

**Urgency:** High

**Goal:** Make timing relationships between multiple animations easier to author.

**Why third:**
- Users need sequencing and staggering, but it is safer to design this after the action and motion surface is more complete.

**Includes:**
- grouped timing helpers such as sequencing, stagger, overlap, or equivalent declarative orchestration
- examples that show multi-object choreography without imperative stateful execution

**Guardrails:**
- preserve the random-access “frame at `t` derives from `t`” model
- do not introduce stateful playback-only constructs

**Exit criteria:**
- users can express common multi-object timing relationships directly and predictably

---

## 8. Phase 4 — Component and Diagnostic Contract Tightening

**Urgency:** High

**Goal:** Sharpen the reusable authoring surface and keep failure modes precise as the language grows.

**Includes:**
- clearer namespace/reachability rules for nested labels
- stronger diagnostics around ambiguous or unintended component access
- better runnable examples for reusable component authoring patterns
- continued tightening of action/property diagnostics where runtime support is intentionally narrow

**Deferred inside this phase:**
- custom component actions remain future-facing until the parameter-driven component model proves insufficient

**Exit criteria:**
- reusable component authoring is documented and testable without ambiguity
- diagnostics consistently tell the user what is unsupported and why

---

## 9. Phase 5 — Breadth Expansions: Primitives, Plots, and Host-Specific Effects

**Urgency:** Medium

**Goal:** Expand capability after the core animation authoring experience is stronger.

**Includes:**
- additional practical primitives such as `Arrow`, `Dot`, `Square`, and `RegularPolygon`
- plotting additions such as `ParametricPlot` and `ImplicitPlot`
- host-specific effect controls that map cleanly onto real runtime hooks

**Guardrails:**
- every new primitive or host-specific key must ship with one focused example, one focused spec section, and direct runtime validation
- do not imply general support from parser acceptance alone

**Exit criteria:**
- new breadth features improve authoring range without reintroducing contract ambiguity

---

## 10. Phase 6 — Tooling and Authoring Workflow Refinement

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
3. **Move to Phase 2: motion ergonomics v1**
4. **Then Phase 3: animation composition v1**
5. **Tighten reusable authoring and diagnostics contracts in Phase 4**
6. **Expand breadth only after the authoring UX is materially better**

This ordering keeps the roadmap aligned with the current engine and with what users feel most acutely: first make common animation intent easier, then broaden the surface deliberately.
