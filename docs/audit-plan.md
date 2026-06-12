# Animatix Language Audit — Fix Plan

> Generated from audit review on 2026-06-11. See [`spec.md`](spec.md),
> [`architecture.md`](architecture.md), [`primitives.md`](primitives.md), and
> [`properties.md`](properties.md) for current design.

---

## H1. `at`/`position` vs `transform` in Managed Layouts

**Design intent:** `at` is for absolute positioning (forbidden for managed-layout
children). `transform` is universal and can be applied to managed-layout children
without interrupting the container's layout.

### Implementation check findings

- `at` / `position` currently opt actors into `PlacementMode::Manual` via
  `crates/animatix/src/timeline/build/actor.rs:464`,
  `crates/animatix/src/timeline/build/actor.rs:494`, and
  `crates/animatix/src/timeline/assignments.rs:142`.
- `transform` is already universal in
  `crates/animatix/src/timeline/property_registry.rs:580` and applied after
  position in `crates/animatix/src/timeline/scene_eval.rs:129`.

### Plan

1. **Add diagnostic code** `AbsolutePositionOnLayoutManagedChild` in
   `crates/animatix-syntax/src/diagnostics.rs:46`, with display string
   `absolute-position-on-layout-managed-child`.

2. **Add helper** `Timeline::is_layout_managed_child(parent_label)` near
   `crates/animatix/src/timeline/build/node.rs:6` or
   `crates/animatix/src/timeline/layout.rs:31`; return `true` only for
   `Row`/`Col`/`Grid`/`Stack` parents, not `Group`/`Mask`/`Filter`/`Graph`.

3. **Declarations:** Before
   `resolve_position_binding_with_lookup_diagnostic()` in
   `crates/animatix/src/timeline/build/actor.rs:464`, detect `at` or `position`
   on layout-managed children, emit warning, and ignore the position binding so
   `mark_track_manual_position()` at `build/actor.rs:494` is not called.

4. **Assignments:** Before the `position`/`at` special case in
   `crates/animatix/src/timeline/assignments.rs:142`, reject or warn+ignore
   `child.at = ...` and `child.position = ...` when the target is a managed
   child.

5. **Keep `transform` untouched;** add a regression test proving `transform`
   changes render transform but leaves `PlacementMode::LayoutManaged`.

6. **Docs:** Update `docs/spec.md:409`, `docs/spec.md:474`,
   `docs/architecture.md:113`, and `docs/primitives.md:327` to say managed
   children use layout position; use `transform`, `shift`, `rotation`, or
   `scale` for visual offsets.

### Risks

- Existing examples or GUI drag behavior may rely on `at` as a manual escape
  hatch inside layout containers.
- `Graph` children intentionally use `at` for math-coordinate mapping; the
  helper must not treat `Graph` as a managed layout parent.
- If `position` remains allowed while `at` is forbidden, users still have a
  bypass; handle both consistently.

### Dependencies

- None, but docs overlap with L2.
- Tests should account for reorder actions, which require
  `PlacementMode::LayoutManaged`.

### Test Strategy

- Add timeline tests in `crates/animatix/src/timeline/tests.rs` or
  `crates/animatix/src/timeline/actions/motion.rs`.
- Verify: `cargo test -p animatix h1_layout_at`.
- Run broader: `cargo test -p animatix`.

### Effort

**Medium**

---

## H2. Drop `Math`, Keep `Typst`

**Design intent:** Remove the `Math` primitive entirely. It already compiles
through the Typst engine. `Typst` (with `content` property) replaces it.

### Implementation check findings

- `Math` is still a full primitive in
  `crates/animatix/src/primitives/mod.rs:178` and
  `crates/animatix/src/primitives/mod.rs:579`.
- `ActorKindId::Math` exists in `crates/animatix/src/timeline/track.rs:63` and
  has many `match` arms across the codebase.
- The Typst primitive (`primitives/typst.rs`) is fully functional with
  `content` property.
- The spec already includes a "Typst vs LaTeX cheat sheet" under the `Math`
  section, highlighting the contradiction.

### Plan

1. **Add `content`** as a real Typst property in
   `crates/animatix/src/timeline/property_registry.rs:526`; make it
   `ASSIGNABLE_A`, applicable to `ActorKindId::Typst`.

2. **Normalize deprecated `Math` declarations** to `Typst` in
   `crates/animatix/src/timeline/build/actor.rs:163` and
   `crates/animatix/src/timeline/declarations_text.rs:363`; map
   `math`/`latex`/`text` to `content`.

3. **Remove `mod math` and `&MATH`** from
   `crates/animatix/src/primitives/mod.rs:178` and at
   `crates/animatix/src/primitives/mod.rs:579`.

4. **Remove `ActorKindId::Math`** from
   `crates/animatix/src/timeline/track.rs:63`; update every match in
   `track.rs`, `declarations_text.rs`, `property_registry.rs`, tests, and
   `recompile_text_at_assignment()` in
   `crates/animatix/src/timeline/assignments.rs:352`.

5. **Add deprecation diagnostic** (e.g. `DeprecatedPrimitiveAlias`) in
   `crates/animatix-syntax/src/diagnostics.rs:46` when source uses `Math`.

6. **Update analyzer** builtins in
   `crates/animatix-analyzer/src/symbol_table.rs:143`, hover in
   `crates/animatix-analyzer/src/hover.rs:163`, and completion docs in
   `crates/animatix-analyzer/src/completer.rs:314`.

7. **Update examples** using `Math` to `Typst, content:`:
   `examples/01_shapes.amx:8`, plus syntax fixtures in
   `crates/animatix-syntax/src/to_source.rs:163`.

8. **Update docs** in `docs/spec.md:13`, `docs/spec.md:1037`,
   `docs/primitives.md:27`, and `docs/properties.md:75`.

### Risks

- Removing `ActorKindId::Math` is invasive — touches serialization, analyzer,
  docs, tests, and examples.
- Backward compatibility requires `Math` to parse/build as an alias for at
  least one release.
- `compile_math()` wraps content as Typst math, while `compile_typst()`
  compiles full Typst markup; alias behavior must be explicitly chosen.

### Dependencies

- L2 docs cleanup should happen after this.
- Analyzer/LSP tests must be updated with runtime changes.

### Test Strategy

- Add parser/build test: `Math, math: "x^2"` builds as `ActorKindId::Typst`
  and emits a deprecation warning.
- Add assignment test: `old.math = "x"` maps to `content`.
- Verify: `cargo test -p animatix`, `cargo test -p animatix-analyzer`.

### Effort

**Large**

---

## H3. Implement Bounds-Based `Mask`

**Design intent:** `Mask` is a container that clips its children to the mask
actor's own bounds (rectangle or ellipse). It is not a layout container.

### Implementation check findings

- `Mask` currently clips later children to the **first child's vector paths**
  in `crates/animatix/src/timeline/scene_eval.rs:639`, which conflicts with the
  requested bounds-based container model.
- `MaskPrimitive::build()` is a no-op ("Build handled by legacy dispatch") in
  `crates/animatix/src/primitives/mask.rs:22`.
- `MaskPrimitive::evaluate()` returns `vec![]` (empty commands).

### Plan

1. **Replace first-child clipping** in
   `crates/animatix/src/timeline/scene_eval.rs:639` with mask-actor bounds
   clipping.

2. **Build the clip path** from the Mask actor's sampled `size` and local
   transform: default rectangle in local coordinates, pushed with
   `scene.push_layer()` using the mask's `global_transform`.

3. **Add optional `clip_shape` property** in
   `crates/animatix/src/timeline/property_registry.rs:526`, applicable only to
   `ActorKindId::Mask`, initially supporting `"rect"` and `"ellipse"`.

4. **Wire `MaskPrimitive::build()`** in
   `crates/animatix/src/primitives/mask.rs:22` to call normal container
   processing, or remove the misleading "legacy dispatch" comment since generic
   `process_actor_decl()` already handles it.

5. **Ensure `PrimitiveDescriptor::for_actor_type("Mask")`** in
   `crates/animatix/src/timeline/primitive.rs:32` remains non-layout.

6. **Document Mask** in `docs/spec.md:558`, `docs/primitives.md:327`, and
   `docs/architecture.md:189`.

### Risks

- Vello layer transforms are easy to get wrong; clip path must be in the
  correct coordinate space.
- Nested Mask + Filter ordering may produce unexpected layer/composite
  behavior.
- Hit regions may include clipped-away children unless hit testing also
  respects masks.

### Dependencies

- None, but H1's parent-layout helper should not classify Mask as
  layout-managed.

### Test Strategy

- Add build/eval test proving a Mask with no first child still clips children
  to its own bounds.
- Add rendering smoke test if offscreen renderer is stable in CI; otherwise
  add a targeted `Scene` layer-count/path test.
- Verify: `cargo test -p animatix mask`.

### Effort

**Medium to large**

---

## H4. Warn When `always` Overrides Keyframed Properties — ✅ DONE

**Completed 2026-06-11:**
- Added `DiagnosticCode::AlwaysOverridesKeyframes` in `diagnostics.rs`
- Added `AnimationTrack::has_keyframes_for()` helper in `track.rs`
- Added warning loop in `build/entry.rs` after modifier compilation
- Added 3 tests (positive + 2 negatives) in `tests.rs`

### Known limitation

- Only detects direct `Stmt::Assignment` in modifier bodies.
- Does not recursively inspect conditionals or `for` loops.
- Does not handle `scene.background_color` as a special case.

---

## H5. Degree Support and Math Constants — Partially Done

**Completed 2026-06-11 (M5 + this):**
- ✅ Lowercase math constants `pi`, `tau`, `two_pi`, `e` in `builtins.rs`
- ✅ `deg(x)` and `rad(x)` functions in `builtins.rs`

### Still needed

- Extend compiled `always` support (IR/VM) for `deg`/`rad` builtins.
- Update rotate action docs/signature in `motion.rs:8`.
- Update examples using raw radians (covered by L4).
- Update `docs/spec.md:276` and `docs/primitives.md`.

---

## H6. Object Field Read/Write — ✅ DONE

**Completed 2026-06-11:**
- Added `Value::get_field()` / `Value::with_field()` helpers in `env.rs`
- Updated `Expr::Path` evaluation in `utils.rs` to walk through `Value::Object`
  fields for multi-segment paths
- Added variable field assignment in `assignments.rs` (`p.x = 30` on Object vars)
- Added tests for read (basic, nonexistent field, backward compat)

### Known limitation

- Field write at frame time (in `always` blocks) is not fully supported — only
  build-time assignment works. The `always` execution path uses compiled
  modifier IR which doesn't know about object field mutation.

---

## M1. Source `Colorscheme` Dotted Keys — ✅ DONE

**Completed 2026-06-11:** Changed the Chumsky construct expression parser in
`parser/mod.rs:449` to use `dotted_ident` instead of `ident` for property keys
inside `TypeName { ... }`. Dotted identifiers like `scene.background` are joined
with `"."` to form the property name string.

### Tree-sitter follow-up still needed

- Update `tree-sitter-animatix/grammar.js` to accept dotted property names
  in object expressions (L5 covers this).

---

## M2. Image/Svg `url` Assignment Docs Contradiction — ✅ DONE

**Completed 2026-06-11:**
- Updated `docs/properties.md` to note that `Svg.url` assignment is
  immediate/static (not time-correct), while `Image.url` supports full keyframe
  animation
- Updated `docs/spec.md` §16 Known Gaps & Limitations with precise language
  about which media types support animated url assignment

---

## M3. `NumberPlane` vs `Graph` Overlap — ✅ DONE

**Completed 2026-06-11:** Updated `docs/architecture.md` and
`docs/primitives.md` to clarify: Graph is a coordinate container for
hosting child plots; NumberPlane is a standalone visual coordinate plane
with auto-generated axes/grid/ticks (no child hosting).

---

## M4. `stroke` vs `color` on `Line` — ✅ DONE

**Completed 2026-06-11:** In `build/actor.rs`, added `stroke_color_explicitly_set`
tracking; when a Line declaration sets `color` without explicit `stroke`/`stroke_color`,
the color is copied to `stroke_color`. In `assignments.rs`, writing to `line.color = ...`
also writes to `stroke_color` for Line actors.

---

## M5. Math Constants `pi`, `tau`, `e` — ✅ DONE

**Completed 2026-06-11:** Added `pi`, `tau`, `two_pi`, `e` in
`crates/animatix/src/timeline/builtins.rs:134`.

### Docs update still needed

~~- Update `docs/spec.md:1033` to mention lowercase constants.~~ ✅
- Update examples using raw `3.14` / `6.28` (H5/L4 covers this).

---

## M6. `sequence`/`stagger` Actor Rejection UX — ✅ DONE

**Completed 2026-06-11:** Updated error messages in
`crates/animatix/src/timeline/sequence.rs:67` and
`crates/animatix/src/timeline/timing.rs:31` to special-case actor declarations
with guidance: "Declare actors before the composition block, then reference
them inside."

---

## L1. Make `MultiplePlayTargets` an Error — ✅ DONE

**Completed 2026-06-11:** Changed `Diagnostic::warning()` to
`Diagnostic::error()` at `crates/animatix/src/composition.rs:585`. Updated
test name and assertion from `test_multiple_play_targets_warning` to
`test_multiple_play_targets_error`.

### Docs update still needed

~~- Update `docs/architecture.md:668` and `docs/spec.md` multi-scene diagnostics
  section to reflect error severity.~~ ✅

---

## L2. Primitive List Consistency (Docs)

**Design intent:** After H2 (remove `Math`, keep `Typst`), ensure all supported
primitive lists in docs agree.

### Plan

1. **Update all supported primitive lists** to remove `Math` and add
   `Typst`/`Mask` where relevant: `docs/spec.md:13`, `docs/spec.md:436`,
   `docs/spec.md:552`, `docs/primitives.md:27`, `docs/properties.md:75`.

2. **Update analyzer built-ins** in
   `crates/animatix-analyzer/src/symbol_table.rs:143`.

3. **Update source fixtures** in
   `crates/animatix-syntax/src/to_source.rs:119`.

### Dependencies

- H2 (must be done first).

### Test Strategy

- `cargo test -p animatix-syntax`.
- `cargo test -p animatix-analyzer`.

### Effort

**Small**

---

## L3. Architecture/Properties Generated-Docs Consistency

**Design intent:** Fix stale claims in architecture and properties docs.

### Plan

1. **Fix stale claims** in `docs/properties.md:90` and
   `docs/primitives.md:398` after M2.

2. **Fix `docs/architecture.md:650`** after M3.

3. **Add a note** that `docs/properties.md` must match `PROPERTY_REGISTRY` in
   `crates/animatix/src/timeline/property_registry.rs:526`.

### Dependencies

- M2, M3, H2 (or can be done as a separate pass).

### Test Strategy

- Manual docs review.
- Optional: add a small registry-vs-docs checker later.

### Effort

**Trivial to small**

---

## L4. Example Modernization

**Design intent:** After H5/M5 and H2, update examples to use new features
(`deg()`, `pi`, `Typst` instead of `Math`).

### Plan

1. **After H5/M5**, replace raw radians in
   `examples/04_motion.amx:17`, `examples/04_motion.amx:29`,
   `examples/16_showcase.amx:37`, `examples/20_feature_reel.amx:50`.

2. **Replace raw pi-ish domains** in `examples/07_plots.amx:14` and
   `examples/18_number_plane_contours.amx:26` with `pi`/`tau` where readable.

3. **After H2**, replace `Math` in `examples/01_shapes.amx:8`.

### Dependencies

- H2, H5, M5.

### Test Strategy

- Parse/build examples with existing example test harness if present, otherwise
  run representative CLI parse/build.
- Verify: `cargo test -p animatix`.

### Effort

**Small**

---

## L5. Tree-Sitter / Analyzer Follow-Through

**Design intent:** If M1 changes tree-sitter grammar, keep tree-sitter and
analyzer in sync.

### Plan

1. **If M1 changes tree-sitter grammar**, update
   `tree-sitter-animatix/grammar.js:236`, regenerate
   `tree-sitter-animatix/src/parser.c`, and update highlights in
   `tree-sitter-animatix/queries/highlights.scm:101`.

2. **Add corpus cases** for dotted construct keys in
   `tree-sitter-animatix/test/corpus/`.

3. **Update analyzer context handling** if property-name nodes can now be
   `path_expression`.

### Dependencies

- M1.

### Test Strategy

- `cd tree-sitter-animatix && tree-sitter test`.
- `cargo test -p animatix-analyzer`.
- `cargo test -p animatix-syntax`.

### Effort

**Small to medium**

---

## Dependency Graph

```
H1 (at/transform) ──── L3 (arch consistency)
     │
H2 (drop Math) ──── L2 (primitive lists) ──── L4 (examples)
     │
H3 (Mask)         H4 (always warning)    L3
     │
H5 (degree+constants) ──── L4 (examples)
     │
H6 (object fields)
     │
M1 (colorscheme keys) ──── L5 (tree-sitter)
     │
M2 (url assignment) ──── L3 (properties.md)
     │
M3 (NumberPlane docs) ──── L3
     │
M4 (Line color)
     │
M5 (math constants) ──── [with H5]
     │
M6 (sequence error msg)
     │
L1 (MultiplePlay → error)
```

**Suggested implementation order:** Start with independent items (M5, M6, L1
are trivial/small), then tackle H-series by decreasing dependency footprint.

---

*This document was generated from the planner findings of the 2026-06-11
language audit. Each plan has been verified against the current source by
reading the referenced files. Edits should follow the file paths and strategies
described above.*