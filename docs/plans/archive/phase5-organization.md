# Phase 5: Track System Organization

Plan for Tasks 5.1 (decompose `AnimationTrack` into tier sub-structs) and 5.2
(split `track.rs` into focused modules). Phases 1–4 are complete and green
(472 tests, 0 clippy). The property registry remains the single source of
truth; this phase only reshapes *storage* and *file layout* — no dispatch
semantics change.

---

## 0. Current-state findings (verified by reading source)

### 0.1 `AnimationTrack` field inventory (66 total: 6 identity + 60 property)

Source: `crates/animatix/src/timeline/track.rs` lines 610–771.

**Identity (6) — stay flat on `AnimationTrack`:**
`label`, `kind`, `first_seen_ms`, `children`, `visible`, `locked`

**Geometry (13) — `GeometryTracks`:**
`position`, `motion_offset`, `rotation`, `scale`, `transform`,
`placement_mode`, `position_binding`, `size`, `layout_size`, `size_spec`
(non-animated `ChildSizeSpec`), **`min_width`, `min_height`, `max_height`**

**Style (9) — `StyleTracks`:**
`color`, `opacity`, `stroke_width`, `stroke_color`, `stroke_progress`,
`fill_opacity`, `line_cap`, `line_join`, `morph_options`

**Filter (6) — `FilterTracks`:**
`filter_blur`, `filter_brightness`, `filter_contrast`, `filter_saturate`,
`filter_hue_rotate`, `filter_sepia`

**Shape (8) — `ShapeTracks`:**
`shape_type`, `line_from`, `line_to`, `head_size`, `arc_angles`, `points`,
`commands`, `vector_paths`

**Text (15) — `TextTracks`:**
`text_content`, `font_family`, `font_size`, `font_weight`, `font_style`,
`line_height`, `letter_spacing`, `word_spacing`, `text_max_width`,
`text_align`, `overflow`, `text_paths`, `ascent`, `descent`, `baseline`
*(font metrics ascent/descent/baseline move into Text — they are font-related)*

**Highlight (5) — `HighlightTracks`:**
`highlight_color`, `highlight_opacity`, `highlight_padding`,
`highlight_radius`, `highlight_blend` *(non-track `vello::peniko::Mix`)*

**Top-level (stay on `AnimationTrack`, not in any tier struct):**
- `svg_paths: Vec<VelloPath>` — plain Vec, not a PropertyTrack
- `image: Option<PropertyTrack<Option<SceneImage>>>` — `#[cfg(feature="render")]`-gated; keep top-level to avoid cfg inside a sub-struct
- `procedural_plot: Option<ProceduralPlot>` — non-track payload
- `plot_param_tracks: HashMap<String, PropertyTrack<f64>>` — dynamic map, not registry-representable

### 0.2 Gaps / corrections to the task brief

1. **`min_width` / `min_height` / `max_height` are unassigned in the brief.**
   They are size constraints and logically belong in `GeometryTracks` next to
   `size_spec`. Assigned there below.
2. **`action_events` is NOT an `AnimationTrack` field.** It lives on `Timeline`
   (`mod.rs:512`). The `ActionEvent` / `ActionCategory` *types* are defined in
   `track.rs` and re-exported, but there is no track sub-struct for them. The
   brief's tier tree listing `action_events` under `AnimationTrack` is wrong;
   this plan does not create one.
3. **`Easing` is defined in `crate::easing`**, not `track.rs`. `property_track.rs`
   (5.2) will *import* it, not define it.
4. **`ResizeMode` is used** (`primitives/mod.rs:631` `resize_mode()`, GUI scale
   gestures). NOT dead code. It is a geometry enum (no `AnimationTrack` field),
   kept as a type alongside `PlacementMode`/`PositionBinding`/`SceneAnchor`.

### 0.3 Dispatch layer (load-bearing, must stay public)

`field_ref` / `field_mut` / `is_field_currently_animating` /
`has_keyframe_at` / `has_keyframes_for` / `list_keyframes` and the
`TrackFieldRef` / `TrackFieldMut` enums are **public API consumed by GUI**
(`crates/animatix-gui/src/app/actions/mod.rs:378,506-540` matches on
`TrackFieldMut` variants). They must remain `pub` and re-exported from
`timeline`. Internally, `property_engine.rs` drives all reads/writes/
introspection through `field_ref`/`field_mut`, and `max_keyframe_time` /
`has_any_keyframes` iterate the registry via `property_keyframe_times(self,
schema.field)`. **Only `field_ref`/`field_mut` need their bodies updated in
5.1** (e.g. `&self.position` → `&self.geometry.position`); everything
downstream is unchanged because it goes through the dispatch.

### 0.4 Blast radius (call-site estimate)

Direct `track.<field>` access (bypasses registry dispatch) across both crates:

| Tier | animatix/src sites | GUI sites | Notes |
|------|--------------------|-----------|-------|
| Filter | ~13 | 0 | concentrated in `scene_eval.rs` + tests |
| Highlight | ~26 | 0 | `typst.rs`, `fragment.rs`, `scene_eval.rs`, tests |
| Shape | ~45 | ~10 | primitives, `assignments.rs`, `keyframe_utils.rs`, GUI `spreadsheet.rs` |
| Text | ~55 | ~15 | primitives, `layout.rs`, `declarations_text.rs`, GUI `spreadsheet.rs` |
| Style | ~70 | ~30 | `color`/`opacity`/`stroke_*` pervasive |
| Geometry | ~90 | ~40 | `position`/`size` most-touched; `&mut` borrows in `keyframe_utils.rs`, `assignments.rs`, `actions/` |
| Identity | ~40 | ~30 | `label`/`kind`/`children`/`visible`/`locked`/`first_seen_ms` — STAY FLAT, no change |
| **Total property** | **~300** | **~95** | Identity fields untouched |

GUI-side direct enumeration hotspots (read fields by name, not via registry):
- `document.rs:747-769` `track_max_ms` — lists 23 fields, calls `latest_keyframe_ms(&track.X)`.
- `app/stores/source_store.rs:133-154` `push_kf_props` — lists 22 fields.
- `app/panels/inspector/spreadsheet.rs:445-501` — per-property `track.X.as_ref()` reads.

These three are the bulk of GUI churn. *Optional* follow-up (out of scope for
the mechanical migration): rewrite them on the registry-driven
`collect_all_keyframe_times` / `property_keyframe_times` API already in
`mod.rs`/`property_engine.rs`, eliminating the hand-maintained field lists.

### 0.5 No simultaneous mutable field borrows found

`&mut track.X` sites (`keyframe_utils.rs`, `assignments.rs`, `actions/`,
`declarations_text.rs`, `property_engine.rs`) all borrow a single field at a
time, sequentially. `insert_start_keyframes` reads `track.evaluate_vector_paths`
(`&self`, owned result) before the `&mut track.vector_paths` write — NLL ends
the shared borrow first. Nested-field borrow `&mut track.geometry.position`
behaves identically to `&mut track.position`. **No borrow-checker regressions
expected.**

---

## 1. Task 5.1 — Decompose into tier sub-structs

### 1.0 Approach

- Add one tier struct per commit, in ascending blast-radius order.
- Each tier struct is `#[derive(Clone, Debug, Default)]` with `pub` fields
  (mirrors current `pub` visibility so all call sites keep compiling).
- `AnimationTrack` gains a `pub <tier>: <TierStruct>` field; `new()` initializes
  it with `Default::default()`; the flat fields are removed.
- Update `field_ref`/`field_mut` bodies to reach through the sub-struct
  (`&self.geometry.position`).
- Update convenience methods on `AnimationTrack` (`layout_size_get`,
  `ensure_layout_size`, `has_layout_size`, `ascent_get`, `descent_get`,
  `baseline_get`, `set_metrics`, `evaluate_text_paths`,
  `evaluate_vector_paths`) to reach through sub-structs.
- Sweep all direct call sites: `track.X` → `track.<tier>.X` (and
  `&mut track.X` → `&mut track.<tier>.X`).
- Identity fields, `svg_paths`, `image`, `procedural_plot`, `plot_param_tracks`
  stay flat — no call-site changes for them.

### 1.1 Recommended tier order (smallest/cleanest first)

The brief suggests starting with Highlight. Data shows **Filter is smaller and
cleaner** (all `Option<PropertyTrack<f32>>`, zero GUI readers, single frame-time
reader file). Filter is the ideal warmup to validate the sub-struct pattern.
Highlight follows. Order below follows the brief's stated principle
("smallest/least-connected first"), with the deviation flagged.

#### 5.1a — `FilterTracks` (6 fields, ~13 sites)  ← recommended FIRST

Fields: `filter_blur`, `filter_brightness`, `filter_contrast`,
`filter_saturate`, `filter_hue_rotate`, `filter_sepia` (all `Option<PropertyTrack<f32>>`).

Call sites to update:
- `timeline/scene_eval.rs:585-590` (6 reads)
- `timeline/tests.rs:1626-1628` (3 reads)
- `timeline/track.rs` tests: `2136`, `2144`, `2203` (3 reads)
- `field_ref`/`field_mut` arms (6 each) in `track.rs`

No GUI sites. No build-time direct writes (all via `field_mut`).

Verification: `cargo test -p animatix --no-fail-fast && cargo test -p animatix-gui --no-fail-fast && cargo clippy -p animatix -p animatix-gui -- -D warnings`

#### 5.1b — `HighlightTracks` (5 fields, ~26 sites)

Fields: `highlight_color` (`Vec4`), `highlight_opacity` (`F32`),
`highlight_padding` (`F32`), `highlight_radius` (`F32`), `highlight_blend`
(`vello::peniko::Mix`, non-track).

Call sites:
- `primitives/typst.rs:123,140-143` (5 reads incl. `highlight_blend`)
- `primitives/fragment.rs:83,136,142,156,175,193` (writes via `.ensure()`)
- `timeline/scene_eval.rs:717-721` (5 reads incl. `highlight_blend`)
- `timeline/tests.rs:1574`, `track.rs` tests `2167,2405`
- `field_ref`/`field_mut` arms (4 each; `highlight_blend` has no dispatch arm)

Note: `highlight_blend` is `Mix` not `PropertyTrack` — it has no
`field_ref`/`field_mut` arm and is read directly. Still moves into the struct.

Verification: same command.

#### 5.1c — `ShapeTracks` (8 fields, ~45 sites)

Fields: `shape_type`, `line_from`, `line_to`, `head_size`, `arc_angles`,
`points`, `commands`, `vector_paths`.

Call sites: `primitives/{rect,polygon,path,line,arrow,ellipse,plot}.rs`,
`timeline/{scene_eval,assignments,declarations_text,build/keyframe_utils,build/actor,build/plot,build/property}.rs`,
`timeline/tests.rs`, `track.rs` tests, GUI `spreadsheet.rs:445-501`,
`keyframe_table.rs`. `&mut track.vector_paths` in `assignments.rs:812,897,944`
and `keyframe_utils.rs:83`.

Verification: same command.

#### 5.1d — `TextTracks` (15 fields, ~55 sites)

Fields: `text_content`, `font_family`, `font_size`, `font_weight`,
`font_style`, `line_height`, `letter_spacing`, `word_spacing`,
`text_max_width`, `text_align`, `overflow`, `text_paths`, `ascent`,
`descent`, `baseline`.

Call sites: `primitives/mod.rs:64-74`, `timeline/layout.rs:435-460,585-589`,
`scene_eval.rs:716,735-742`, `declarations_text.rs:402-435,508-533`,
`assignments.rs:565,572,633`, `track.rs` methods `ascent_get/descent_get/
baseline_get/set_metrics/evaluate_text_paths`, GUI `spreadsheet.rs`,
`keyframe_table.rs:509`. Note `evaluate_text_paths` crosses tiers
(`self.text.text_paths` + `self.text.text_content` + `self.style.morph_options`).

Verification: same command.

#### 5.1e — `StyleTracks` (9 fields, ~70 sites)

Fields: `color`, `opacity`, `stroke_width`, `stroke_color`, `stroke_progress`,
`fill_opacity`, `line_cap`, `line_join`, `morph_options`.

Pervasive: `color`/`opacity`/`stroke_*` in every primitive, `scene_eval.rs`,
`declarations_text.rs:336,341`, `assignments.rs`, `actions/{entrance,exit,reveal}.rs`
(`ensure_guard_keyframe(&mut track.opacity/stroke_progress/fill_opacity)`),
`keyframe_utils.rs`, GUI `spreadsheet.rs`, `source_store.rs`, `document.rs`,
`overlay.rs`. `evaluate_vector_paths` crosses tiers
(`self.shape.vector_paths` + `self.style.morph_options`).

Verification: same command.

#### 5.1f — `GeometryTracks` (13 fields, ~90 sites)  ← LAST, largest

Fields: `position`, `motion_offset`, `rotation`, `scale`, `transform`,
`placement_mode`, `position_binding`, `size`, `layout_size`, `size_spec`,
`min_width`, `min_height`, `max_height`.

Highest blast radius. `position`/`size` touched in `scene_eval.rs`,
`layout.rs:320,728,773-777,813,855`, `position.rs:181-195`, `assignments.rs`
(many), `build/{actor,keyframe_utils,entry,node}.rs`, `mod.rs` (Timeline methods
`actor_world_affine`, `layout_children_for`, `compute_layout_positions_for_order`),
GUI `overlay.rs`, `context.rs`, `drag_utils.rs`, `actions/mod.rs`,
`handlers/actor.rs`, `source_store.rs`, `document.rs`, `keyframe_table.rs`,
`runtime.rs`.

`size_spec` is non-animated (`ChildSizeSpec`, no dispatch arm). `layout_size`
convenience methods (`layout_size_get/ensure_layout_size/has_layout_size/
layout_size_last`) update to `self.geometry.layout_size`.

Verification: full workspace `cargo test --no-fail-fast && cargo clippy --workspace -- -D warnings`.

### 1.2 Per-tier commit shape (template)

Each 5.1a–f commit:
1. Define tier struct (`#[derive(Clone, Debug, Default)]`, `pub` fields) in `track.rs`.
2. Replace the flat fields on `AnimationTrack` with a single `pub <tier>: <TierStruct>`.
3. Update `AnimationTrack::new()` (remove flat inits; add `<tier>: Default::default()`).
4. Update `field_ref`/`field_mut` arms for that tier's fields.
5. Update affected `AnimationTrack` convenience methods.
6. Sweep call sites (`track.X` → `track.<tier>.X`) in animatix + GUI.
7. `cargo check --workspace` → `cargo test --no-fail-fast` → `cargo clippy --workspace -- -D warnings`.
8. `cog commit refactor "<tier> tier sub-struct" timeline` (scope `timeline` per `cog.toml`).

### 1.3 Risks (5.1)

- **Cross-tier method bodies** (`evaluate_text_paths`/`evaluate_vector_paths`
  reach into both `text`/`shape` and `style.morph_options`). Mitigated by keeping
  these methods on `AnimationTrack` (they see all sub-structs). No change needed
  beyond field-path rewrite.
- **`highlight_blend` / `size_spec` non-track fields** in sub-structs: they lack
  `field_ref`/`field_mut` arms (correct — they aren't keyframed). Just ensure
  `Default` is implemented (`Mix::Difference` default for blend; `None` for
  size_spec). Set explicit defaults in `new()` if `Default::default()` differs
  (blend default is `Mix::Difference` — verify `Default` for `Mix`, else init
  explicitly in `new()` like today).
- **GUI enumeration lists** (`document.rs`, `source_store.rs`, `spreadsheet.rs`)
  are the riskiest sweep — easy to miss one field. Mitigation: after each tier,
  `cargo check -p animatix-gui` catches missed sites as hard compile errors
  (field-not-found). Run it per tier, not just at the end.
- **`image` cfg-gating** stays top-level — do NOT move into a sub-struct (avoids
  `#[cfg]` on struct fields complicating `Default`/`Clone`).
- **`Default` for `AnimationTrack`**: not currently derived (uses `new()`).
  Sub-structs get `Default`; `AnimationTrack::new()` sets identity fields
  explicitly and sub-structs via `Default::default()`. No `#[derive(Default)]`
  needed on `AnimationTrack`.

---

## 2. Task 5.2 — Split `track.rs` into focused modules

### 2.0 Dependency DAG (no cycles; Rust tolerates bidirectional `use` regardless)

```
property_track.rs  PropertyTrack<T>, Interpolate (+all impls), TrackAccessor
       ↑                        ↑ (use, not a true dep)
actor_kind.rs      ActorKindId, ShapeKind, ActorCategory, ActorKindMeta,
                   actor_kind_registry/meta/meta_by_name, from_type_name,
                   ResizeMode, ActionEvent, ActionCategory  *(see 2.2)*
       ↑
animation_track.rs AnimationTrack + tier sub-structs + new() + convenience
                   methods + max_keyframe_time/has_any_keyframes +
                   evaluate_text_paths/evaluate_vector_paths +
                   geometry enums (PlacementMode, SceneAnchor, PositionBinding) +
                   DEFAULT_LAYOUT_HALF_SIZE/DEFAULT_WHITE + shape_type_to_u32
       ↑
dispatch.rs        TrackFieldRef, TrackFieldMut + impls +
                   impl AnimationTrack { field_ref, field_mut,
                   is_field_currently_animating, has_keyframe_at,
                   has_keyframes_for, list_keyframes }
morph.rs (extend)  interpolate_text_paths, interpolate_vello_paths,
                   lerp_color, evaluate_paths_with_options  (move from track.rs)
mod.rs             `mod` decls + `pub use` re-exports (stable external API)
track.rs           DELETED (mod.rs re-exports directly) — see 2.3
```

`animation_track.rs` calls `self.field_ref(...)` (impl block lives in
`dispatch.rs`) and `property_keyframe_times` (`property_engine.rs`) — these are
method calls, not module deps; impl-block location is irrelevant to callers.
`evaluate_text_paths`/`evaluate_vector_paths` call morph.rs helpers — morph.rs
does not depend on animation_track.rs. Clean DAG.

### 2.1 Item-to-module mapping

**`property_track.rs`** (new)
- `PropertyTrack<T>` struct + full `impl` (new, add_keyframe, evaluate,
  evaluate_copy, is_currently_animating, interpolation_segment, evaluate_with,
  last_value, last_value_with, last_keyframe_time, is_effectively_static,
  set_default_value, default_value, keyframes, keyframes_mut) + `Clone` impl.
- `Interpolate` trait + every impl currently in track.rs: `f32`, `f64`,
  `[f32;2]`, `[f32;4]`, `[f32;6]`, `u32`, `MorphOptions`, `Vec<TextPath>`,
  `Vec<VelloPath>`, `Option<SceneImage>` (cfg), `String`, `Vec<String>`,
  `Vec<[f32;2]>`, `PlacementMode`, `SceneAnchor`, `PositionBinding`.
  *(Interpolate impls import their types from animation_track.rs / morph.rs /
  renderer — bidirectional `use` compiles fine.)*
- `TrackAccessor<T>` trait + `impl TrackAccessor for Option<PropertyTrack<T>>`.

**`animation_track.rs`** (new)
- Constants `DEFAULT_LAYOUT_HALF_SIZE`, `DEFAULT_WHITE`.
- Geometry enums `PlacementMode`, `SceneAnchor`, `PositionBinding` (+ their
  non-Interpolate inherent impls). `ResizeMode` stays here too (geometry enum).
  *(Interpolate impls for these live in property_track.rs.)*
- Tier sub-structs `GeometryTracks`, `StyleTracks`, `FilterTracks`,
  `ShapeTracks`, `TextTracks`, `HighlightTracks`.
- `AnimationTrack` struct + `impl` (`new`, `layout_size_*`, `ascent_get`/
  `descent_get`/`baseline_get`/`set_metrics`, `evaluate_text_paths`,
  `evaluate_vector_paths`, `max_keyframe_time`, `has_any_keyframes`).
- `fn shape_type_to_u32` helper (used only by dispatch's `evaluate_value` —
  could instead move to dispatch.rs; keep with the ShapeType consumer. Put in
  dispatch.rs since that's its only caller. **Decision: move to `dispatch.rs`.**)

**`dispatch.rs`** (new)
- `TrackFieldRef<'a>`, `TrackFieldMut<'a>` enums.
- `impl TrackFieldRef` (`evaluate_value`, `has_keyframe_at`, `keyframe_count`,
  `keyframe_times`, `keyframe_easing`).
- `impl AnimationTrack` (`field_ref`, `field_mut`, `is_field_currently_animating`,
  `has_keyframe_at`, `has_keyframes_for`, `list_keyframes`).
- `fn shape_type_to_u32` (only caller is `TrackFieldRef::evaluate_value`).

**`actor_kind.rs`** (extend existing)
- Move from track.rs: `ActorKindId` (+ `from_type_name`), `ShapeKind`
  (+ `From<ShapeType>`), `ActorCategory` (+ `label`), `ActorKindMeta` re-export,
  `actor_kind_registry`, `actor_kind_meta`, `actor_kind_meta_by_name`.
- Existing `ActorKind` trait / `find_actor_kind` / `PrimitiveActorKind` stay.
- **`ActionEvent` / `ActionCategory`**: conceptually action metadata, not actor
  kind. Two options: (a) keep in `animation_track.rs` (co-located with track
  types), (b) new tiny `action_event.rs`. **Decision: keep in
  `animation_track.rs`** (fewer modules; they're only stored on `Timeline`).
  `actor_kind.rs` stays focused on kind identification.

**`morph.rs`** (extend existing)
- Move from track.rs: `lerp_color`, `interpolate_text_paths`,
  `interpolate_vello_paths`, `evaluate_paths_with_options`.
- These currently are private fns used by `AnimationTrack::evaluate_*` and by
  `Interpolate for Vec<TextPath>/Vec<VelloPath>`. After move, make them
  `pub(crate)` (or `pub` if needed by animation_track.rs in the same crate).
  `Interpolate` impls in property_track.rs call `interpolate_text_paths`/
  `interpolate_vello_paths` (morph.rs) — `pub(crate)` suffices.

### 2.2 `mod.rs` changes

Replace `pub mod track;` with:
```rust
pub(crate) mod property_track;
pub(crate) mod animation_track;
pub(crate) mod dispatch;
// actor_kind, morph already declared
```
Update the big `pub use track::{...}` re-export to source from the new modules:
```rust
pub use property_track::{PropertyTrack, Interpolate, TrackAccessor};
pub use animation_track::{
    AnimationTrack, GeometryTracks, StyleTracks, FilterTracks, ShapeTracks,
    TextTracks, HighlightTracks, PlacementMode, ResizeMode, SceneAnchor,
    PositionBinding, ActionEvent, ActionCategory,
    DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE,
};
pub use dispatch::{TrackFieldRef, TrackFieldMut};
pub use actor_kind::{
    ActorKindId, ShapeKind, ActorCategory, ActorKindMeta,
    actor_kind_registry, actor_kind_meta, actor_kind_meta_by_name,
};
```
*(Exact set matches the current `pub use track::{...}` block at `mod.rs:205-210`
— preserves external API.)*

Internal `use crate::timeline::track::X` references elsewhere in the crate
update to `crate::timeline::{X}` (re-exported) or the specific submodule.
`property_engine.rs` already imports `TrackFieldMut` via
`crate::timeline::track::TrackFieldMut` — update to `crate::timeline::TrackFieldMut`.

### 2.3 `track.rs` fate

**Decision: delete `track.rs`; `mod.rs` re-exports directly.** A thin façade
module that only re-exports adds an indirection with no benefit once content is
moved. The `pub use` in `mod.rs` preserves the `crate::timeline::X` path all
callers use. (If any external code imports `crate::timeline::track::X`
path-qualified, grep-and-fix; the re-export makes `crate::timeline::X` work.)

### 2.4 Ordering (do 5.1 fully first, then 5.2)

5.2 is pure moves with `pub use` keeping API stable. Order within 5.2 to keep
each step compiling:

1. **Create `property_track.rs`**: move `PropertyTrack`, `Interpolate`+impls,
   `TrackAccessor`. Leave the rest in `track.rs`. `mod.rs` adds
   `pub(crate) mod property_track;` and `pub use property_track::{...}`.
   `track.rs` now `use super::property_track::{PropertyTrack, Interpolate, TrackAccessor};`.
   → compile, test.
2. **Create `dispatch.rs`**: move `TrackFieldRef`/`TrackFieldMut` + impls +
   `impl AnimationTrack { field_ref, field_mut, is_field_currently_animating,
   has_keyframe_at, has_keyframes_for, list_keyframes }` + `shape_type_to_u32`.
   `track.rs` keeps `AnimationTrack` struct + `new` + eval helpers.
   → compile, test. *(Verify `impl AnimationTrack` split across `track.rs` and
   `dispatch.rs` compiles — it does; Rust allows multiple impl blocks.)*
3. **Extend `actor_kind.rs`**: move `ActorKindId`/`ShapeKind`/`ActorCategory`/
   `ActorKindMeta`/registry fns/`from_type_name`. Update re-exports.
   → compile, test.
4. **Extend `morph.rs`**: move `lerp_color`/`interpolate_text_paths`/
   `interpolate_vello_paths`/`evaluate_paths_with_options`; make `pub(crate)`.
   Update `Interpolate` impls (now in property_track.rs) and
   `evaluate_text_paths`/`evaluate_vector_paths` (still in track.rs) to call
   `super::morph::...`. → compile, test.
5. **Create `animation_track.rs`**: move everything remaining in `track.rs`
   (struct, sub-structs, `new`, convenience methods, `max_keyframe_time`,
   `has_any_keyframes`, `evaluate_*`, geometry enums, constants,
   `ActionEvent`/`ActionCategory`). Delete `track.rs`. Update `mod.rs` `pub use`.
   → compile, test, clippy.
6. Final sweep: `rg 'timeline::track::'` to catch any stale path-qualified
   imports; fix to `timeline::`.

Each step is one commit: `cog commit refactor "split track.rs: <step>" timeline`.

### 2.5 Risks (5.2)

- **Split `impl AnimationTrack` across `track.rs`/`dispatch.rs`**: legal in Rust
  (impl blocks may live in any module of the defining crate). Verify with
  `cargo check` at step 2. If a `pub(crate)`/visibility issue arises, make the
  moved methods `pub` (they already are).
- **`Interpolate` impls in property_track.rs need types from animation_track.rs
  and morph.rs** (PlacementMode, PositionBinding, MorphOptions, Vec<TextPath>
  via TextPath, Vec<VelloPath>). Bidirectional `use super::animation_track::X`
  / `use super::property_track::Y` compiles (Rust resolves the whole crate).
  No action beyond adding the `use` lines.
- **`evaluate_paths_with_options` is generic** (`<T: Interpolate>`); moving to
  morph.rs is fine (morph.rs already generic-free, just add the fn). It uses
  `apply_easing` (`crate::easing`) and `PropertyTrack::interpolation_segment`
  (`pub(crate)` today — confirm visibility; may need to stay `pub(crate)` or
  become `pub(crate)` if not already). Check `interpolation_segment` visibility:
  it is currently `fn` (private to track.rs). **Blocker: it's called by
  `evaluate_paths_with_options` which moves to morph.rs.** Resolution: make
  `interpolation_segment` `pub(crate)` (it's an internal helper). Flag in step 4.
- **Test module** (`#[cfg(test)] mod tests` in track.rs, ~500 lines): split
  naturally — `PropertyTrack`/`Interpolate`/`TrackFieldRef` tests go with their
  module (`property_track.rs`, `dispatch.rs`); `AnimationTrack`/registry
  iteration tests go in `animation_track.rs`. Keep all tests passing at each
  step (move the relevant `#[cfg(test)] mod tests` with the code).
- **`shape_type_to_u32`** currently private in track.rs, called only by
  `TrackFieldRef::evaluate_value`. Moving both to dispatch.rs keeps it private
  to that module. Clean.
- **Re-export surface**: GUI and analyzer import many names from
  `animatix::timeline::{...}`. The `mod.rs` `pub use` list (2.2) must exactly
  preserve the current export set. Verify by diffing the `pub use track::{...}`
  block before/after. Add `GeometryTracks`/`StyleTracks`/etc. as new exports
  (they're `pub` types callers may now reach via `track.geometry`).

---

## 3. Rollback strategy

- Each tier (5.1a–f) and each 5.2 step is its own commit. Rollback = `git revert <sha>`.
- 5.1 tiers are independent: reverting a later tier does not affect earlier ones
  (each commit leaves the tree compiling). If 5.1f (Geometry) goes wrong, revert
  just it; 5.1a–e stay.
- 5.2 steps are sequential and each compiles standalone; revert the failing step
  and redo. Because 5.2 is pure moves behind `pub use`, a partial 5.2 state
  (some modules split, rest still in track.rs) compiles — safe to stop midway.
- **Do not combine 5.1 and 5.2 in one commit.** 5.1 changes field paths; 5.2
  moves files. Mixing makes bisection impossible. 5.1 fully green before 5.2.
- Before starting: tag `phase4-green` (or note current HEAD) so the baseline
  (472 tests, 0 clippy) is recoverable: `git tag phase4-green`.

---

## 4. Verification gates (run at every commit)

```sh
cargo check --workspace                          # 0 errors
cargo test --no-fail-fast                        # all pass (472 baseline)
cargo clippy --workspace -- -D warnings          # 0 warnings
```

Per-tier during 5.1, also run `cargo check -p animatix-gui` immediately after
the sweep to catch missed GUI field rewrites (they fail as E0609 field-not-found).

After 5.1f and after 5.2.5 (final), run the full workspace gate plus:
```sh
rg 'timeline::track::' crates/                   # should be empty after 5.2.6
rg '\btrack\.(position|color|opacity|rotation|scale|transform|motion_offset|filter_|stroke_|fill_opacity|shape_type|line_from|line_to|head_size|arc_angles|points|commands|vector_paths|text_|font_|line_height|letter_spacing|word_spacing|overflow|ascent|descent|baseline|highlight_|placement_mode|position_binding|size|layout_size|min_width|min_height|max_height)\b' crates/animatix/src crates/animatix-gui/src
# should only match identity fields (label, kind, children, visible, locked, first_seen_ms)
# and top-level (svg_paths, image, procedural_plot, plot_param_tracks)
```

---

## 5. Open questions / assumptions to confirm before executing

1. **`Mix::default()`**: confirm `vello::peniko::Mix` implements `Default` and
   that `Default::default() == Mix::Difference`. If not, `HighlightTracks` must
   not derive `Default` — implement `Default` manually with
   `highlight_blend: Mix::Difference`, or init explicitly in `AnimationTrack::new()`.
   (Current `new()` sets `highlight_blend: Mix::Difference` explicitly.)
2. **`interpolation_segment` visibility** (5.2 step 4 blocker): it is private
   `fn` in track.rs today, called by `evaluate_paths_with_options` (moving to
   morph.rs). Plan: make `pub(crate)`. Confirm no other private callers break.
3. **GUI direct-enumeration migration** (optional, out of scope): decide
   whether to rewrite `document.rs::track_max_ms`, `source_store.rs::push_kf_props`,
   `spreadsheet.rs` field reads onto the registry API. Recommended as a **separate
   follow-up phase** after 5.1+5.2 land — it reduces ~70 hand-maintained field
   references to registry iteration, but is behavior-changing (must verify the
   registry enumerates exactly the same fields). Not part of this plan's commits.
4. **`field_ref`/`field_mut` stay as `impl AnimationTrack` methods** (not moved
   onto sub-structs). Confirmed: `property_engine` and GUI call
   `track.field_ref(field)` / `track.field_mut(field)` with a registry `ActorField`;
   sub-structs don't know `ActorField`. Dispatch stays centralized on
   `AnimationTrack`. (5.2 relocates the impl block to `dispatch.rs`, not the
   methods' receiver type.)
5. **Sub-struct field visibility = `pub`**: required because GUI reads
   `track.geometry.position` etc. after the sweep. Confirmed acceptable (mirrors
   current `pub` flat fields).
