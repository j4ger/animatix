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

The active roadmap now begins with **layout contract honesty and narrowed container-layout guidance**. The highest-value near-term work is to keep runtime behavior, docs, examples, and tests aligned around the current shipped declaration-time measure/place model before any broader surface expansion.

### Internal architecture note

The repository's internal structural cleanup and primitive-system refactor are tracked separately in `.sisyphus/plans/refactor_roadmap.md`.

That plan is intentionally scoped to internal architecture and refactoring discipline. It should not be treated as a language-surface or shipped-feature roadmap item unless and until it materially changes roadmap sequencing.

### Current priority order

1. layout contract honesty and narrowed container-layout guidance
2. diagnostic UX and contract-surface feedback
3. colorscheme follow-up with loadable schemes and inheritance
4. breadth expansions only after those contracts are stable
5. lower-maintenance tooling/editor refinement that improves feedback without creating a second syntax authority
6. Tree-sitter GUI integration only after its authoring value justifies the extra synchronization and maintenance cost

---

## 5. Phase 1 — Layout Contract Honesty and Narrowed Container Layout

**Urgency:** High

**Goal:** Make the shipped layout model explicit, narrow, and trustworthy: declaration-time measure/place, parent-driven container placement, explicit manual opt-out, and no accidental implication of full flexbox or sampled reflow semantics.

**Why first:**
- The current runtime already ships `Row`, `Col`, `Grid`, `Stack`, root layout defaults, scene-relative placement, and manual child opt-out, but some docs still read like a broader flexbox roadmap than the runtime actually supports.
- The highest-friction layout issues now come more from contract drift than from missing vocabulary.
- Narrowing the layout story before widening it preserves user trust and prevents future docs/examples from teaching semantics the runtime does not own.

**Includes:**
- align `docs/layout_design.md`, `docs/spec.md`, `docs/primitives.md`, `docs/architecture.md`, and runnable examples around one canonical phrase: **declaration-time measure/place contract**
- make clear that containers are a deterministic composition scaffold, not a promise of full CSS-style flex behavior
- document per-container supported semantics honestly: `Row` / `Col` own `gap` + cross-axis `align`; `Grid` is deterministic `cols` + `gap`; `Stack` is shared-origin overlap; manual child placement is an explicit opt-out
- identify and document which primitives currently provide truthful layout size and which still remain narrower or less battle-tested participants
- tighten tests/examples/docs so animated transforms such as visual-only `scale` are not described as implying sibling reflow

**Guardrails:**
- do not widen the layout model into `flex-grow`, `flex-shrink`, wrapping, min/max sizing, or solver-heavy constraints
- do not promise per-frame relayout from animated content, visibility changes, or visual-only transforms in this phase
- do not let examples or design prose imply a broader layout runtime than the shipped one

**Exit criteria:**
- the major docs describe the same bounded layout contract without conflicting future-facing language
- examples teach layout-first composition without implying sampled relayout or full flexbox parity
- the roadmap clearly treats layout contract honesty as the active top priority rather than as a side note to feature expansion

---

## 6. Phase 2 — Diagnostic UX and Contract-Surface Feedback

**Urgency:** High

**Goal:** Sharpen reusable authoring rules and make failure modes clearer, more actionable, and more visible as the language/runtime surface grows.

**Why second:**
- Once the shipped layout contract is stated honestly, better diagnostics become the next lever for trust and day-to-day authoring quality.
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
- sampled relayout / animated-size-triggered container recomputation beyond the current declaration-time measure/place contract
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
- treat the current layout system as if it already promises full flexbox-style or per-frame reflow semantics
- over-optimize for Manim parity when Animatix can provide a clearer declarative workflow
- build richer GUI/editor workflows on top of shifting runtime behavior
- treat Tree-sitter GUI integration as the default next tooling step before diagnostic UX and contract-surface feedback are measurably better
