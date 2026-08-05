# Track System Audit Fixes — Implementation Plan

Guiding principle: **the property registry (`PROPERTY_REGISTRY` + `ActorField`) is the single source of truth for all field enumeration.** No hand-maintained field lists. The model to replicate is `collect_all_keyframe_times` (`mod.rs:222`): iterate registry/indices → dispatch through `field_ref`/`field_mut`.

Workspace: `/home/xiayuxuan/Documents/animatix/`. Crate: `crates/animatix`. All tasks must keep `cargo check` + `cargo test --no-fail-fast` green. Per `AGENTS.md`: no unjustified `#[allow(dead_code)]`, remove true dead code, use `tracing`, commit with `cog commit`.

---

## Design decisions, assumptions, and blockers (read first)

### D1 — Registry-completeness prerequisite (BLOCKER for #1, #2, #6)
The registry is **not yet** complete enough to be the single source of truth. These keyframed storage fields have **no `ActorField` variant and no `PROPERTY_REGISTRY` entry**, and are reached only by direct field access:

- Font metrics: `ascent`, `descent`, `baseline` (set via `AnimationTrack::set_metrics` → `add_keyframe`; read by `ascent_get`/`descent_get`/`baseline_get`).
- Highlight: `highlight_color`, `highlight_opacity`, `highlight_padding`, `highlight_radius` (written by `actions/highlight.rs`; read by `scene_eval.rs:717-720`).

Consequence: a registry-driven `max_keyframe_time`/`has_any_keyframes` that iterates only existing `ActorField` variants would **still omit** metrics + highlight — reproducing the exact bugs #1/#2. So Task 1.0 (add the missing variants + registry rows + `field_ref`/`field_mut` arms) is a hard prerequisite. **Bonus finding:** the current `max_keyframe_time`/`has_any_keyframes` *also* omit `ascent`/`descent`/`baseline` (not in the issue list) — Task 1.0 + 1.1 fixes this too.

Decision: add `ActorField::{Ascent, Descent, Baseline, HighlightColor, HighlightOpacity, HighlightPadding, HighlightRadius}` and registry rows with `Applicable::ActorKinds(&[A::Equation, A::Fragment])` for highlight (Equation/Fragment are the real users) and `Applicable::Never` + `F::ANIMATED` for metrics (internal, build-computed, not user-assignable). This keeps them out of the inspector while making them enumerable.

### D2 — Issue #11 (`pub`→`pub(crate)` on `keyframes`/`default_value`) is BLOCKED
`animatix-gui/src/app/actions/mod.rs:388-548` directly mutates `pt.default_value = ...` for ~15 fields (size, offset, at, rotation, scale, color, stroke_width, arc_angles, points, commands, font_family, font_size, font_weight, font_style, shape_type, placement_mode). Making these `pub(crate)` **breaks the GUI build**. Options presented in Task 3.4; recommended path = add `PropertyTrack::set_default_value` + migrate GUI, then tighten visibility. Do **not** blindly flip visibility.

### D3 — Issue #4 (`TextMaxWidth` collision) — corrected blast radius
- Registry property `"max_width"` → `ActorField::TextMaxWidth` (text wrapping, `Applicable::Text/Typst/Code`).
- `field_ref`/`field_mut` dispatch `TextMaxWidth` → **`self.max_width`** (the layout-constraint field). ❌
- Text wrapping is read from `self.text_max_width` (`primitives/mod.rs:72`, `layout.rs:460,585`, `declarations_text.rs:429`).
- The layout `max_width` field is **only read** (`layout.rs:775`, default `f32::INFINITY`) and **never written** by anything except the buggy dispatch. So `max_width: 200` on a Text actor currently (a) does not wrap text and (b) pollutes a layout field.
- There is **no** `ActorField::MaxWidth` variant and **no** registry property for the layout `max_width` constraint (`min_width`/`min_height`/`max_height` exist; `max_width` name is taken by text wrap).

Fix (Task 1.3): dispatch `TextMaxWidth` → `self.text_max_width`. Then the layout `max_width` field becomes dead (never written) → remove the field + the `layout.rs:775` read (replace with `f32::INFINITY` literal) per AGENTS.md dead-code rule. Verify no test/`build/*` writes `track.max_width` first.

### D4 — `plot_param_tracks` and `child_orders` are not registry-representable
- `plot_param_tracks: HashMap<String, PropertyTrack<f64>>` — dynamic parameter names (e.g. `"freq"`), cannot be static registry rows.
- `child_orders: BTreeMap<String, PropertyTrack<Vec<String>>>` lives on `Timeline`, not `AnimationTrack`.

These remain **explicit exceptions** in `max_keyframe_time`/`has_any_keyframes`/`keyframe_times_s`. The registry is the source of truth for *named storage fields*; dynamic collections are enumerated directly. Documented inline.

### D5 — Issue #5 (`read_property_value_or_default`) is NOT dead (correction)
GUI callers: `property_groups.rs:80`, `property_popup.rs:122`, `context.rs:98`. It **is** buggy: it linear-scans `PROPERTY_REGISTRY` for `schema.field == field` and returns the first match's default. For component fields this is ambiguous — `width`/`height`/`radius_x`/`radius_y`/`size` all map to `ActorField::Size`, so the first-by-order winner (`align`? no — `size` comes before `width` alphabetically; still, `height` and `width` share `Size` and would both resolve to whichever sorts first). Fix (Task 1.5): change the signature to take `&PropertySchema` (caller already has it) and use `(schema.default_value)(kind)`; delete the scan. Migrate the 3 GUI callers. This also resolves the "type collision" (no more F32-for-Vec2 risk, since the schema's typed default is used).

### D6 — `is_currently_animating` semantics (#17, #18)
The current contract: "next keyframe after `time_ms` uses non-`Linear` easing." This couples the `_animating_*` env flag to the `write_*` helpers' choice of `Easing::Linear` for auto-snapshot start keyframes. Two bugs:
- #17: a user-authored `Linear` tween reports `_animating = 0` for the whole interval.
- #18: a single future keyframe at t=1000 queried at t=0 reports "animating".

Fixing this is **semantic** and touches every `_animating_*` consumer (modifier authors rely on the current meaning). Task 3.6 proposes a corrected definition: "there exists a keyframe at `time > time_ms` whose *previous* keyframe is at `time <= time_ms`" (i.e. we are strictly inside an interpolation segment), regardless of easing. This fixes #18 (single future kf → prev is None/default → not animating) and #17 (Linear tween → still inside segment → animating). **Risk:** any modifier that relied on the Linear-exclusion hack to suppress `_animating` during auto-snapshots will change behavior. Must grep modifier stdlib/examples and add tests; flag as a behavior change requiring a docs note.

---

## Phase 1 — Correctness (P0)

### Task 1.0 — Registry completeness prerequisite  *(prerequisite for 1.1, 1.2, 2.1)*
**Complexity:** medium · **Files:** `property_registry.rs`, `track.rs`, `property_engine.rs`

1. Add `ActorField` variants: `Ascent`, `Descent`, `Baseline`, `HighlightColor`, `HighlightOpacity`, `HighlightPadding`, `HighlightRadius`.
2. Extend `ActorField::default_value` (`property_registry.rs:370-410`) for the new variants:
   - `Ascent`/`Descent`/`Baseline` → `PropertyValue::F32(0.0)`.
   - `HighlightColor` → `Vec4(DEFAULT_WHITE)`-ish (match `scene_eval.rs:717` default `[0.3,0.5,1.0,1.0]`? — use `[0.3,0.5,1.0,1.0]` to match the render-time fallback, or keep neutral; **decision:** use the `scene_eval` fallback so registry default == render default).
   - `HighlightOpacity` → `F32(0.0)`, `HighlightPadding` → `F32(4.0)`, `HighlightRadius` → `F32(3.0)` (match `scene_eval.rs:718-720`).
3. Add `PROPERTY_REGISTRY` rows:
   - `highlight_color` (`Color`, `F::ANIMATED`, `HighlightColor`, `Applicable::ActorKinds(&[A::Equation, A::Fragment])`).
   - `highlight_opacity`, `highlight_padding`, `highlight_radius` (`F32`, `F::ANIMATED`, …, same applicability).
   - `ascent`/`descent`/`baseline` (`F32`, `F::ANIMATED`, `Applicable::Never`) — internal, not inspector-visible, but enumerable.
   - **Keep registry sorted by `name`** (the `registry_is_sorted` test enforces this).
4. Add `field_ref`/`field_mut` arms (`track.rs:1147-1230`) for all 7 new variants → `F32`/`Vec4` as appropriate. This also advances issue #3 (coverage).
5. No behavior change yet (just makes fields reachable). Existing tests must still pass.

**Verify:** `cargo test -p animatix property_registry` (sorted/lookupable/no-dupes) + `cargo check`.

---

### Task 1.1 — Fix `max_keyframe_time` (issue #1) + bonus metrics  *(depends: 1.0)*
**Complexity:** medium · **Files:** `track.rs` (`max_keyframe_time`, ~`:878-914`), `mod.rs` (helper)

Rewrite `AnimationTrack::max_keyframe_time` to be registry-driven:

```
pub fn max_keyframe_time(&self) -> Option<u64> {
    use crate::timeline::property_registry::{PROPERTY_REGISTRY, PropertyFlags};
    let mut max: Option<u64> = None;
    for schema in PROPERTY_REGISTRY {
        // storage fields only; skip group/NoStorage
        if let Some(t) = property_keyframe_times(self, schema.field).into_iter().max() {
            max = Some(max.map_or(t, |m| m.max(t)));
        }
    }
    // D4 exception: dynamic plot parameter tracks
    for pt in self.plot_param_tracks.values() {
        if let Some(t) = pt.last_keyframe_time() { max = Some(max.map_or(t, |m| m.max(t))); }
    }
    max
}
```

Notes:
- Iterating **all** `PROPERTY_REGISTRY` entries (not `allowed_property_indices(kind)`) ensures a stray keyframe on a non-applicable field is still counted for *duration*. `property_keyframe_times` returns empty for fields with no track, so cost is one `field_ref` match per schema (~80). Acceptable; this runs once per `duration_seconds()` call, not per frame.
- This replaces the 46-entry hand-list and automatically includes `transform`, `highlight_*`, metrics, `text_max_width`, min/max constraints.
- `property_keyframe_times` lives in `property_engine.rs` and already dispatches via `field_ref` — so it only works after Task 1.0 adds the arms.

**Verify:** `cargo test -p animatix` + new tests from Task 4.3 (transform-only, highlight-only, metrics-only max time).

---

### Task 1.2 — Fix `has_any_keyframes` (issue #2)  *(depends: 1.0)*
**Complexity:** small · **Files:** `track.rs` (`has_any_keyframes`, ~`:917-948`)

Rewrite registry-driven:

```
pub fn has_any_keyframes(&self) -> bool {
    use crate::timeline::property_registry::PROPERTY_REGISTRY;
    for schema in PROPERTY_REGISTRY {
        if property_has_keyframes(self, schema.field) {
            // "animated" = not effectively static (2+ kfs, or 1 kf at t>0)
            // reuse is_effectively_static via a new helper (see Task 1.2b)
            if !self.field_is_effectively_static(schema.field) { return true; }
        }
    }
    self.plot_param_tracks.values().any(|t| !t.is_effectively_static())
}
```

**Task 1.2b:** add `AnimationTrack::field_is_effectively_static(field) -> bool` that dispatches via `field_ref` and calls `PropertyTrack::is_effectively_static` on the present track (returns `true` if track absent). This avoids re-implementing the "1 kf at t>0" logic per variant. (Alternatively extend `property_engine` with `property_is_effectively_static`; pick whichever sits closer to `is_effectively_static`'s home in `track.rs`.)

**Verify:** `cargo test -p animatix` + Task 4.3 highlight-only/static tests. Note `is_static_subtree` (`mod.rs`) depends on this — re-run the static-subtree cache tests in `tests.rs`.

---

### Task 1.3 — Fix `TextMaxWidth` dispatch + remove dead layout `max_width` (issue #4)  *(depends: nothing; parallelizable with 1.0-1.2)*
**Complexity:** small · **Files:** `track.rs`, `layout.rs`

1. `field_ref`/`field_mut`: `TextMaxWidth => …(&self.text_max_width)` (both ref and mut, lines `:1179`, `:1223`).
2. Verify nothing writes the layout `max_width`: `grep -rn '\.max_width\s*=' crates/animatix/src` (expected: only GUI `default_value` writes, none for layout `max_width`). **Confirmed during planning: no writer in `animatix/src`.**
3. Remove the `pub max_width: Option<PropertyTrack<f32>>` field from `AnimationTrack` (struct def `:668`-ish + `new()` init `:782`-ish).
4. `layout.rs:775`: replace `track.max_width.get(time_ms, f32::INFINITY)` with `f32::INFINITY` (or remove the branch if it short-circuits).
5. Remove the two `self.max_width.last_time()` / `check!(self.max_width)` references that Tasks 1.1/1.2 already eliminated (they'll be compile errors otherwise — confirms the rewrite took effect).
6. **Decision point:** if a future layout `max_width` property is desired, add `ActorField::MaxWidth` + a distinct registry name (e.g. `"layout_max_width"`) now. **Recommendation: do not add speculatively** (AGENTS.md: no dead code). Leave a `// Reserved for future layout max-width property` only if there's a concrete near-term need; otherwise omit.

**Verify:** `cargo check` (catches all stale `max_width` refs) + `cargo test -p animatix` + a manual `.amx` with `text { max_width: 200; … }` confirming wrap engages (Task 4.4 round-trip).

---

### Task 1.4 — Complete `field_ref`/`field_mut` coverage (issue #3)  *(depends: 1.0)*
**Complexity:** medium · **Files:** `track.rs` (`field_ref` `:1147`, `field_mut` `:1191`), `property_engine.rs` (maybe new `TrackFieldRef`/`Mut` variants)

Add arms for the currently-`None`-returning variants. Mapping decisions:

| `ActorField` | Storage field | `TrackFieldRef` variant |
|---|---|---|
| `FontWeight` | `self.font_weight` | `F32` |
| `FontStyle` | `self.font_style` | `String` |
| `LineHeight` | `self.line_height` | `F32` |
| `LetterSpacing` | `self.letter_spacing` | `F32` |
| `WordSpacing` | `self.word_spacing` | `F32` |
| `MinWidth` | `self.min_width` | `F32` |
| `MinHeight` | `self.min_height` | `F32` |
| `MaxHeight` | `self.max_height` | `F32` |
| `VectorPaths` | `self.vector_paths` | **new `VectorPaths(&Option<PropertyTrack<Vec<VelloPath>>>)`** |
| `TextPaths` | `self.text_paths` | **new `TextPaths(&Option<PropertyTrack<Vec<TextPath>>>)`** |
| `ImageData` | `self.image` | **new `Image(&Option<PropertyTrack<Option<SceneImage>>>)`** (`#[cfg(feature="render")]`) |
| `AudioSource` | — | `None` (no storage field on `AnimationTrack`; audio is on `Timeline.audio_segments`) → keep returning `None` but document |
| `AudioVolume` | — | same: no `AnimationTrack` storage → `None` |
| `PositionBinding` | `self.position_binding` | **new `PositionBinding(&Option<PropertyTrack<PositionBinding>>)`** |

Design choice: adding new `TrackFieldRef`/`TrackFieldMut` variants (`VectorPaths`, `TextPaths`, `Image`, `PositionBinding`) means the **11-arm matches in `property_engine.rs` (#7) and the GUI's `actions/mod.rs:507` match must grow arms**. Since Task 2.2 will collapse the 5 engine matches into one helper, do the variant additions there. For Task 1.4, add only the arms that map to **existing** variants (FontWeight→F32, FontStyle→String, LineHeight/LetterSpacing/WordSpacing/MinWidth/MinHeight/MaxHeight→F32). Defer `VectorPaths`/`TextPaths`/`Image`/`PositionBinding` (need new enum variants) to Task 2.2 — but **note**: `write_property_field` currently no-ops `VectorPaths`/`TextPaths`/`ImageData`/`PositionBinding` in tier-1 (`property_engine.rs:118-125`), so reads via `field_ref` returning `None` for them is consistent today. Adding read access for them is a *capability addition*, not a correctness fix — fold into Task 2.2.

So Task 1.4 scope = the 8 F32/String arms above. After this, `field_ref`/`field_mut` return `Some` for every `ActorField` that has direct `AnimationTrack` storage and an existing `TrackFieldRef` variant. The remaining `None`s are: group fields, `NoStorage`, `SvgPaths` (stored as `Vec<VelloPath>` not a track), `AudioSource`/`AudioVolume` (no track storage), and the 4 deferred new-variant cases.

**Verify:** `cargo test -p animatix` + Task 4.1 exhaustive `field_ref`/`field_mut` coverage test (parameterized over `PROPERTY_REGISTRY`).

---

### Task 1.5 — Fix `read_property_value_or_default` (issue #5, corrected)  *(depends: nothing; parallelizable)*
**Complexity:** small-medium · **Files:** `property_engine.rs`, `mod.rs` (re-export), 3 GUI files

1. Change signature to take the schema the caller already holds:
   ```
   pub fn read_property_value_or_default(
       track: &AnimationTrack,
       schema: &PropertySchema,
       time_ms: u64,
   ) -> PropertyValue {
       read_property_value(track, schema.field, time_ms)
           .unwrap_or_else(|| (schema.default_value)(track.kind))
   }
   ```
   This eliminates the linear scan and the type-collision (the schema's typed default is used, component-extraction handled by caller via `schema.read_source` where needed).
2. Update `mod.rs:72` re-export (signature changed).
3. Migrate GUI callers:
   - `property_groups.rs:80`: `read_property_value_or_default(track, schema, time_ms)` (already has `schema`).
   - `property_popup.rs:122` and `context.rs:98`: pass the looked-up `schema` (they currently pass `schema.field` + `track.kind`; they already do `lookup_property`).
4. Audit: are any callers relying on the *field*-based lookup (i.e. callers that have only an `ActorField`, not a schema)? Grep showed only the 3 GUI sites + the re-export. If a future caller has only an `ActorField`, they should use `ActorField::default_value(field)` (kind-independent) or look up the schema. **Decision:** keep a thin `read_property_value_or_default_by_field` only if needed; current callers all have the schema, so remove the by-field path.

**Verify:** `cargo check --workspace` (GUI compiles) + `cargo test --workspace`.

---

## Phase 2 — Maintainability (P1)

### Task 2.1 — Registry-driven `keyframe_times_s` + retire `extend_track_times` (issue #6)  *(depends: 1.0, 1.1, 1.2)*
**Complexity:** small · **Files:** `mod.rs` (`keyframe_times_s` `:662`, `extend_track_times` `:213`)

Rewrite `Timeline::keyframe_times_s` to reuse `collect_all_keyframe_times`-style enumeration per track, plus the cross-track sources (`background_color`, `child_orders`, `variable_tracks`) that `collect_all_keyframe_times` (per-actor) doesn't cover:

```
pub fn keyframe_times_s(&self) -> Vec<f64> {
    let mut times_ms: BTreeSet<u64> = BTreeSet::new();
    for track in self.tracks.values() {
        for ms in collect_all_keyframe_times(track).iter().map(|f| (*f * 1000.0) as u64) {
            times_ms.insert(ms);
        }
        // plot_param_tracks (D4 exception)
        for pt in track.plot_param_tracks.values() {
            times_ms.extend(pt.keyframes.keys().copied());
        }
    }
    times_ms.extend(self.background_color.keyframes.keys().copied());
    for t in self.child_orders.values() { times_ms.extend(t.keyframes.keys().copied()); }
    for t in self.variable_tracks.values() { times_ms.extend(t.keyframes.keys().copied()); }
    times_ms.into_iter().map(|ms| ms as f64 / 1000.0).collect()
}
```

Note: `collect_all_keyframe_times` uses `allowed_property_indices(kind)` (applicable fields only) — appropriate for GUI markers. For `keyframe_times_s` (also GUI markers) that's the right semantic. **But** confirm `collect_all_keyframe_times` now includes highlight/metrics after Task 1.0 (it will, since they're `Applicable::ActorKinds(&[Equation,Fragment])`/`Never` — wait, `Never` excludes metrics from `allowed_property_indices`). **Decision:** metrics (`Applicable::Never`) won't appear in `collect_all_keyframe_times` — that's fine for GUI markers (metrics aren't user-authored keyframes), and `max_keyframe_time` (Task 1.1) iterates **all** registry entries so it still catches them. Keep the two helpers distinct: `collect_all_keyframe_times` = applicable (GUI markers); `max_keyframe_time`/`has_any_keyframes` = all storage fields (duration/static-ness). Document this split.

Then delete `extend_track_times` (`mod.rs:213`) — now unused.

**Verify:** `cargo test -p animatix` + GUI scrubber tests if any. `cargo check --workspace`.

---

### Task 2.2 — Collapse the 5 `TrackFieldRef` matches in `property_engine.rs` (issue #7)  *(depends: 1.4)*
**Complexity:** medium · **Files:** `property_engine.rs`, `track.rs` (new helper), GUI `actions/mod.rs`

The 5 functions `read_property_value` (`:490`), `property_has_keyframe_at` (`:504`), `property_keyframe_count` (`:528`), `property_keyframe_times` (`:547`), `property_keyframe_easing` (`:566`) each repeat the 11-arm match. Collapse by adding **inherent methods on `TrackFieldRef`** (and the 4 new variants from Task 1.4-deferred):

```
impl<'a> TrackFieldRef<'a> {
    fn keyframes(&self) -> Option<&BTreeMap<u64, (…)>> { match self { … } }  // None if absent
    fn evaluate_value(&self, time_ms) -> Option<PropertyValue> { … }         // copy/clone-aware
}
```

Then each of the 5 functions becomes `track.field_ref(field).map_or(default, |f| f.<op>())`. This:
- Removes 5×11 = 55 arms → 1×11 in the helper.
- Makes adding a new `TrackFieldRef` variant a single-place change (the helper), not 5 places. This unblocks adding `VectorPaths`/`TextPaths`/`Image`/`PositionBinding` variants (Task 1.4-deferred) cheaply.

Sub-tasks:
1. Add the 4 deferred `TrackFieldRef`/`Mut` variants + `field_ref`/`field_mut` arms (VectorPaths, TextPaths, Image `#[cfg(render)]`, PositionBinding).
2. Add `TrackFieldRef::{keyframes, evaluate_value, …}` helper methods.
3. Rewrite the 5 `property_*` functions to use the helpers.
4. Update GUI `actions/mod.rs:507` match to add the new `TrackFieldMut` arms (it matches `TrackFieldMut` directly; add `VectorPaths`/`TextPaths`/`Image`/`PositionBinding` arms mapping to the GUI's `PV` variants, or `_` fallback with a diagnostic).
5. Re-audit `write_property_field` tier-1 no-ops (`VectorPaths`/`TextPaths`/`ImageData`/`PositionBinding`): now that `field_mut` returns them, decide whether tier-1 should still no-op (yes — these are set by dedicated build paths like `compile_text_paths`, not by generic `write_property_field`). Keep the tier-1 no-op arms; they now exist *because* `field_mut` succeeds, preventing the tier-2 fall-through.

**Verify:** `cargo test -p animatix` (esp. `property_*` tests) + `cargo check --workspace` (GUI). Add Task 4.1/4.2 coverage tests.

---

### Task 2.3 — Deduplicate `evaluate_paths_with_options` vs `evaluate_with` (issue #8)  *(depends: nothing; parallelizable)*
**Complexity:** small-medium · **Files:** `track.rs` (`evaluate_paths_with_options` `:952`, `evaluate_text_paths` `:859`, `evaluate_vector_paths` `:874`)

`evaluate_paths_with_options` duplicates the keyframe-walk/interpolate from `evaluate_with` but adds morph-options lookup at the found keyframe time. Refactor so the *walk* is shared:

Option A (preferred): generalize `PropertyTrack::evaluate_with` to accept an optional "post-interpolate hook" — but that complicates the hot path. 

Option B (cleaner): extract the segment-walk into `PropertyTrack::interpolation_segment(time_ms) -> Option<(prev_time, prev_val, found_time, found_val, found_easing)>` (returns `None` for empty/before-first/after-last cases handled by caller). Then both `evaluate_with` and `evaluate_paths_with_options` call `interpolation_segment` and apply their own easing+interpolate (paths additionally looks up `morph_options.keyframes.get(&found_time)`).

Recommend **Option B** — it makes the "beyond last keyframe" caching (issue #19) implementable in one place too. `evaluate_with` becomes: get segment → if None, return `last_value`/`default` → else `prev_val.interpolate(found_val, apply_easing(progress, easing))`. `evaluate_paths_with_options` becomes the same plus morph-options.

Note: `evaluate_paths_with_options` operates on `&PropertyTrack<T>` where `T: Clone + Interpolate` (not `Copy`), and uses a 4-arg `interpolate` fn. Keep its signature; just share the segment logic.

**Verify:** `cargo test -p animatix` (existing path-morph tests in `track.rs` tests module) + Task 4.7/4.8 parity tests.

---

## Phase 3 — Idioms & Ergonomics (P2)

### Task 3.1 — Derive `Debug` for `PropertyTrack<T>` and `AnimationTrack` (issue #9)  *(parallelizable)*
**Complexity:** small · **Files:** `track.rs`

- `PropertyTrack<T>`: add `Debug` bound. `#[derive(Debug)]` requires `T: Debug`; every stored `T` (`f32`, `[f32;2/4/6]`, `u32`, `String`, `Vec<…>`, `ShapeType`, `PlacementMode`, `PositionBinding`, `MorphOptions`, `Option<SceneImage>`) already impls `Debug`. The `RefCell<Option<(u64,T)>>` field is `Debug` if `T: Debug`. So `#[derive(Debug)]` works with a `T: Debug` bound — but `PropertyTrack` has no `T: Debug` bound on the struct. Add `impl<T: Interpolate + Clone + Debug> Debug` manually, or `#[derive(Debug)] PropertyTrack<T: Debug>`. Verify `AnimationTrack` (contains `Option<PropertyTrack<...>>` for many `T`) — all `T` impl `Debug`, so `#[derive(Debug)]` on `AnimationTrack` works. Check `procedural_plot: Option<ProceduralPlot>`, `plot_param_tracks: HashMap<String, PropertyTrack<f64>>`, `size_spec`, `highlight_blend: vello::peniko::Mix` all impl `Debug`.
- This may ripple: any `Debug`-only consumer now prints tracks. Confirm `Timeline` (which holds tracks) doesn't already manually impl `Debug` (it doesn't — `Timeline` has `RefCell` fields and no `Debug`). Adding `Debug` to `AnimationTrack` does **not** require `Timeline: Debug`. Leave `Timeline` without `Debug`.

**Verify:** `cargo check -p animatix`.

---

### Task 3.2 — Manual `Clone` for `PropertyTrack<T>` to drop memo cache (issue #10)  *(parallelizable)*
**Complexity:** small · **Files:** `track.rs`

Replace `#[derive(Clone)]` on `PropertyTrack<T>` (line `:455`) with a manual impl that clones `keyframes` + `default_value` and resets `last_evaluated` to `None` (mirrors `Timeline::clone` `:530-560` which drops caches):

```
impl<T: Interpolate + Clone> Clone for PropertyTrack<T> {
    fn clone(&self) -> Self {
        Self {
            keyframes: self.keyframes.clone(),
            default_value: self.default_value.clone(),
            last_evaluated: std::cell::RefCell::new(None),
        }
    }
}
```

**Verify:** `cargo test -p animatix` (clone-related tests in `tests.rs`) + Task 4.5 cache-invalidation test.

---

### Task 3.3 — `Interpolate: Clone` supertrait (issue #12)  *(depends: 3.2 for clean ordering; parallelizable otherwise)*
**Complexity:** small-medium · **Files:** `track.rs`, all `impl Interpolate` sites, `TrackAccessor`, callers with `T: Interpolate + Clone`

Change `pub trait Interpolate` → `pub trait Interpolate: Clone`. Then simplify bounds:
- `PropertyTrack<T: Interpolate + Clone>` → `PropertyTrack<T: Interpolate>` (struct + impls).
- `TrackAccessor<T: Interpolate + Clone>` → `TrackAccessor<T: Interpolate`.
- `HasDuration for PropertyTrack<T: Interpolate + Clone>` → `T: Interpolate`.
- Callers: `evaluate_paths_with_options<T: Clone + Interpolate>` → `T: Interpolate`. `interpolate_text_paths`/`interpolate_vello_paths` are free fns, no bound change.
- `impl Interpolate for …` blocks: every existing impl is on a `Clone` type (`f32`, `[f32;N]`, `u32`, `String`, `Vec<…>`, `PositionBinding`, `MorphOptions`, `Option<SceneImage>`), so they satisfy the new supertrait automatically. **Verify** `PlacementMode`, `SceneAnchor`, `ResizeMode` impl `Clone` (they derive it). 

Risk: any external impl of `Interpolate` (analyzer/lsp/gui?) that isn't `Clone` breaks. Grep `impl Interpolate` workspace-wide first. If none external, proceed.

**Verify:** `cargo check --workspace` + `cargo test --no-fail-fast`.

---

### Task 3.4 — Tighten `keyframes`/`default_value` visibility + migrate GUI (issue #11, BLOCKED → unblock)  *(depends: D2 decision)*
**Complexity:** medium · **Files:** `track.rs`, `animatix-gui/src/app/actions/mod.rs`

1. Add accessor methods on `PropertyTrack<T>`:
   - `pub fn set_default_value(&mut self, value: T)` (invalidates cache).
   - `pub fn default_value(&self) -> &T` (read access).
   - `pub fn keyframes(&self) -> &BTreeMap<u64, (T, Easing)>` (read access — already effectively public).
   - `pub fn keyframes_mut(&mut self) -> &mut BTreeMap<u64, (T, Easing)>` (invalidates cache).
2. Migrate `animatix-gui/src/app/actions/mod.rs:388-548`: replace `pt.default_value = x` → `pt.set_default_value(x)`. ~15 sites. The `pb_track.default_value = new_binding` (PositionBinding) likewise.
3. Audit other `pub` field reads of `keyframes`/`default_value` across the workspace (the grep in planning found ~50 `.keyframes` reads and ~10 `.default_value` reads, mostly in `animatix/src` which is same-crate — fine for `pub(crate)`; the GUI reads `t.keyframes.len()` etc. which need `pub` read accessors). Migrate GUI reads to `keyframes()` / `default_value()`.
4. Change field visibility: `pub keyframes` → `pub(crate) keyframes`; `pub default_value` → `pub(crate) default_value`.
5. Internal `animatix/src` direct reads can stay (same crate) or migrate to accessors for consistency — recommend migrating hot ones (`scene_eval.rs:189`, `actions/mod.rs:231,246`, `declarations_text.rs:88`) to accessors to model the pattern, but `pub(crate)` permits them either way.

**Rollback:** if GUI migration is too noisy, split into 3.4a (add accessors, migrate GUI) and 3.4b (flip visibility) as separate commits; 3.4a alone is safe and 3.4b can land later.

**Verify:** `cargo check --workspace` + `cargo test --workspace`.

---

### Task 3.5 — `TrackAccessor::get_or_default` (issue #13)  *(parallelizable)*
**Complexity:** small · **Files:** `track.rs` (`TrackAccessor`), callers

Add `fn get_or_default(&self, time_ms: u64) -> Option<T>` returning `self.as_ref().map(|t| t.evaluate(time_ms))` (no default param — uses the track's own `default_value` if present, else `None`). Keep `get(time_ms, default)` for backward compat. Migrate callers that pass a redundant default equal to the track's `default_value` (audit `track.position.get(t, [0.0,0.0])`-style calls where `[0.0,0.0]` matches the track default). This is ergonomic; not all callers migrate. 

**Verify:** `cargo test --workspace`.

---

### Task 3.6 — Fix `is_currently_animating` semantics (issues #17, #18)  *(depends: D6; semantic change — needs care)*
**Complexity:** medium · **Files:** `track.rs` (`is_currently_animating` `:483`), `property_engine.rs` (`_animating_*` injection `:634`), docs

New definition (per D6): "strictly inside an interpolation segment":
```
pub fn is_currently_animating(&self, time_ms: u64) -> bool {
    // Need both a prev (<= time_ms) and a next (> time_ms) keyframe.
    let next = self.keyframes.range((std::ops::Bound::Excluded(time_ms), std::ops::Bound::Unbounded)).next();
    let prev = self.keyframes.range(..=time_ms).next_back();
    matches!((prev, next), (Some(_), Some(_)))
}
```
- Fixes #18: single future kf → `prev = None` → `false`.
- Fixes #17: Linear tween → still `Some/Some` → `true`.
- Decouples from the `write_*` Linear-snapshot implementation detail.

**Risk (D6):** modifier stdlib/examples that exploited the old Linear-exclusion to keep `_animating_* = 0` during auto-snapshots will now report `1` during the snapshot interval. Grep `always` blocks + `_animating` usage in `builtins/`, `examples/`, `docs/`. If a built-in relies on the old behavior, either (a) accept the behavior change and update the example, or (b) keep a separate `is_in_nonlinear_segment` for the snapshot heuristic and use the new definition for `_animating_*`. **Recommendation:** change `_animating_*` to the new definition (it's the user-facing semantic) and audit built-ins; the snapshot-Linear trick was an implementation accident, not a contract.

Add a docs note in `docs/` for the `_animating_*` semantics change (user-visible).

**Verify:** `cargo test --workspace` + Task 4.6 `is_currently_animating` tests + manually run an example with an `always` block reading `_animating_opacity`.

---

### Task 3.7 — Memoization: cache beyond-last-keyframe + single-clone (issues #19, #20)  *(depends: 2.3 for segment helper)*
**Complexity:** small · **Files:** `track.rs` (`evaluate_with` `:489`, `last_value_with` `:531`)

- #19: the `None => return self.last_value_with(clone_val)` early-return at `:501` skips the cache write. Move cache population before the early return: compute `last_value`, store `(time_ms, clone_val(&last_value))` in `last_evaluated`, then return. Now repeated "beyond last" queries hit the cache.
- #20: the `time_ms <= first_time` branch (`:506-510`) does `clone_val(found_val)` then `clone_val(&value)` to store — two clones. Store first, return a ref-clone: 
  ```
  let value = clone_val(found_val);
  *self.last_evaluated.borrow_mut() = Some((time_ms, clone_val(&value)));
  return value;
  ```
  can become (avoid double clone): clone once into the cache, then clone out of the cache for return — still two. Better: since `clone_val` is `T::clone` or `*v`, for `Copy` types it's free; for `Clone` types the double-clone is the cost. Use `std::mem::replace`-style: build `value`, then `let cached = value.clone(); *cache = Some((time_ms, cached)); value`. Same count. The real win is for `Copy`: `evaluate_copy` should bypass cloning the cache entry on read. Already does (`clone_val = |v| *v`). Mark #20 as "minor; addressed if free, else document." 

Actually #20's two-clone is structural: you must have a value to return AND a value to cache. For non-`Copy` `T` that's inherently 2 clones unless you cache by `Arc<T>`. **Decision:** for #20, change cache to `Option<(u64, T)>` and on hit return `clone_val(cached)` (1 clone on hit, 0 on miss-write for the value being returned if we move it in then clone out — still 1+1 on miss). Net: miss = 1 clone (build result) + 1 clone (into cache) = 2; hit = 1 clone. Unchanged for miss. So #20 is **not really fixable** without `Arc`. Document this; focus #19 (the real win) and leave #20 as a noted micro-limitation. Update issue tracker accordingly.

**Verify:** `cargo test -p animatix` + Task 4.5 cache hit/invalidation tests + a micro-benchmark if desired.

---

### Task 3.8 — `--no-default-features` build (issue #22)  *(parallelizable)*
**Complexity:** small-medium · **Files:** `track.rs` (`image` field `:663`), `scene_eval.rs`/`primitives` if needed

The `image: Option<PropertyTrack<Option<crate::timeline::image::SceneImage>>>` field references `image::SceneImage` which is `#[cfg(feature = "render")]`. The `Interpolate for Option<SceneImage>` impl is already `#[cfg(feature = "render")]` (`track.rs:417`). So without `render`, the field type is unknown → won't compile. Gate the field + its `field_ref`/`field_mut`/`evaluate`-adjacent uses behind `#[cfg(feature = "render")]`, OR introduce a type alias `#[cfg(feature = "render")] type ImageData = Option<SceneImage>; #[cfg(not(...))] type ImageData = ();` and gate the `Interpolate` impl accordingly. 

**Decision:** gate the `image` field, its `new()` init, and the `ImageData` `ActorField` arm behind `#[cfg(feature = "render")]`. The `TrackFieldRef::Image` variant (added in Task 2.2) must also be gated. Verify `cargo build -p animatix --no-default-features` compiles (it currently doesn't build at all per the issue — this is a latent fix; confirm with `cargo check -p animatix --no-default-features`).

Caveat: the crate may have *other* non-`render` compile blockers beyond `image`. Scope this task to the `image` field; if `--no-default-features` still fails elsewhere, file follow-ups (don't expand this task).

**Verify:** `cargo check -p animatix --no-default-features` + `cargo check -p animatix` (default).

---

### Task 3.9 — Remove dead `VariableTrack::new` (issue #16)  *(parallelizable)*
**Complexity:** trivial · **Files:** `mod.rs` (`:420`)

Grep confirmed zero callers of `VariableTrack::new` (the struct derives `Default` and is constructed via `Default::default()` or map insertion). `VariableTrack` itself is used (`variable_tracks` field, `HasDuration` impl) — only `new()` is dead. Remove `fn new()`. Per AGENTS.md dead-code rule.

**Verify:** `cargo test -p animatix` (no test calls `new`).

---

## Phase 4 — Tests (P2 testing gaps)

All test tasks add to `crates/animatix/src/timeline/tests.rs` (existing `#[cfg(test)] mod tests;` declared in `mod.rs:1438`). Each is independent and parallelizable unless noted. Prefer parameterized tests over `PROPERTY_REGISTRY` where possible (a single test loops all schemas → catches future regressions for free).

### Task 4.1 — Exhaustive `field_ref`/`field_mut` coverage over `ActorField` (issue #23)  *(depends: 1.0, 1.4, 2.2)*
Loop over every `ActorField` that has `AnimationTrack` storage; assert `field_ref` returns `Some` for all except the documented `None` set (group fields, `NoStorage`, `SvgPaths`, `AudioSource`, `AudioVolume`). For each `Some`, assert the returned `TrackFieldRef` variant matches the expected storage type. Mirror for `field_mut`. This test is the regression net for issues #3/#4.

### Task 4.2 — `write_property_field` → `read_property_value` round-trip (issue #24)  *(depends: 1.4, 1.5, 2.2)*
For each `ActorField` with a registry property and direct storage: write a `PropertyValue` via `write_property_field` at t=0 and t=500 with an easing, then `read_property_value` at t=0/250/500 and assert interpolation. Covers #3 silently-dropped writes. Exclude tier-1 no-op fields (`VectorPaths`/`TextPaths`/`ImageData`/`PositionBinding`/`PlacementMode`/`MorphOptions`).

### Task 4.3 — `max_keyframe_time`/`has_any_keyframes` for transform-only, highlight-only, metrics-only (issue #25)  *(depends: 1.1, 1.2)*
Build an `AnimationTrack`, add a single keyframe to *only* `transform` (assert `max_keyframe_time == Some(t)` and `has_any_keyframes == true`); repeat for each `highlight_*` and each metric. Also assert an all-empty track → `None`/`false`. Also assert a single kf at t=0 → `has_any_keyframes == false` (effectively static).

### Task 4.4 — `max_width`/`text_max_width` round-trip (issue #4 regression)  *(depends: 1.3)*
Assign `max_width: 200` to a Text actor via `write_property_field(TextMaxWidth, F32(200), …)`; assert `read_property_value(track, TextMaxWidth, t)` returns `F32(200)` and that `track.text_max_width` (not `max_width`) holds the keyframe. Confirms the dispatch fix.

### Task 4.5 — Memoization cache hit/invalidation (issue #29)  *(depends: 3.2, 3.7)*
- Hit: `evaluate(500)` twice; assert the second returns the same value (and, if instrumented, didn't re-walk — use a `Cell`-counter `Interpolate` impl or just assert correctness).
- Invalidation: `add_keyframe` after `evaluate` → `evaluate` again returns the new interpolated value (cache cleared by `add_keyframe`).
- Beyond-last (#19): `evaluate(large_t)` twice → second is cached (assert via a `RefCell` inspection or a counter).
- Clone drops cache (3.2): `track.clone().evaluate(same_t)` re-walks (assert cache is `None` post-clone via a test-only accessor or behavioral check).

### Task 4.6 — `is_currently_animating` / `_animating_*` (issue #26)  *(depends: 3.6)*
- Empty track → false. Single future kf (t=1000) at t=0 → false (#18). Two kfs (0→1000) at t=500 → true; at t=0 → true (at first kf, next exists, prev = first itself via `..=0`); at t=1000 → false (no next). Linear easing still true (#17).
- `_animating_*` env injection: build a tiny timeline, run `inject_property_into_env`, assert `env.get("label._animating_opacity")` matches `is_currently_animating`.

### Task 4.7 — `is_effectively_static` (issue #27)  *(parallelizable)*
0 kfs → true; 1 kf at t=0 → true; 1 kf at t=500 → false; 2 kfs → false. Also `AnimationTrack::has_any_keyframes` consistency with per-field `is_effectively_static`.

### Task 4.8 — Empty / single-keyframe / `time_ms < first_keyframe` (issue #28)  *(parallelizable, depends: 2.3)*
- Empty track `evaluate(t)` → `default_value`.
- Single kf at t=500: `evaluate(0)` → first kf value (per current `time_ms <= first_time` semantics, #21); `evaluate(500)` → kf value; `evaluate(1000)` → kf value.
- `time_ms < first_time` (e.g. first at 200, query 100): assert returns first kf value (not default) — pins #21 semantics with a test.

### Task 4.9 — Easing application (issue #30)  *(parallelizable)*
Track with kfs at 0 (`0.0`) and 1000 (`100.0`), easing `Easing::EaseInOut` (or a known piecewise). Assert `evaluate(250)`, `evaluate(500)`, `evaluate(750)` match `apply_easing(0.25/0.5/0.75, easing)` interpolation. Also `Easing::Linear` is identity-progress.

### Task 4.10 — `evaluate_copy` vs `evaluate` parity (issue #31)  *(parallelizable)*
For `Copy` types (`f32`, `[f32;2]`, `[f32;4]`, `u32`, `[f32;6]`): build identical tracks, assert `evaluate_copy(t) == evaluate(t)` for t in {0, mid, end, beyond, before-first}. Pins the two code paths stay equivalent (especially after Task 2.3 refactor).

### Task 4.11 — `time_ms <= first_time` semantics pinned (issue #21)  *( folds into 4.8 )*
Covered by 4.8; add an explicit assertion + doc-comment on `evaluate_with` stating "for `time_ms <= first_keyframe`, returns the first keyframe's value (not the default)." Update the `evaluate_with` doc comment to state this contract.

---

## Phase 5 — Organization (P2 structural)

These are larger, higher-risk refactors. Do them **last**, after behavior is fixed and tested (Phases 1-4). Each is independently revertible (pure moves). Keep `cargo check` + `cargo test` green after each.

### Task 5.1 — Decompose `AnimationTrack` into tier sub-structs (issue #14)  *(LARGE — depends: 1.0-1.4, 2.2 done)*
**Complexity:** large · **Files:** `track.rs`, all `animatix/src` direct-field readers (~30 sites), GUI

Introduce:
```
pub struct GeometryTracks { position, motion_offset, rotation, scale, transform, placement_mode, position_binding, size, layout_size, min_width, min_height, max_height }
pub struct StyleTracks { color, opacity, stroke_width, stroke_color, stroke_progress, fill_opacity, line_cap, line_join, morph_options }
pub struct FilterTracks { filter_blur, …, filter_sepia }
pub struct ShapeTracks { shape_type, line_from, line_to, head_size, arc_angles, points, commands, vector_paths }
pub struct TextTracks { text_content, font_family, font_size, font_weight, font_style, line_height, letter_spacing, word_spacing, text_max_width, text_align, overflow, text_paths, ascent, descent, baseline }
pub struct HighlightTracks { highlight_color, highlight_opacity, highlight_padding, highlight_radius, highlight_blend }
```
`AnimationTrack` becomes: identity fields + `geometry: GeometryTracks` + `style: StyleTracks` + … + `image`, `svg_paths`, `procedural_plot`, `plot_param_tracks`, `size_spec`.

**Blast radius:** every `track.position` → `track.geometry.position`; ~150 call sites in `animatix/src` + GUI. `field_ref`/`field_mut` dispatch updates to `self.geometry.position` etc. (one集中 place). This is mechanical but huge.

**Mitigation / rollback:** do it in tier-by-tier commits (Geometry first, build green, then Style, …). Add `#[allow(deprecated)]`-style compatibility accessor methods `track.position() -> &PropertyTrack` temporarily? No — AGENTS.md forbids unused allows. Instead, do one tier per commit and migrate all readers in that commit. Revert = single commit revert per tier.

**Risk:** `#[derive(Clone, Debug)]` must propagate to sub-structs (they're all `Clone`+`Debug` after Task 3.1/3.2). `field_ref`/`field_mut` borrows `&self.geometry.field` — borrow checker fine (disjoint borrows via sub-struct). `field_mut` mutates `&mut self.geometry.field` — fine.

**Decision:** this is the highest-risk task. **Recommend executing only if the team wants the long-term structure**; it's not a correctness fix. If deferred, leave a `// TODO(issue #14): decompose into tier sub-structs` note and move on. The plan includes it for completeness.

---

### Task 5.2 — Split `track.rs` (1729 lines) into focused modules (issue #15)  *(LARGE — depends: 5.1 recommended first, or standalone)*
**Complexity:** large · **Files:** new `property_track.rs`, `animation_track.rs`, `dispatch.rs`, `morph.rs` (extend existing), `actor_kind.rs` (extend existing)

Proposed split (current `track.rs` responsibilities):
- `actor_kind.rs` (exists, `mod actor_kind;`): move `ActorKindId`, `ShapeKind`, `ActorCategory`, `ActorKindMeta` re-exports, `from_type_name`, `From<ShapeType> for ShapeKind`. (`ActorKind` already lives here.)
- `property_track.rs`: `PropertyTrack<T>`, `Interpolate` trait + all impls, `TrackAccessor`, `TrackFieldRef`/`TrackFieldMut` enums, `field_ref`/`field_mut`/`is_field_currently_animating`/`has_keyframe_at`/`has_keyframes_for`/`list_keyframes` (the dispatch surface), `evaluate_paths_with_options` + `interpolate_text_paths`/`interpolate_vello_paths` (or move path interp to `morph.rs`).
- `animation_track.rs`: `AnimationTrack` struct + `new` + tier accessors + `max_keyframe_time`/`has_any_keyframes` + `evaluate_text_paths`/`evaluate_vector_paths`.
- `dispatch.rs` (optional): the `field_ref`/`field_mut` match blocks if kept separate from `PropertyTrack` helpers.
- `morph.rs` (exists): move `interpolate_text_paths`/`interpolate_vello_paths`/`lerp_color`/`evaluate_paths_with_options` here (they're morph-specific). Depends on `PropertyTrack` being in `property_track.rs`.
- `track.rs` shrinks to a façade re-exporting the above (or `mod.rs` re-exports directly). The `pub use track::{…}` in `mod.rs:183` must keep the same public surface.

`ActionEvent`/`ActionCategory` → move to `actions/mod.rs` or a new `actions/event.rs` (they're action metadata). `PositionBinding`/`SceneAnchor`/`PlacementMode`/`ResizeMode` → `property_track.rs` or a `primitives.rs` (they're value types with `Interpolate`).

**Blast radius:** pure module move; `mod.rs` `pub use` keeps external API stable. Internal `use crate::timeline::track::X` refs update. `tests.rs` imports `use super::*` — must re-export from the new home modules or update.

**Risk:** `Interpolate` impls for `Vec<VelloPath>`/`Vec<TextPath>` reference `interpolate_vello_paths`/`interpolate_text_paths` — if those move to `morph.rs`, the impls move too (or `morph.rs` becomes `pub(crate)`-imported). Circular-import risk: `property_track.rs` ↔ `morph.rs`. Resolve by keeping path-interp fns in `morph.rs` and the `Interpolate` impls in `morph.rs` too (they're morph). `PropertyTrack` itself doesn't depend on morph.

**Ordering:** do 5.2 **after** 5.1 (sub-structs) so `animation_track.rs` is a clean home for the decomposed struct. If 5.1 is deferred, 5.2 still works but `animation_track.rs` is large.

---

## Dependency graph & parallelization

```
1.0 (registry complete) ─┬─► 1.1 (max_keyframe_time)
                         ├─► 1.2 (has_any_keyframes)
                         ├─► 1.4 (field_ref/mut coverage) ─► 2.2 (collapse matches)
                         └─► 2.1 (keyframe_times_s)
1.3 (TextMaxWidth) ──────────────────────────────────► (independent, parallel with 1.0-1.2)
1.5 (read_property_value_or_default) ───────────────► (independent, parallel)
2.3 (evaluate_paths dedup) ─────────────────────────► (independent, parallel)
3.1 (Debug) ───────────────────────────────────────► (independent)
3.2 (Clone) ───────────────────────────────────────► (independent) ─► 4.5 (cache tests)
3.3 (Interpolate: Clone) ──────────────────────────► (after 3.2 ok; parallel otherwise)
3.4 (pub(crate) + GUI) ───────────────────────────► (independent; split 3.4a/3.4b)
3.5 (get_or_default) ─────────────────────────────► (independent)
3.6 (is_currently_animating) ─────────────────────► (independent) ─► 4.6
3.7 (memo beyond-last) ───────────────────────────► (after 2.3) ─► 4.5
3.8 (--no-default-features) ──────────────────────► (independent; after 2.2 ideally)
3.9 (VariableTrack::new) ─────────────────────────► (independent, trivial)
4.x tests ────────────────────────────────────────► (depend on their corresponding fix tasks)
5.1 (decompose) ──────────────────────────────────► (LARGE; after 1.x, 2.2; optional)
5.2 (split track.rs) ─────────────────────────────► (LARGE; after 5.1 ideally; optional)
```

**Parallel tracks (can proceed simultaneously by different people/commits):**
- Track A: 1.0 → 1.1 → 1.2 → 2.1 (registry-driven enumeration).
- Track B: 1.4 → 2.2 (dispatch coverage + match collapse).
- Track C: 1.3 + 1.5 (independent correctness fixes).
- Track D: 2.3 + 3.7 (evaluate dedup + memo).
- Track E: 3.1 + 3.2 + 3.3 + 3.5 + 3.9 (low-risk ergonomics).
- Track F: 3.6 (semantic; needs audit + docs).
- Track G: 3.8 (build-gating).
- Track H: 3.4 (GUI-coupled; split a/b).
- Tests 4.x slot in as their deps land.

**Serial bottlenecks:** 1.0 blocks Track A and B. 2.2 blocks the new-`TrackFieldRef`-variant part of 1.4. 5.1 blocks 5.2 (recommended).

---

## Complexity summary

| Task | Complexity | Files |
|---|---|---|
| 1.0 | medium | property_registry.rs, track.rs, property_engine.rs |
| 1.1 | medium | track.rs, mod.rs |
| 1.2 | small | track.rs |
| 1.3 | small | track.rs, layout.rs |
| 1.4 | medium | track.rs, property_engine.rs |
| 1.5 | small-medium | property_engine.rs, mod.rs, 3 GUI files |
| 2.1 | small | mod.rs |
| 2.2 | medium | property_engine.rs, track.rs, GUI |
| 2.3 | small-medium | track.rs |
| 3.1 | small | track.rs |
| 3.2 | small | track.rs |
| 3.3 | small-medium | track.rs + callers |
| 3.4 | medium | track.rs, GUI |
| 3.5 | small | track.rs |
| 3.6 | medium | track.rs, property_engine.rs, docs |
| 3.7 | small | track.rs |
| 3.8 | small-medium | track.rs |
| 3.9 | trivial | mod.rs |
| 4.1-4.11 | small each | tests.rs |
| 5.1 | large | track.rs + ~30 readers + GUI |
| 5.2 | large | new modules + mod.rs |

---

## Global risks & rollback

- **Registry sort invariant:** every Task 1.0/1.3 registry addition must keep `PROPERTY_REGISTRY` sorted by `name`; the `registry_is_sorted` test guards this. The `schema!` macro rows are hand-ordered — easy to mis-sort. Run that test after every registry edit.
- **`collect_all_keyframe_times` vs `max_keyframe_time` semantic split (D1/D4):** `collect_all_keyframe_times` uses *applicable* indices (GUI markers, excludes `Applicable::Never` metrics); `max_keyframe_time` uses *all* registry entries (duration, includes metrics). Getting these crossed would either hide metrics from duration or surface internal fields in the GUI. Tests 4.3 + the existing GUI-marker tests guard both.
- **#18/#17 semantic change (3.6):** biggest user-visible risk. Audit `builtins/`, `examples/`, `docs/` for `_animating_*` consumers before merging. Keep old impl behind a test that documents the new contract. Rollback = revert the single `is_currently_animating` commit.
- **GUI coupling (3.4, 1.5, 2.2):** any signature/visibility change ripples to `animatix-gui`. Always run `cargo check --workspace`, not just `-p animatix`. Rollback per-commit.
- **Large refactors (5.1, 5.2):** pure structural moves; revertible per-commit, but conflict-prone if interleaved with Phases 1-4. Land them **last** on a quiet tree.
- **`--no-default-features` (3.8):** may uncover *other* ungated `render` deps; scope strictly to `image`. If the crate still doesn't build no-default, file separate follow-ups rather than expanding the task.
- **Dead-code rule (AGENTS.md):** Tasks 1.3 (layout `max_width` removal), 2.1 (`extend_track_times`), 3.9 (`VariableTrack::new`) remove code — confirm zero callers via grep *immediately before* deletion (callers can appear in tests/examples). The grep evidence in this plan is a snapshot; re-verify at execution time.

**Per-task verification baseline:** `cargo check -p animatix` (0 errors) and `cargo test -p animatix --no-fail-fast` (all passing) after every task; `cargo check --workspace` + `cargo test --workspace --no-fail-fast` for any task touching the public API or GUI (1.5, 2.1, 2.2, 3.3, 3.4, 3.6, 5.x). Commit each task with `cog commit <type> "<summary>" [scope]` (scopes: `timeline`, `renderer`, `gui`, `docs` as appropriate).
