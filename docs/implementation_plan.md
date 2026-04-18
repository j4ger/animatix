# Animatix Roadmap

This roadmap starts from the runtime that exists today. It is intentionally grounded in the shipped baseline so the repo does not keep planning documents for work that already landed.

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

---

## 3. Recently Completed Work

These slices matter as context, but they are no longer active roadmap items:

- roadmap/spec contract sync so shipped behavior is documented honestly
- reveal actions v1 plus unsupported-target diagnostics
- colorschemes v1 with built-in schemes, alias-backed defaults, and `color: auto`
- motion/composition ergonomics already reflected in the shipped baseline above

---

## 4. Active Roadmap Overview

The active roadmap begins with contract tightening, then moves outward into truthful layout measurement, reusable theming, broader runtime breadth, and tooling refinement.

### Internal architecture note

The repository's internal structural cleanup and primitive-system refactor are tracked separately in `.sisyphus/plans/refactor_roadmap.md`.

That plan is intentionally scoped to internal architecture and refactoring discipline. It should not be treated as a language-surface or shipped-feature roadmap item unless and until it materially changes roadmap sequencing.

### Current priority order

1. component and diagnostic contract tightening
2. size-aware layout for measured children
3. colorscheme follow-up with loadable schemes and inheritance
4. breadth expansions only after those contracts are stable
5. tooling/editor refinement after the runtime surface stops shifting underneath it

---

## 5. Phase 1 — Component and Diagnostic Contract Tightening

**Urgency:** High

**Goal:** Sharpen reusable authoring rules and keep failure modes precise as the language/runtime surface grows.

**Why first:**
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

## 6. Phase 2 — Size-Aware Layout for Measured Children

**Urgency:** High

**Goal:** Make the existing layout containers more truthful by teaching supported children to report real layout size, starting with a narrow `Row` / `Col` measure/place slice.

**Why second:**
- The current layout model already improves authoring, but container placement still depends on incomplete child size reporting.
- This is the smallest honest step toward flex-like ergonomics without committing the runtime to full CSS-style semantics.
- It directly improves both human-authored and AI-authored composition because it reduces reliance on fixed offsets for mixed media/text scenes.

**Includes:**
- define a runtime contract for layout-participating child size
- make `Text`, `Math`, and `Code` report measured local bounds into the size track used by layout
- preserve and clarify the existing intrinsic-size path for `Image`
- keep `Row` / `Col` deterministic with mixed measured-size and authored-size children
- add focused tests and one focused runnable demo for measured-child layout participation

**Guardrails:**
- do not claim full flexbox parity
- do not add `flex-grow`, `flex-shrink`, wrapping, or min/max sizing yet
- do not promise per-frame relayout from animated content or visual-only transforms in this slice
- do not pull in a solver-heavy constraint system

**Exit criteria:**
- supported measured children participate truthfully in `Row` / `Col`
- docs and examples clearly state the supported subset and the remaining gaps
- layout behavior remains compatible with random-access evaluation and current export workflows

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
- diagnostics for missing schemes, invalid files, invalid tuples, inheritance cycles, and unresolved external tokens
- docs/examples that distinguish built-in-only v1 from the broader reusable scheme story
- optional expression-environment exposure only if it still earns its complexity after file-backed loading lands

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

## 10. Deferred Architectural Work

These remain valuable, but they should stay out of the near-term critical path because they imply broader model changes.

- camera framing, pan, zoom, and other viewport-state features
- `strategy: fade` and other compositing-heavy transition models
- hot reload / file watching driven authoring workflows
- scene inspectors, property panels, visual timeline editors, and other larger GUI systems
- native embedded rendering surfaces in the GUI
- multi-file project management UX

These should only move forward once the action/motion authoring surface is stable enough that we are not redesigning the foundation underneath them.

---

## 11. What We Should Not Do Next

- treat parser acceptance as proof of runtime support
- widen the action catalog before defining honest target coverage and diagnostics
- start camera or viewport work before local motion semantics are settled
- mix layout semantics, transform semantics, and composition semantics into one oversized phase
- over-optimize for Manim parity when Animatix can provide a clearer declarative workflow
- build richer GUI/editor workflows on top of shifting runtime behavior
