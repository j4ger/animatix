# Animatix Implementation Plan

This plan is grounded in the runtime that exists today. It removes already-shipped foundation work from the active roadmap and focuses on the gaps between the current implementation and the intended language/design surface.

---

## 1. Current Shipped Baseline

The following should be treated as already landed foundations, not future roadmap bullets:

- Core scene primitives: `Text`, `Math`, `Code`, `Svg`, `Image`, `Circle`, `Rect`, `Line`, `Ellipse`, `Arc`, `Polygon`, and `Path`
- Plotting: `Graph`, `CartesianPlot`, `PolarPlot`
- Layout/container foundation: `Row`, `Col`, `Grid`, `Stack`, `Group`, root layout defaults, scene-relative placement, and manual child placement within layout containers
- Reactive model: stateless `always`, compile-time `for`, and random-access frame evaluation
- Component MVP: imported `pub component` instantiation, parameter binding, dotted nested-label assignment targets, and rhs sampled property lookup
- Tooling foundation: CLI renderer, egui-based GUI shell, and `tree-sitter-animatix`

The roadmap below starts from that baseline.

---

## 2. Planning Principles

1. **Close parser/runtime mismatch before widening syntax.**
2. **Prefer vertical slices.** A roadmap item is not done until runtime, docs, examples, and tests agree.
3. **Keep current guarantees smaller than parser acceptance.** The spec should never imply that generic parser acceptance means runtime support.
4. **Preserve random-access semantics.** New modifier and animation features must remain compatible with preview, scrubbing, image export, and video export.
5. **Keep future-facing ideas visible but clearly deferred.** Planned syntax should not leak into the active contract until it is runnable.

---

## 3. Primary Gap: Bracket Modifier System

### Target Design

Square brackets should converge on a **typed declarative modifier bag**:

- one universal shorthand: bare time literal means duration
- a small shared timing vocabulary: `duration` and `ease` first
- host-specific keys only where explicitly supported
- strict per-host validation after parsing
- no silent acceptance of keys that do nothing

### Current Status

What is already real today:

- built-in actions support positional duration plus named `ease`
- property assignments support positional duration plus named `ease`
- `Text`, `Math`, and `Code` declarations support positional duration plus named `ease`

What is still mismatched:

- actor re-declarations parse bracket modifiers but do not handle duration consistently
- inline actor items inherit the same actor-declaration mismatch
- `delay` is documented as a concept but is not implemented
- morph-specific bracket keys such as `strategy`, `path_arc`, and `stretch` remain planned only
- modifier parsing/normalization logic is duplicated across multiple runtime sites
- unknown modifier keys are generally ignored instead of validated

### Phase 1 — Modifier Contract Alignment

**Goal:** Make the modifier system honest, consistent, and teachable before adding new modifier features.

**Includes:**
- centralize modifier parsing/normalization logic for the shared timing vocabulary
- align actor re-declaration handling with the rest of the runtime for duration + `ease`
- make parser/runtime/docs/examples agree on what is currently supported
- add explicit validation behavior for unsupported keys per host

**Exit criteria:**
- `duration` shorthand and named `ease` behave consistently across all currently-supported modifier hosts
- the spec can describe one shipped timing subset without caveats between actions, assignments, and declarations
- unsupported modifier keys fail or are reported deliberately rather than disappearing silently

### Phase 2 — Shared Timing Vocabulary Expansion

**Goal:** Add a small, explicit next layer of timing semantics without turning brackets into an open-ended mini-language.

**Candidate work:**
- `delay`
- duplicate-key conflict rules
- explicit zero-duration / instant semantics across hosts
- consistent defaulting rules when only `ease` is provided

**Guardrails:**
- do not add broad arbitrary named-key support just because the parser can carry it
- keep new keys compatible with stateless evaluation and track building

**Exit criteria:**
- every shipped timing key has parser tests, runtime tests, one runnable example, and one concise spec explanation

### Phase 3 — Host-Specific Modifier Extensions

**Goal:** Introduce richer bracket keys only where the host actually needs them.

**Deferred candidates:**
- morph controls: `strategy`, `path_arc`, `stretch`
- path/stroke effect controls such as trimming or dashing
- future action-specific keys beyond the shared timing bag

**Guardrails:**
- each host-specific key must name a real runtime hook
- do not let morph-specific controls become implied global modifier keys
- keep runtime support narrower than parser possibility unless validation is in place

---

## 4. Secondary Gap: Component Runtime Refinement

### Current Status

The component MVP is real and useful, but its user-facing contract still needs sharpening.

Already shipped:
- cross-file `pub component` loading and instantiation
- parameter binding
- nested-label isolation per instance
- dotted writes and sampled rhs reads through expanded labels

Still remaining:
- clearer namespace/reachability rules for nested labels
- stronger error behavior around ambiguous or unintended external access
- better runnable examples for reusable component authoring patterns
- custom component actions remain future-facing only

### Active Plan

1. **Lock the current contract more explicitly** in docs/tests/examples.
2. **Clarify reachability and naming rules** before expanding syntax.
3. **Keep custom component actions deferred** until the current parameter-driven model proves insufficient.

**Exit criteria:**
- component rules are precise enough to support docs, tests, and tooling without ambiguity
- the spec describes the shipped component subset without parser-only wording for already-landed behavior

---

## 5. Secondary Gap: Advanced Plotting and Morph Controls

These remain valuable, but they should come after the modifier contract is honest.

### Candidate work

- `ParametricPlot`
- `ImplicitPlot`
- morph strategy controls exposed through bracket modifiers
- higher-level path/stroke effect controls

### Why this is later

- the parser/runtime mismatch around current modifier semantics is more harmful than the absence of these features
- these features will be easier to ship cleanly after the bracket modifier contract is stabilized

**Exit criteria:**
- each feature ships with one focused demo, one focused spec section, and host-specific validation rules

---

## 6. Tooling and Authoring UX

The GUI and editor story are no longer “build from zero” work. The next work here should refine the existing shell, not outrun the language contract.

### Candidate work

- continue improving the egui GUI shell
- richer action/component discovery based on shipped registries
- eventual hot reload / file watching
- better example/tutorial structure

### Guardrail

Do not build a richer visual/editor workflow on top of misleading or inconsistent language semantics.

---

## 7. What We Should Not Do Next

- Add more aspirational modifier keys to the spec before the current timing subset is consistent
- Treat parser acceptance as proof of runtime support
- Expand morph syntax aggressively before actor re-declaration timing semantics are fixed
- Push toward a full visual editor while the user-facing contract is still shifting underneath it

---

## 8. Recommended Near-Term Execution Order

1. **Finish modifier contract alignment**
   - centralize timing-modifier handling
   - fix actor re-declaration duration handling
   - make unsupported keys explicit

2. **Tighten the component contract**
   - clearer namespace/reachability rules
   - better examples and failure semantics

3. **Then expand the language again**
   - `delay`
   - morph-specific keys
   - advanced plotting additions

This keeps the roadmap aligned with the current engine: stabilize the semantics users can already see, then widen the surface deliberately.
