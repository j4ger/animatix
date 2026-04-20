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
7. **Improve feedback loops before richer editor infrastructure.** Diagnostic UX, unsupported-surface explanations, and contract clarity should improve before the GUI depends on another syntax integration layer.

---

## 3. Recently Completed Work

These slices matter as context, but they are no longer active roadmap items:

- roadmap/spec contract sync so shipped behavior is documented honestly
- reveal actions v1 plus unsupported-target diagnostics
- colorschemes v1 with built-in schemes, alias-backed defaults, and `color: auto`
- motion/composition ergonomics already reflected in the shipped baseline above

---

## 4. Active Roadmap Overview

The active roadmap begins with diagnostic UX and contract-surface feedback, then moves outward into truthful layout measurement, reusable theming, broader runtime breadth, and only then higher-maintenance tooling/editor integration.

### Internal architecture note

The repository's internal structural cleanup and primitive-system refactor are tracked separately in `.sisyphus/plans/refactor_roadmap.md`.

That plan is intentionally scoped to internal architecture and refactoring discipline. It should not be treated as a language-surface or shipped-feature roadmap item unless and until it materially changes roadmap sequencing.

### Current priority order

1. diagnostic UX and contract-surface feedback
2. size-aware layout for measured children
3. colorscheme follow-up with loadable schemes and inheritance
4. breadth expansions only after those contracts are stable
5. lower-maintenance tooling/editor refinement that improves feedback without creating a second syntax authority
6. Tree-sitter GUI integration only after its authoring value justifies the extra synchronization and maintenance cost

---

## 5. Phase 1 — Diagnostic UX and Contract-Surface Feedback

**Urgency:** High

**Goal:** Sharpen reusable authoring rules and make failure modes clearer, more actionable, and more visible as the language/runtime surface grows.

**Why first:**
- The runtime now spans nested label targeting, scoped composition helpers, and colorscheme defaults, so contract sharpness matters more than adding another broad feature family immediately.
- This work compounds the value of the already-shipped surface instead of expanding ambiguity.
- Better user-facing feedback is the shortest path to a more trustworthy GUI/editor experience; it should land before heavier syntax-integration work.

**Includes:**
- clearer namespace/reachability rules for nested labels
- stronger diagnostics around ambiguous or unintended component access
- action/property diagnostics that stay honest as the supported surface grows
- documentation that distinguishes parser acceptance, runtime support, and explicitly deferred surface area
- better runnable examples for reusable component authoring patterns

**Guardrails:**
- do not widen into custom component actions or a richer runtime object model yet
- do not build editor workflows on top of ambiguous contract edges
- do not make Tree-sitter GUI integration a prerequisite for better diagnostics or clearer feedback

**Exit criteria:**
- reusable component authoring is documented and testable without ambiguity
- diagnostics consistently tell the user what is unsupported, why it is unsupported, and which contract boundary was crossed
- examples cover both a valid path and an intentionally unsupported path for the revised contract surface

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

**Follow-on: animated-size-triggered reflow**

After the current size-aware layout slice is stable, the next layout-specific architectural question is whether some animated size changes should trigger container relayout instead of staying declaration-time only.

**Why later:**
- it depends on the size-reporting contract above staying explicit and well-tested first
- it is a separate architectural step from the current declaration-time measure/place model
- it needs a truthful boundary between layout-affecting size changes and visual-only transforms such as the current `scale`

**Would include:**
- define which animated size changes are allowed to trigger relayout for supported children
- evaluate whether relayout should be sampled per frame, per keyframe boundary, or through a narrower deterministic recomputation model compatible with random-access evaluation
- keep diagnostics and docs honest about which primitives can trigger relayout and which still do not
- preserve an explicit distinction between visual-only transforms and layout-affecting size changes

**Guardrails:**
- do not quietly widen the current Phase 2 contract to imply animated relayout before it is implemented
- do not conflate visual-only `scale` with layout-triggering size changes unless the runtime contract says so explicitly
- do not turn this into full flexbox parity, a solver-heavy system, or global responsive reflow

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
- better diagnostic UX in the GUI: clearer summary surfaces, more actionable contract feedback, and stronger visibility for parse/build/runtime mismatches
- richer action/component discovery based on the real shipped registries
- better example/tutorial structure
- keyboard transport shortcuts and other workflow polish
- use the lowest-maintenance editor feedback path that still reflects the real parser/runtime contract

**Guardrails:**
- do not build richer editor workflows on top of ambiguous language/runtime behavior
- do not introduce a second syntax-maintenance loop unless it clearly improves authoring feedback beyond simpler diagnostic/UI work

---

## 10. Deferred Architectural Work

These remain valuable, but they should stay out of the near-term critical path because they imply broader model changes.

- camera framing, pan, zoom, and other viewport-state features
- `strategy: fade` and other compositing-heavy transition models
- hot reload / file watching driven authoring workflows
- scene inspectors, property panels, visual timeline editors, and other larger GUI systems
- native embedded rendering surfaces in the GUI
- multi-file project management UX
- Tree-sitter-backed GUI integration beyond the standalone grammar package

These should only move forward once the action/motion authoring surface is stable enough that we are not redesigning the foundation underneath them.

For Tree-sitter specifically, the standalone grammar package remains valuable and shipped, but GUI consumption should stay out of the near-term critical path until a concrete authoring-feedback gap cannot be solved well through parser/runtime diagnostics, examples, or lighter editor feedback.

---

## 11. What We Should Not Do Next

- treat parser acceptance as proof of runtime support
- widen the action catalog before defining honest target coverage and diagnostics
- start camera or viewport work before local motion semantics are settled
- mix layout semantics, transform semantics, and composition semantics into one oversized phase
- over-optimize for Manim parity when Animatix can provide a clearer declarative workflow
- build richer GUI/editor workflows on top of shifting runtime behavior
- treat Tree-sitter GUI integration as the default next tooling step before diagnostic UX and contract-surface feedback are measurably better
