# Animatix Roadmap

This roadmap starts from the runtime that exists today. It is intentionally grounded in the shipped baseline so the repo does not keep planning documents for work that already landed.

This file is the repository's **master planning document** for product/runtime priorities. Related planning docs should support this roadmap rather than compete with it:

- `docs/colorscheme_design.md` is the detailed design document for colorscheme features (both shipped and planned)
- `docs/architecture_refactor_plan.md` tracks the internal refactor/support lane that should reduce delivery risk without redefining roadmap priority

---

## 1. Shipped Baseline

The following are already part of the current baseline and should not be treated as active roadmap items:

- Core scene primitives: `Text`, `Math`, `Code`, `Svg`, `Image`, `Circle`, `Rect`, `Line`, `Ellipse`, `Arc`, `Polygon`, and `Path`
- Primitive breadth already shipped on top of that baseline: `Dot`, `Square`, `Arrow`, and `RegularPolygon`
- Plotting: `Graph`, `CartesianPlot`, `PolarPlot`, `ParametricPlot`, and `ImplicitPlot`
- Layout/container foundation: `Row`, `Col`, `Grid`, `Stack`, `Group`, root layout defaults, scene-relative placement, and manual child placement within layout containers
- Reactive model: stateless `always`, compile-time `for`, and random-access frame evaluation
- Component MVP: imported `pub component` instantiation, parameter binding, dotted nested-label assignment targets, and rhs sampled property lookup
- Module system v1: `pub let` value exports and `import ... as` namespaced imports with qualified access (`alias.export_name`)
- Colorschemes v1: built-in scene selection, semantic color/stroke aliases, deterministic `color: auto` defaults, and inline `Colorscheme` primitive definition with `extends` inheritance
- Colorscheme primitive-type defaults: automatic scheme-appropriate default colors when primitives omit explicit `color` / `stroke`
- Tooling foundation: CLI renderer, egui-based GUI shell, and `tree-sitter-animatix`
- Shared timing vocabulary already shipped in the runtime contract: duration shorthand, named `delay`, named `ease`, deterministic duplicate-key handling, and explicit instant-change semantics
- Reveal actions v1 now shipped in the runtime contract: `fade-in`, `draw-in`, `wipe-in`, `fade-out`, `wipe-out`, `reveal-out`, `draw-out`, plus honest unsupported-target diagnostics for vector-only reveal verbs
- Motion ergonomics already shipped in the current runtime contract: `move`, `shift`, `rotate`, and `scale`
- Composition ergonomics already shipped in scoped form: `sequence` and `stagger` blocks for actions/property assignments with deliberate diagnostics for unsupported contents
- Scoped morph modifier support already shipped for timed path-morphing re-declarations: `strategy: auto|match`, `path_arc`, and `stretch`
- Colorscheme module reuse v1: standard `.amx` modules with `pub let` color exports, imported via `import ... as`, accessed through module-qualified names (e.g., `theme.accent`), demonstrated in `examples/modules.amx`
- Hot reload / file watching in GUI: auto-reloads .amx files when they change on disk
- Path animation / dynamic points: `polygon.points = [(x,y), ...] [duration]` for animating polygon vertices
- Effects actions v1: `shake`, `pulse`, `bounce` for emphasis/attention animations (see `examples/effects_demo.amx`)
- Primitive rotation: `angle` property for `Ellipse`, `Arc`, `RegularPolygon` supporting both visual transform and geometry rotation (see `examples/rotation_demo.amx`)

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

### Internal architecture note

The repository's internal structural cleanup and primitive-system refactor are tracked separately in `docs/architecture_refactor_plan.md`.

That plan is intentionally scoped to internal architecture and refactoring discipline. It should not be treated as a language-surface or shipped-feature roadmap item unless and until it materially changes roadmap sequencing.

### Active supporting design note

The detailed design for colorscheme features lives in `docs/colorscheme_design.md`.

That document should be treated as the implementation design for colorscheme work, not as a separate competing roadmap.

### Current priority order

1. ~~Phase 1~~ — Primitive-Type Default Colors: Shipped
2. ~~Phase 2~~ — Module System Enhancement: `pub let` exports and `import ... as` syntax: Shipped
3. ~~Phase 3~~ — Colorscheme Module Reuse: Standard module-based scheme sharing: Shipped (see `examples/modules.amx`)
4. ~~Phase 4~~ — Breadth Expansions: Effects actions (`shake`, `pulse`, `bounce`) and primitive rotation (`angle` property): Shipped
5. **Phase 5** — Tooling and Authoring Workflow Refinement (active): Hot reload shipped; remaining: diagnostic UX, example/tutorial structure
6. **Phase 6** — Extended Authoring Surface: Remaining practical gaps (coordinate system fixes, text property completion)
7. Tree-sitter GUI integration only after its authoring value justifies the extra synchronization and maintenance cost

---

## 4. Phase 1 — Primitive-Type Default Colors: Automatic scheme-appropriate defaults

**Urgency:** High

**Goal:** Apply scheme-appropriate default colors to primitives when no explicit `color` or `stroke` is authored, dramatically reducing palette boilerplate.

**Why first:**
- The colorscheme infrastructure already exists (built-in schemes, semantic tokens, environment seeding)
- This is a pure runtime change — no parser or module system work needed
- Immediate authoring UX improvement: scenes become dramatically shorter
- Sets the foundation for what "scheme-driven authoring" feels like before adding module reuse

**Includes:**
- Primitive-to-token mapping: text-like → `text.primary`, shapes → `surface.primary`, strokes → `stroke.default`, plots → `accent.primary`
- Track initialization changes: use scheme defaults instead of hardcoded white when property is omitted
- Text/Math/Code declaration changes: default to scheme text color instead of white
- Diagnostics for missing scheme tokens (fallback to white with warning)
- Tests verifying defaults apply and explicit overrides still win
- Example demonstrating minimal-boilerplate scene authoring

**Guardrails:**
- preserve explicit-color precedence: omitted < scheme default < alias < auto < explicit < assignment < always
- if scheme lacks expected token, fall back to current hardcoded default (white) with optional diagnostic
- do not change behavior of scenes without `config.colorscheme` — they keep current hardcoded defaults
- keep changes localized to track initialization and declaration processing

**Exit criteria:**
- primitives without explicit colors receive scheme-appropriate defaults when a scheme is selected
- explicit colors still override defaults completely
- scenes without `config.colorscheme` behave exactly as before
- docs and examples demonstrate the reduced boilerplate

---

## 5. Phase 2 — Module System Enhancement: `pub let` exports and `import ... as` syntax

**Status: Shipped**

**Urgency:** High

**Goal:** Extend the existing module system to support value exports and namespaced imports, enabling reusable data modules (including but not limited to colorschemes).

**Why after Phase 1:**
- Phase 1 delivers immediate authoring value without infrastructure changes
- Phase 2 enables sharing and reuse, building on the authoring patterns established in Phase 1
- The module system (`import`, `pub component`) already provides file-level reuse for components
- `let` declarations already exist for local variables
- Adding `pub let` and `import ... as` is a natural extension of existing patterns

**Includes:**
- `pub let` syntax for exporting named values from `.amx` files
- `import "path" as name` syntax for namespaced imports
- Module namespace binding for qualified access (`name.exported_value`)
- Diagnostics for missing imports, unresolved module paths, and name collisions
- Value export support for: colors, numbers, tuples, strings, and arrays
- docs/examples that demonstrate module-based reuse patterns

**Guardrails:**
- reuse existing `ModuleGraph` file loading and cycle detection
- keep exports declarative and load-time oriented (no runtime mutation of imported values)
- do not introduce a special module file format — reuse standard `.amx` files
- do not make GUI work a dependency for shipping the runtime feature
- maintain backward compatibility with existing `import "path"` (non-namespaced) syntax

**Exit criteria:**
- users can define `pub let` exports in one `.amx` file and access them via `import "path" as name` in another
- module-qualified names resolve correctly in expressions and property values
- invalid imports fail with existing module diagnostics (unknown module, unresolved path)
- docs reflect only the behavior that is actually backed by runtime/tests/examples

---

## 6. Phase 3 — Colorscheme Module Reuse: Standard module-based scheme sharing (SHIPPED)

**Urgency:** High

**Status: Shipped**

Phase 3 shipped with the existing Phase 2 module infrastructure. No additional runtime work was required beyond what Phase 2 already enabled.

**Goal:** Enable colorscheme definition and reuse through the standard module system built in Phase 2.

**Why after Phase 2:**
- Phase 2 is now shipped, enabling `pub let` exports and `import ... as` syntax
- Colorscheme module reuse depends on this Phase 2 infrastructure
- The inline `Colorscheme` primitive already works for ad-hoc definition
- Module-based reuse is the natural next step after the infrastructure exists

**Includes:**
- Colorscheme modules as standard `.amx` files with `pub let` color exports
- Scheme composition via standard `import` and module-qualified access (e.g., `theme.background`, `theme.accent_primary`)
- Optional `pub component` wrapper that pre-binds a scheme to a reusable palette configuration
- Diagnostics for missing imports, unresolved module paths, and invalid color values
- docs/examples that show scheme modules alongside other standard module patterns

**Guardrails:**
- preserve the current precedence stack where explicit `color`, `stroke`, timed assignments, and `always` overrides beat scheme defaults
- keep the model declarative and load-time/build-time oriented
- do not introduce a special `Colorscheme` primitive or separate file format for module schemes — reuse standard modules
- do not make GUI work a dependency for shipping the runtime feature
- maintain full backward compatibility with existing inline `Colorscheme` primitive and built-in schemes

**Exit criteria:**
- users can define a colorscheme in one `.amx` file and import it into scenes via standard module syntax
- scheme colors are accessed through module-qualified names or bound via component parameters
- invalid imports fail with existing module diagnostics (unknown module, unresolved path)
- docs reflect only the behavior that is actually backed by runtime/tests/examples

---

## 7. Phase 4 — Breadth Expansions: Host-Specific Effects and Remaining Practical Surface (SHIPPED)

**Urgency:** High

**Status: Shipped**

**Shipped:**
- Effects actions: `shake`, `pulse`, `bounce` with intensity/frequency modifiers
- Primitive rotation: `angle` property assignment for `Ellipse`, `Arc`, `RegularPolygon`
- Examples: `examples/effects_demo.amx`, `examples/rotation_demo.amx`

**Goal:** Expand capability after the current authoring contract and module system work are both stable.

**Includes:**
- host-specific effect controls that map cleanly onto real runtime hooks
- any remaining practical primitives/plot helpers that still have clear value after the current shipped breadth
- one focused example and one focused spec section per newly widened surface

**Exit criteria:**
- new breadth features improve authoring range without reintroducing contract ambiguity

---

## 8. Phase 5 — Tooling and Authoring Workflow Refinement (ACTIVE)

**Urgency:** Medium

**Goal:** Improve discovery, feedback, and day-to-day editing on top of the stabilized runtime contract.

**Shipped in Phase 5:**
- Hot reload / file watching: Auto-reloads .amx files when they change externally, preserving preview state (see `gui_architecture.md`)
- Path animation: Dynamic points property for Polygon primitives with smooth interpolation

**Remaining Phase 5:**

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

## 9. Deferred Architectural Work

These remain valuable, but they should stay out of the near-term critical path because they imply broader model changes.

- camera framing, pan, zoom, and other viewport-state features
- `strategy: fade` and other compositing-heavy transition models
- ~~sampled relayout / animated-size-triggered container recomputation~~ (now active, see `docs/dynamic_layout_design.md`)
- hot reload / file watching driven authoring workflows
- scene inspectors, property panels, visual timeline editors, and other larger GUI systems
- native embedded rendering surfaces in the GUI
- multi-file project management UX
- Tree-sitter-backed GUI integration beyond the standalone grammar package

These should only move forward once the action/motion authoring surface is stable enough that we are not redesigning the foundation underneath them.

For Tree-sitter specifically, the standalone grammar package remains valuable and shipped, but GUI consumption should stay out of the near-term critical path until a concrete authoring-feedback gap cannot be solved well through parser/runtime diagnostics, examples, or lighter editor feedback.

---

## 10. What We Should Not Do Next

- treat parser acceptance as proof of runtime support
- widen the action catalog before defining honest target coverage and diagnostics
- start camera or viewport work before local motion semantics are settled
- mix layout semantics, transform semantics, and composition semantics into one oversized phase
- treat the current layout system as if it already promises full flexbox-style or per-frame reflow semantics
- over-optimize for Manim parity when Animatix can provide a clearer declarative workflow
- build richer GUI/editor workflows on top of shifting runtime behavior
- treat Tree-sitter GUI integration as the default next tooling step before the authoring-feedback gap justifies the extra synchronization cost
- remove or deprecate the inline `Colorscheme` primitive before module-based alternatives are fully functional
- apply primitive-type defaults before the precedence model is clearly documented and tested
- make default colors depend on complex runtime state or conditional logic
