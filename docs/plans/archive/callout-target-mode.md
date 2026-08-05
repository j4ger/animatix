# Plan: Targeted Callout Mode (auto-placement + GUI manual adjustment)

## Goal
Add a `target` mode to the `Callout` primitive where the arrow tip (`to`) and tail/label (`from`) are auto-derived from another actor's bounds (`target: box, place: right, standoff: 40, to_offset: (0,0)`), then allow GUI drags to write `to_offset`/`label_at` offsets. Minimal working target mode first; GUI offset drag second; shift-detach / polish deferred.

## Design decisions (verified against code)
- **New props are independent fields, NOT in `VectorShapeState` group** (like `label_at`). They don't compose with shape morphing. Stored on `GeometryTracks` alongside `label_at` (least new infrastructure; no new `TrackFieldRef`/`PropertyValue` variants — reuse `String`/`F32`/`Vec2`).
- **`place`/`target` stored as `String`** (matches `text_align`/`font_style` precedent). No new enum. `place` ∈ {left,right,top,bottom}, default `right`.
- **Target bounds lookup is the core risk.** `EvaluateCtx` (primitives/mod.rs:315) only has the callout's own `track`. Fix: add `timeline: Option<&'a Timeline>`; `scene_eval.rs` passes `Some(self)` (it's `impl Timeline`, holds all tracks; shared borrows of `self` + a borrow into `self.tracks` coexist). Callout reads target's `geometry.position` (center) + `geometry.size` (half-extents — verify semantics, see Risk R3) directly. **Limitation accepted for Phase 1:** uses target local position+size, not full world transform (fine for flat scenes). Transform-aware bounds deferred.
- **Target-mode geometry:** when `target` non-empty → `to = attach_point(place) + to_offset`; `from = to + label_at` (so arrow tail meets label; label still drawn at `to + label_at` = `from`, reusing existing label code unchanged). When `target` empty → existing manual `from`/`to` path untouched.
- **Bare-ident parsing:** `target: box` / `place: right` (Expr::Ident/Path) won't parse via the generic `ValueType::String` parser (returns `""`). `callout.build` special-handles Ident/Path/Str for `target`/`place` at declaration. Assignment of `place`/`target` via keyframes needs string form (`place: "left"`); `standoff`/`to_offset` (F32/Vec2) assign/animate normally via the registry.
- **No IR/VM sync** (callout is tree-walker only — confirmed: no `Callout` in `ir.rs`/`vm.rs`).
- **No PEG/tree-sitter sync** (`target: box`, `place: right`, `to_offset: (0,0)` are existing Expr forms). Verify only.

---

## Phase 1 — Core target mode (engine only)

### Task 1.1 — Add 4 `ActorField` variants + storage
**Files:** `crates/animatix/src/timeline/property_registry.rs`, `crates/animatix/src/timeline/animation_track.rs`
- In `ActorField` enum (property_registry.rs ~290, near `LabelAt`): add `Target`, `CalloutPlace`, `Standoff`, `ToOffset`.
- In `ActorField::default_value` (property_registry.rs ~415, near `LabelAt` arm): add arms → `Target => String("")`, `CalloutPlace => String("right")`, `Standoff => F32(40.0)`, `ToOffset => Vec2([0.0,0.0])`.
- In `GeometryTracks` (animation_track.rs ~367, near `label_at`): add `pub target: Option<PropertyTrack<String>>`, `pub callout_place: Option<PropertyTrack<String>>`, `pub standoff: Option<PropertyTrack<f32>>`, `pub to_offset: Option<PropertyTrack<[f32;2]>>`. (`GeometryTracks` derives `Default` → None defaults are correct.)
- **Verify:** `cargo check -p animatix` (will fail on dispatch match arms until 1.2 — expected; or do 1.1+1.2 together).

### Task 1.2 — Wire dispatch `field_ref` / `field_mut`
**File:** `crates/animatix/src/timeline/dispatch.rs`
- `field_ref` (~610, near `LabelAt`): `Target => TrackFieldRef::String(&self.geometry.target)`, `CalloutPlace => TrackFieldRef::String(&self.geometry.callout_place)`, `Standoff => TrackFieldRef::F32(&self.geometry.standoff)`, `ToOffset => TrackFieldRef::Vec2(&self.geometry.to_offset)`.
- `field_mut` (~681): same 4 arms with `&mut`.
- Check the string→field maps at ~763/813/863 (`"placement_mode" => PlacementMode` style) — these are name lookups, likely non-exhaustive; if exhaustive over `ActorField`, add the 4 names. Run compiler to find any remaining non-exhaustive match.
- **Verify:** `cargo check -p animatix` compiles clean.

### Task 1.3 — Register the 4 properties
**File:** `crates/animatix/src/timeline/property_registry.rs` (`PROPERTY_REGISTRY`, must stay sorted by name)
- Insert (alphabetical): 
  - `schema!("place", ValueType::String, F::ASSIGNABLE, ActorField::CalloutPlace, None, Applicable::ActorKinds(&[A::Callout]), |_| PropertyValue::String("right".into()))`
  - `schema!("standoff", ValueType::F32, F::ASSIGNABLE_AI, ActorField::Standoff, None, Applicable::ActorKinds(&[A::Callout]), |_| PropertyValue::F32(40.0))`
  - `schema!("target", ValueType::String, F::ASSIGNABLE, ActorField::Target, None, Applicable::ActorKinds(&[A::Callout]), |_| PropertyValue::String(String::new()))`
  - `schema!("to_offset", ValueType::Vec2, F::ASSIGNABLE_AI, ActorField::ToOffset, None, Applicable::ActorKinds(&[A::Callout]), |_| PropertyValue::Vec2([0.0,0.0]))`
- **Verify:** `cargo test -p animatix --lib registry_is_sorted` (the sorted-order test at property_registry.rs ~747) + `lookup_property` roundtrip test pass.

### Task 1.4 — Give `EvaluateCtx` timeline access
**Files:** `crates/animatix/src/primitives/mod.rs`, `crates/animatix/src/timeline/scene_eval.rs`, `crates/animatix/src/timeline/tests/legend.rs`
- `EvaluateCtx` (primitives/mod.rs:315): add field `pub timeline: Option<&'a crate::timeline::Timeline>`. Import `Timeline` (already in scope via `crate::timeline::Timeline`).
- scene_eval.rs (~498, the `EvaluateCtx { ... }` literal): add `timeline: Some(self),`. (`self: &Timeline`; `track` is a shared borrow of `self.tracks` — coexists. If borrow-checker complains about reborrow lifetime, narrow to `timeline: Some(&*self)` or tie lifetimes; fallback = closure `target_lookup: Option<&dyn Fn(&str)->Option<&AnimationTrack>>` — see Risk R1.)
- tests/legend.rs (2 sites, ~89 & ~158): add `timeline: None,`.
- Grep confirmed only 3 `EvaluateCtx {` sites total (scene_eval + 2 legend tests) — update all.
- **Verify:** `cargo check --workspace` (catches GUI/analyzer/LSP drift too) + `cargo test -p animatix --lib`.

### Task 1.5 — Seed new props in `callout.build`
**File:** `crates/animatix/src/primitives/callout.rs`
- Add helpers near `parse_string`: `parse_ident_or_str(&Expr) -> Option<String>` (handles `Expr::Ident`, `Expr::Path`→join with `.`, `Expr::Str`). Reuse for `target` and `place`.
- In `build`, after existing prop loop, parse: `target` (ident/str → String), `place` (ident/str → String, default "right"), `standoff` (parse_f32, default 40.0), `to_offset` (parse_vec2, default [0,0]).
- Seed keyframes (mirror existing `label_at` seeding pattern ~127): `track.geometry.target.ensure(String::new()).add_keyframe(0, target, Linear)`, same for `callout_place`/`standoff`/`to_offset`.
- **Verify:** `cargo test -p animatix --lib` (existing callout tests still pass; new props seeded).

### Task 1.6 — Target-mode geometry in `callout.evaluate`
**File:** `crates/animatix/src/primitives/callout.rs`
- After sampling `from`/`to`/`label_at`/etc. (around ~193), sample new props: `target = ctx.track.geometry.target.get(ctx.time_ms, String::new())`, `place`, `standoff`, `to_offset` (defaults: "right"/40.0/[0,0]).
- Apply overrides from `ctx.overrides` for the 4 new keys (mirror existing override handling).
- If `target` non-empty AND `ctx.timeline` is Some:
  - `let Some(t_track) = ctx.timeline.unwrap().get_track(&target) { ... }` — read `center = t_track.geometry.position.get(ctx.time_ms, [0,0])`, `half = t_track.geometry.size.get(ctx.time_ms, [50,50])` (half-extents — confirm via R3).
  - Compute `attach` by `place`: right→`(center[0]+half[0]+standoff, center[1])`, left→`(center[0]-half[0]-standoff, center[1])`, top→`(center[0], center[1]-half[1]-standoff)`, bottom→`(center[0], center[1]+half[1]+standoff)`. Unknown place→ treat as right.
  - `to = [attach[0]+to_offset[0], attach[1]+to_offset[1]]`; `from = [to[0]+label_at[0], to[1]+label_at[1]]`.
  - If target track missing → `tracing::warn!("callout target '{}' not found", target)` and fall back to sampled from/to (don't render broken arrow).
- If `target` empty → keep existing `from`/`to` (manual mode) untouched.
- Label drawing at `to + label_at` (~206) stays as-is (in target mode that equals `from`).
- **Verify:** new test (Task 1.7) + `cargo test -p animatix --lib`.

### Task 1.7 — Tests for target mode
**File:** `crates/animatix/src/timeline/tests/callout.rs`
- `test_callout_target_mode`: declare a `box: Rect { at:(200,200), size:(100,100) }` + `note: Callout { target: box, label: "Important", place: right, standoff: 40 }`. Build, evaluate at 0s. Assert `note` track has `target="box"`, `place="right"`, `standoff=40`. (Geometry assertions on rendered arrow require inspecting RenderCommand paths — optional; at minimum assert evaluate doesn't panic and target track resolved.)
- `test_callout_target_missing_actor_falls_back`: `target: nonexistent` → evaluate doesn't panic (warn logged).
- `test_callout_manual_mode_unchanged`: existing manual from/to still renders (regression guard — existing tests cover this, but add one asserting `target` empty string default).
- **Verify:** `cargo test -p animatix --lib callout` → all green.

### Task 1.8 — Example + docs
**Files:** `examples/callout_target_example.amx` (new), `docs/spec.md` (~540), `docs/roadmap.md` (~16)
- New example: a Rect + a target-mode Callout (`target: box, place: right, standoff: 40, label: "Important"`), plus a second with `to_offset`/`label_at` tweaks. Ensure it parses + renders (manual `cargo run` or a test that builds it).
- spec.md:540 table: append `target, place, standoff, to_offset` to the `Callout` row; add a short "Target mode" paragraph.
- roadmap.md:16: note target mode added (or add a new roadmap line if kept as in-progress).
- **Verify:** `bash scripts/check-parser-sync.sh` (confirms tree-sitter still parses the new example — should pass since no new tokens) + `cargo test -p animatix-syntax`.

**Phase 1 commit:** `cog commit feat "add Callout target mode with auto-placement" animatix` (or split into 2: schema/fields, then evaluate+tests). Run full gate before commit:
```
cargo check --workspace && cargo test -p animatix-syntax && cargo test -p animatix --lib && cargo test --no-fail-fast
```

---

## Phase 2 — GUI manual offset adjustment (if feasible)

> Goal: let the user drag the callout arrow tip / label on the canvas to write `to_offset` / `label_at`. Inspector editing of the 4 new props is expected to work for free (registry `Applicable` drives the inspector/keyframe table) — verify in Task 2.1.

### Task 2.1 — Verify inspector shows new props (likely free)
**Files:** `crates/animatix-gui/src/app/panels/inspector/*` (read-only check), `crates/animatix-analyzer/*` (read-only check)
- Confirm inspector + keyframe table enumerate `PROPERTY_REGISTRY` filtered by `schema.applicable.includes(ActorKindId::Callout)` (keyframe_table.rs test at ~492 confirms this pattern). New props should appear automatically.
- If the analyzer has a hardcoded callout property allowlist, add the 4 names. (Grep `animatix-analyzer` for `Callout`/`label_at` to find any allowlist.)
- **Verify:** `cargo check --workspace` + manual GUI launch: select a Callout → inspector shows target/place/standoff/to_offset.

### Task 2.2 — Callout drag gesture: tip → `to_offset`, label → `label_at`
**Files:** `crates/animatix-gui/src/app/preview/gestures/move_actor.rs`, `crates/animatix-gui/src/app/preview/drag_utils.rs`, possibly `crates/animatix-gui/src/app/preview/context.rs`
- In the move/press gesture, when the selected actor `kind == ActorKindId::Callout` AND its `target` track is non-empty (target mode):
  - Hit-test two sub-regions: the arrow tip (near `to`) and the label (near `from`/`to+label_at`). Reuse `ctx.get_actor_props` / a computed local bound; may need a small callout-specific hit helper in `drag_utils.rs`.
  - Dragging the tip → emit `DocumentCommand::PropertyEdit(PropertyEdit { actor, property: "to_offset", value: PropertyValue::Vec2(new_offset), create_keyframe: ctx.keyframe_mode })` where `new_offset = mouse_scene - auto_attach_point`. Compute `auto_attach_point` from the target actor's current props (position/size/place/standoff) — mirror the engine math from Task 1.6.
  - Dragging the label → emit `property: "label_at", value: PropertyValue::Vec2(mouse_scene - to)`.
- If `target` empty (manual mode) → keep existing behavior (drag writes `from`/`to`/`position` as today, or leave manual callouts non-draggable — decide; minimal = leave as-is).
- Reuse `finalize_drag_keyframes` (drag_utils.rs ~325) pattern for keyframe creation on drag-end.
- **Verify:** `cargo test -p animatix-gui` + manual: drag a target-mode callout's tip → `to_offset` updates in source/inspector and arrow follows.

### Task 2.3 (optional, smaller) — Snap support for callout offsets
**File:** `crates/animatix-gui/src/app/preview/drag_utils.rs` (`resolve_snap`)
- If 2.2 lands, optionally route callout tip/label drags through `resolve_snap` for guide/edge snapping. Lower priority; defer if time-boxed.

**Phase 2 commit:** `cog commit feat "drag Callout tip/label to set to_offset/label_at" gui`.

---

## Phase 3 — Deferred (explicitly out of scope for now)
- **Shift-detach**: Shift+drag a target-mode callout freezes current auto-geometry into manual `from`/`to` and clears `target`. Needs a new gesture branch + a "bake geometry" op (read current `to`/`from`, write them as keyframes, blank `target`). Design when Phase 1/2 land.
- **Transform-aware target bounds**: target nested in a transformed/rotated parent → currently uses local position+size, not world transform. Fix by routing through `evaluate_node_transform` (cached via `transform_cache`) to get the target's world `local_transform` + `half_size`, then transform the attach point back into the callout's local space. Bigger; defer.
- **`place` as a typed enum** (`CalloutPlace` + `TrackFieldRef` variant + `PropertyValue` variant + `Interpolate`): type-safe, validated. String works for Phase 1.
- **Bare-ident `String` parsing everywhere**: extend `parse_value` `ValueType::String` arm to accept `Expr::Ident`/`Expr::Path` (extract name) so `target: box` / `place: right` work via assignments too. Broader behavior change (affects all String props) — needs its own test sweep.
- **Env-lookup in `callout.build`**: `standoff: gap` (ident in env) currently won't seed (build uses literal parsing). Use `evaluate_expr` in build if needed.

---

## Risks
- **R1 (core, Phase 1.4):** Borrow-checker friction adding `timeline: &'a Timeline` to `EvaluateCtx` while `track: &'a AnimationTrack` borrows `self.tracks`. Shared borrows should compose, but if the compiler rejects the lifetime unification in the `EvaluateCtx { track, timeline: self }` literal, fallback to a lookup closure: `pub target_lookup: Option<&'a dyn Fn(&str) -> Option<&'a AnimationTrack>>`, wired in scene_eval as `Some(&|name| self.tracks.get(name))`. Test this first in Task 1.4.
- **R2:** Target actor declared *after* the callout in source order. At build time all tracks exist (build is multi-pass), and `evaluate` reads `ctx.timeline.get_track(target)` at frame time when all tracks are populated → OK. Verify with a test where target is declared after callout.
- **R3:** `geometry.size` semantics — is it half-extents or full? Evidence: `ActorField::Size` default = `DEFAULT_LAYOUT_HALF_SIZE` [50,50]; `evaluate_node_transform` uses `half_size = effective_vec2(..., "size", ...)`; registry `width` reads `Size.x * 2.0`. → `size` stores **half-extents**. Use `half = size` directly. Double-check against a Rect `size:(100,100)` in a test (does it render 100×100 or 200×200?) before finalizing the attach math.
- **R4:** Callout `build` runs via `ActorKind` dispatch (Annotation → `PrimitiveActorKind`). Confirm `callout.build` actually executes during `process_actor_decl` (it does for Annotation — `find_actor_kind` returns Some). If for some reason the general actor path skips `primitive.build` for callouts, the new props won't seed → verify with the Task 1.7 assertion that `target` track is populated.
- **R5 (GUI):** Hit-testing callout sub-regions (tip vs label) needs the callout's computed `to`/`from` in screen space, which in target mode depends on the target's current frame props. `PreviewContext::get_actor_props` gives position/size; may need to extend `ActorProps` or compute attach on the fly. Could surface as the larger part of Phase 2.
- **R6:** Existing callout tests construct tracks via `callout.build` and assert `from`/`to`/`label`/`label_at`. Adding new props must not change those. The 4 new `GeometryTracks` fields default to `None` → no behavior change for existing tests. Run the full callout test module after 1.5.

## Files to touch (summary)
- `crates/animatix/src/timeline/property_registry.rs` — 4 ActorField variants, 4 default_value arms, 4 registry rows (1.1, 1.3)
- `crates/animatix/src/timeline/animation_track.rs` — 4 GeometryTracks fields (1.1)
- `crates/animatix/src/timeline/dispatch.rs` — 4 field_ref + 4 field_mut arms (1.2)
- `crates/animatix/src/primitives/mod.rs` — `EvaluateCtx.timeline` field (1.4)
- `crates/animatix/src/timeline/scene_eval.rs` — pass `timeline: Some(self)` (1.4)
- `crates/animatix/src/timeline/tests/legend.rs` — 2× `timeline: None` (1.4)
- `crates/animatix/src/primitives/callout.rs` — build seeding + evaluate target-mode geometry (1.5, 1.6)
- `crates/animatix/src/timeline/tests/callout.rs` — target-mode tests (1.7)
- `examples/callout_target_example.amx` (new), `docs/spec.md`, `docs/roadmap.md` (1.8)
- Phase 2: `crates/animatix-gui/src/app/preview/gestures/move_actor.rs`, `drag_utils.rs`, `context.rs` (2.2)
