# Animatix Colorscheme Implementation Plan

This file turns the future-facing colorscheme design into an implementation-ready rollout plan.

The guiding decision is simple:

- **ship a useful built-in baseline first**
- **preserve explicit actor color semantics throughout**
- **treat external colorschemes as declarative data, not code**

This plan assumes the current runtime baseline from `docs/spec.md`, `docs/primitives.md`, and `docs/architecture.md`.

---

## 1. Current Baseline

The following are already true today and should remain stable through the colorscheme rollout:

- scene background color is controlled through `scene.background_color`
- actor colors are controlled through `color`, `stroke`, and `stroke_color`
- explicit color values can be declared directly or assigned later through timeline statements
- frame-local stateless overrides already sit on top of sampled track values
- examples commonly define local palette variables near the top of a file and reuse them manually

The colorscheme feature should extend that surface rather than replacing it.

---

## 2. Planning Principles

1. **Preserve today’s mental model.** Explicit `color:` remains the strongest declaration-level signal.
2. **Prefer vertical slices.** Each phase should land with tests, docs, and one focused example where appropriate.
3. **Keep shipped-vs-planned boundaries honest.** Do not widen `docs/spec.md` until runtime behavior and tests exist.
4. **Resolve schemes before frame evaluation.** Colorscheme loading belongs to load/build time, not an ad hoc frame-time side channel.
5. **Start with the smallest useful product.** Built-in schemes and role defaults come before file-backed sharing.
6. **Avoid plugin complexity.** External schemes should stay declarative until a proven future need justifies anything stronger.

---

## 3. Proposed Delivery Order

The recommended delivery order is:

1. contract and terminology sync
2. internal colorscheme data model + one built-in scheme
3. scene-level scheme selection
4. role-based declaration defaults
5. actor-cycle auto assignment
6. external loadable scheme files + inheritance
7. examples, docs, and optional environment exposure follow-up

This order gives immediate user value while containing risk.

---

## 4. Phase 0 — Contract Sync and Terminology

**Goal:** Define one canonical vocabulary before code changes begin.

**Includes:**

- document the difference between explicit color values, role-based defaults, timed assignments, and frame-local overrides
- confirm that colorscheme selection is a load-time concern, not a new animated property
- settle the initial naming surface (`colorscheme`, `color_role`, `stroke_role`, `actor` role)

**Tests first:**

- no runtime tests required yet
- design and implementation docs land first

**Exit criteria:**

- the docs use one clear precedence vocabulary
- no part of the plan depends on ambiguous override rules

---

## 5. Phase 1 — Internal Colorscheme Model and Built-In Baseline

**Goal:** Introduce internal colorscheme structures without changing the DSL surface yet.

**Includes:**

- add internal `Colorscheme` / `ResolvedColorscheme` data structures
- define semantic token storage and actor-cycle storage
- add one built-in baseline such as `default-dark`
- define the in-memory representation shared by built-in and future external schemes

**Tests first:**

- unit tests for scheme resolution and fallback defaults
- table-driven tests for token lookup and actor-cycle presence

**Guardrails:**

- no file loading yet
- no GUI work
- no parser changes beyond what the next phase requires

**Exit criteria:**

- built-in schemes can be resolved in memory deterministically
- unresolved token lookups have defined fallback behavior

---

## 6. Phase 2 — Scene-Level Scheme Selection

**Goal:** Let a document select a built-in colorscheme through `config`.

**Includes:**

- add `colorscheme` as a planned/implemented `config` setting at the parser/runtime level when ready
- resolve built-in scheme names during load/build
- seed `scene.background_color` from `scene.background` only when the scene omits an explicit background

**Tests first:**

- parser tests for `config { colorscheme: ... }`
- build/timeline tests proving selected scheme background is applied only when explicit background is absent
- diagnostics tests for unknown built-in scheme names

**Guardrails:**

- do not treat `colorscheme` as a timed property
- do not override explicit `scene.background_color`

**Exit criteria:**

- a document can select a built-in scheme by name
- explicit authored background still wins

---

## 7. Phase 3 — Role-Based Declaration Defaults

**Goal:** Let declarations opt into scheme-driven defaults without changing explicit color semantics.

**Includes:**

- add `color_role`
- add `stroke_role`
- resolve semantic tokens such as `text.primary`, `surface.primary`, and `accent.warning`
- seed declaration-time track defaults from those roles when explicit color values are omitted

**Tests first:**

- parser tests for role properties
- timeline tests proving `color_role` seeds a declaration default
- precedence tests proving explicit `color` / `stroke` win over role-based defaults
- diagnostics tests for unknown role tokens

**Guardrails:**

- explicit `color`, `stroke`, and `stroke_color` remain stronger than role properties
- do not widen this phase into actor auto-assignment yet

**Exit criteria:**

- authors can use semantic roles for common scene text/surface/accent usage
- precedence between role and explicit value is test-backed and documented

---

## 8. Phase 4 — Actor-Cycle Auto Assignment

**Goal:** Support deterministic distinct colors for actor-like nodes through explicit opt-in.

**Includes:**

- add `color_role: actor`
- assign actor-cycle colors deterministically by final actor identity/path
- keep cycle-wrap behavior deterministic and documented

**Tests first:**

- timeline tests for stable assignment by labeled actor path
- component-expansion tests for prefixed nested-label stability
- tests covering anonymous-node deterministic behavior where supported
- precedence tests proving explicit `color` and later assignments still win

**Guardrails:**

- do not introduce primitive-type auto-coloring
- do not make unlabeled reorder-sensitive behavior look more stable than it is

**Exit criteria:**

- actor-role declarations receive distinct deterministic colors
- explicit overrides still behave exactly as users expect

---

## 9. Phase 5 — External Loadable Schemes and Inheritance

**Goal:** Make schemes shareable across projects through declarative external files.

**Includes:**

- load file-backed schemes from the selected `config.colorscheme` path
- add `extends`
- resolve inheritance into the same `ResolvedColorscheme` used by built-ins
- add cycle detection and invalid-file diagnostics

**Tests first:**

- loader tests for path resolution success/failure
- inheritance-resolution tests
- cycle tests
- invalid-data diagnostics tests
- integration tests showing a file-backed scheme behaves identically to a built-in resolved scheme

**Guardrails:**

- no remote loading
- no executable scripts/plugins
- no partial success that leaves scene state half themed and half defaulted without diagnostics

**Exit criteria:**

- users can reference a local declarative scheme file
- invalid loads fail honestly and fall back safely

---

## 10. Phase 6 — Docs, Examples, and Optional Environment Exposure

**Goal:** Make the feature teachable and decide whether additional expression-surface access is worth exposing.

**Includes:**

- add one focused example for built-in scheme usage
- add one focused example for actor-cycle usage
- add one focused example for external file-backed scheme usage
- update `docs/spec.md`, `docs/primitives.md`, and `docs/architecture.md` only when the corresponding runtime slices are truly shipped
- optionally expose resolved scheme values into the expression environment under `scheme.*` names if that remains worth the complexity after the core feature lands

**Tests first:**

- example-based smoke coverage where practical
- tests for environment lookup only if the optional exposure lands

**Guardrails:**

- do not widen the expression surface before the core role/defaulting model is stable
- do not make GUI work a dependency for shipping the runtime feature

**Exit criteria:**

- users can discover the feature from docs and examples alone
- shipped-vs-planned documentation remains accurate

---

## 11. Diagnostics Matrix

The implementation should explicitly cover these failure modes:

1. unknown built-in colorscheme name
2. missing file-backed colorscheme path
3. malformed scheme file
4. invalid color tuple/value
5. missing role token in resolved scheme
6. empty actor cycle when `color_role: actor` is used
7. colorscheme inheritance cycle

Each should produce a build-facing diagnostic and a deterministic fallback path.

---

## 12. Verification Checklist

Before the colorscheme work is considered landed:

- parser tests pass for new config/properties
- precedence tests cover scheme default vs explicit declaration vs assignment vs frame-local override
- component/nested-label tests cover actor-cycle stability where relevant
- docs reflect shipped behavior only
- at least one runnable example demonstrates the new surface honestly
- no GUI feature is required to access the runtime behavior

---

## 13. What We Should Not Do First

- ship plugin/executable themes before declarative file-backed schemes
- make primitive type the main identity for automatic distinct colors
- blur `color_role` and explicit `color` precedence
- depend on a property inspector or scene editor before the DSL/runtime slice exists
- broaden into a general style system covering typography, spacing, and motion in the same phase

---

## 14. Recommended Initial Commit Shape

1. `docs: add colorscheme design and implementation plan`
2. `test: add failing colorscheme config and precedence coverage`
3. `feat: add built-in colorscheme model and scene selection`
4. `feat: add color_role and stroke_role resolution`
5. `feat: add actor-cycle automatic assignment`
6. `feat: add file-backed colorscheme loading and inheritance`
7. `docs: sync shipped colorscheme contract and examples`

This preserves small, reviewable vertical slices.
