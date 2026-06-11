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

## H4. Warn When `always` Overrides Keyframed Properties

**Design intent:** When an `always` block writes to a property that also has
keyframes, emit a warning — the `always` value wins every frame, silently
defeating the keyframes.

### Plan

1. **Add `DiagnosticCode::AlwaysOverridesKeyframes`** in
   `crates/animatix-syntax/src/diagnostics.rs:46`.

2. **Collect assignment targets** from `timeline.modifiers` after build
   processing in `crates/animatix/src/timeline/build/entry.rs:183`.

3. **Recursively inspect modifier statements** from `always` bodies:
   assignments, conditionals, and `for` loops; map `at` to `position` via
   `PropertySchema.read_source` in
   `crates/animatix/src/timeline/property_registry.rs:526`.

4. **Compare against target tracks** using
   `AnimationTrack::list_keyframes()` in
   `crates/animatix/src/timeline/track.rs:1178` or field accessors from
   `property_registry`.

5. **Emit a warning** once per `{actor}.{property}` pair, with message
   explaining that `always` wins every frame.

6. **Include `scene.background_color`** as a special case if `always` can
   target `scene`.

### Risks

- Build inserts snapshot keyframes for animations; warnings may be noisy if
  every animated assignment is flagged.
- Conditional `always` writes are still potential overrides; keep warning
  conservative.
- Dotted component targets must resolve like normal assignments.

### Dependencies

- None.

### Test Strategy

- Add tests in `crates/animatix/src/timeline/tests.rs`: keyframed `opacity` +
  `always { actor.opacity = ... }` warns; non-keyframed property does not.
- Verify: `cargo test -p animatix always_overrides`.

### Effort

**Small to medium**

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

## H6. Object Field Read/Write

**Design intent:** `let p = Point { x: 10, y: 20 }` exists but `p.x` doesn't
work. Implement field read and write for `Value::Object`.

### Implementation check findings

- Parser maps `p.x` to `Expr::Path(["p","x"])` in
  `crates/animatix-syntax/src/parser/mod.rs:491`.
- Object field access currently works only through a method fallback in
  `crates/animatix/src/timeline/utils.rs:760` — not via direct field lookup.

### Plan

1. **Add a resolver** in `evaluate_expr_inner()` at
   `crates/animatix/src/timeline/utils.rs:464`: if `Expr::Path(parts)` does not
   resolve as a flat env key, evaluate the first segment and walk
   `Value::Object` fields for the remaining segments.

2. **Add `Value::get_field()` / `Value::set_field_path()`** helpers in
   `crates/animatix/src/timeline/env.rs:45`.

3. **Implement field assignment** for local variables in
   `Timeline::process_body()` at
   `crates/animatix/src/timeline/build/process.rs:103`: when assignment target
   resolves to a variable object rather than an actor, update the current
   variable track/env value.

4. **Mirror field write support** in `always` execution in
   `crates/animatix/src/timeline/modifier_exec.rs:19` and compiled modifier
   runtime if object writes need frame-time support.

5. **Extend parser tests** around
   `crates/animatix-syntax/src/parser/mod.rs:837` to confirm `p.x = 30` becomes
   target `["p"]`, property `"x"`.

### Risks

- Assignment grammar uses the last path segment as a property, so `p.x = 30` is
  indistinguishable from actor property assignment until build-time resolution.
- Variable tracks are time-keyed; mutating object fields must create a new
  object value at the assignment time rather than mutating past values.
- Component dotted actor paths must still win over object-field writes when
  both names exist.

### Dependencies

- None.

### Test Strategy

- Add evaluator tests: `let p = Point { x: 10 }; let x = p.x`.
- Add build tests: `p.x = 30` updates object variable; unknown field reports a
  diagnostic.
- Verify: `cargo test -p animatix object_field`.

### Effort

**Medium**

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

## M2. Image/Svg `url` Assignment Docs Contradiction

**Design intent:** Fix the contradiction where `properties.md` says `url` is
assignable but the spec/media code doesn't support timed assignment for SVG.

### Implementation check findings

- `Image.url` assignment **is** implemented in
  `crates/animatix/src/primitives/image.rs:47` as timed keyframes.
- `Svg.url` has partial immediate/static assignment in
  `crates/animatix/src/primitives/svg.rs:45` — not time-correct.

### Plan

1. **Preferred: runtime fix** — make `Svg.url` assignment keyframed like
   `Image.url`. Add a keyframed SVG path payload to `AnimationTrack` near
   `crates/animatix/src/timeline/track.rs:545`, or reuse `vector_paths` for
   SVG.

2. **Update `SvgPrimitive::handle_assignment()`** in
   `crates/animatix/src/primitives/svg.rs:45` to write timed keyframes instead
   of replacing `track.svg_paths` globally.

3. **Keep `ImagePrimitive::handle_assignment()`** in
   `crates/animatix/src/primitives/image.rs:47` as the model for SVG.

4. **Update docs** in `docs/spec.md:1127`, `docs/primitives.md:59`,
   `docs/primitives.md:398`, and `docs/properties.md:90`.

### Risks

- Current `Svg.url` assignment looks supported but is not time-correct;
  documenting it as supported without fixing timing would be misleading.
- Adding a new SVG path track touches render evaluation and duration
  calculation.
- Animated URL changes are discrete/crossfade-less unless extra behavior is
  added.

### Dependencies

- None.

### Test Strategy

- Add `Image.url` assignment test proving image keyframes change at the
  assignment time.
- Add `Svg.url` assignment test after runtime fix.
- Verify: `cargo test -p animatix media_assignment`.

### Effort

**Docs-only: trivial. Runtime-correct SVG: medium.**

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