# Animatix Colorscheme Implementation Plan

This file now tracks the shipped colorschemes v1 baseline and the remaining rollout work for broader colorscheme support.

The guiding decision is simple:

- **keep the shipped v1 contract honest**
- **preserve explicit actor color semantics throughout**
- **treat future external colorschemes as declarative data, not code**

This plan assumes the current runtime baseline from `docs/spec.md`, `docs/primitives.md`, and `docs/architecture.md`.

---

## 1. Current Baseline

The following are already true today and should remain stable through colorscheme follow-up work:

- scene background color is controlled through `scene.background_color`
- actor colors are controlled through `color`, `stroke`, and `stroke_color`
- explicit color values can be declared directly or assigned later through timeline statements
- frame-local stateless overrides already sit on top of sampled track values
- built-in colorscheme selection now ships through `config { colorscheme: ... }`
- `color`, `stroke`, and deterministic `color: auto` now ship in the runtime contract
- a runnable built-in colorscheme example now exists in `examples/colorscheme_demo.amx`

The colorscheme feature should extend that surface rather than replacing it.

---

## 2. Planning Principles

1. **Preserve today’s mental model.** Explicit `color:` remains the strongest declaration-level signal.
2. **Prefer vertical slices.** Each phase should land with tests, docs, and one focused example where appropriate.
3. **Keep shipped-vs-planned boundaries honest.** Do not widen `docs/spec.md` until runtime behavior and tests exist.
4. **Resolve schemes before frame evaluation.** Colorscheme loading belongs to load/build time, not an ad hoc frame-time side channel.
5. **Broaden the smallest useful thing next.** File-backed sharing comes before optional expression/environment sugar.
6. **Avoid plugin complexity.** External schemes should stay declarative until a proven future need justifies anything stronger.

---

## 3. Remaining Delivery Order

The recommended delivery order is:

1. keep the shipped v1 contract honest
2. external loadable scheme files + inheritance
3. broader examples/docs follow-up
4. optional environment exposure only if it still earns its complexity later

This order gives additional user value without destabilizing the current precedence model.

---

## 4. Shipped Foundation

The following slices are now shipped and should be treated as completed baseline work:

- Phase 0 — contract sync and terminology
- Phase 1 — internal colorscheme model and built-in baseline
- Phase 2 — scene-level scheme selection
- Phase 3 — alias-backed declaration defaults
- Phase 4 — automatic color assignment

That shipped baseline means the remaining roadmap is about broadening the model, not inventing it.

---

## 5. Phase 5 — External Loadable Schemes and Inheritance

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

## 6. Phase 6 — Docs, Examples, and Optional Environment Exposure

**Goal:** Make the broadened feature teachable and decide whether additional expression-surface access is worth exposing.

**Includes:**

- add one focused example for external file-backed scheme usage
- update `docs/spec.md`, `docs/primitives.md`, and `docs/architecture.md` only when the corresponding runtime slices are truly shipped
- optionally expose resolved scheme values into the expression environment under `scheme.*` names if that remains worth the complexity after the file-backed model lands

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

## 7. Diagnostics Matrix

The remaining colorscheme implementation should explicitly cover these failure modes:

1. unknown built-in colorscheme name
2. missing file-backed colorscheme path
3. malformed scheme file
4. invalid color tuple/value
5. missing role token in resolved scheme
6. empty auto-assignment pool when `color: auto` is used
7. colorscheme inheritance cycle

Each should produce a build-facing diagnostic and a deterministic fallback path.

---

## 8. Verification Checklist

Before colorscheme follow-up work is considered landed:

- parser tests pass for new config/properties
- precedence tests cover scheme default vs explicit declaration vs assignment vs frame-local override
- component/nested-label tests cover automatic color stability where relevant
- docs reflect shipped behavior only
- at least one runnable example demonstrates the new surface honestly
- no GUI feature is required to access the runtime behavior

---

## 9. What We Should Not Do Next

- ship plugin/executable themes before declarative file-backed schemes
- make primitive type the main identity for automatic distinct colors
- blur colorscheme alias lookup and explicit `color` precedence
- depend on a property inspector or scene editor before the DSL/runtime slice exists
- broaden into a general style system covering typography, spacing, and motion in the same phase

---

## 10. Recommended Next Commit Shape

1. `test: add failing file-backed colorscheme loading coverage`
2. `feat: add local colorscheme file loading`
3. `feat: add colorscheme inheritance and diagnostics`
4. `examples: add loadable colorscheme demo`
5. `docs: sync broadened colorscheme contract and examples`

This preserves small, reviewable vertical slices.
