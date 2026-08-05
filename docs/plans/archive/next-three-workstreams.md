# Next Three Workstreams — Implementation Plan

Each workstream is self-contained. Within a workstream, tasks are ordered by
dependency. Verification commands assume the repo root
`/home/xiayuxuan/Documents/animatix/`.

---

# Workstream 1 — Split `timeline/tests.rs` into per-module test files

## Goal
Break the ~1700-line `crates/animatix/src/timeline/tests.rs` into focused,
co-located test modules so each tests file sits beside (or inside) the module
it exercises, and `cargo test` still discovers every test.

## Current state
- `crates/animatix/src/timeline/mod.rs:1098-1099` declares the test module as a
  **file module**:
  ```rust
  #[cfg(test)]
  mod tests;
  ```
- `tests.rs:1` opens with `use super::*;` (brings in everything `mod.rs`
  re-exports/uses: `Timeline`, `AnimationTrack`, `Environment`, `Value`,
  `SceneDimensions`, `DebugRenderOptions`, `for_iter_values`,
  `load_standard_library`, `gap_uniform`, `padding_uniform`, `LayoutEngine`,
  `ContainerMetadata`, `LayoutType`, `PlacementMode`, `Easing`, `ActorKindId`,
  `collect_all_keyframe_times`, etc.) **plus** an explicit
  `use crate::ast::{BinaryOp, Property};` on line 2.
- Many sibling modules already have inline `#[cfg(test)] mod tests { … }`
  blocks (verified via grep): `animation_track.rs`, `property_registry.rs`,
  `property_engine.rs`, `primitive.rs`, `scene_eval.rs`, `morph.rs`,
  `taffy_layout.rs`, `kurbo_shapes.rs`, `position.rs`, `index.rs`, `utils.rs`,
  `shapes/mod.rs`, `shapes/primitives.rs`, all `actions/*` modules.
  → We must NOT collide with these; new files get distinct names.

## Test inventory & grouping (read from `tests.rs`)

| Test fn | Primary module tested | Destination file |
|---|---|---|
| `static_scene_cache_populated_after_first_evaluate` | scene_eval (static subtree cache) | `tests_scene_eval.rs` |
| `static_scene_skips_frame_env` | scene_eval (`needs_frame_env`) | `tests_scene_eval.rs` |
| `test_for_iter_values_supports_tuple_literals` | property_lookup (`for_iter_values`) | `tests_property_lookup.rs` |
| `test_apply_modifier_stmt_supports_conditionals_statelessly` | modifier_exec / frame_env | `tests_modifiers.rs` |
| `test_colorscheme_primitive_declaration` | colorscheme | `tests_colorscheme.rs` |
| `test_colorscheme_let_declaration` | colorscheme | `tests_colorscheme.rs` |
| `test_colorscheme_inheritance` | colorscheme | `tests_colorscheme.rs` |
| `test_colorscheme_auto_cycle` | colorscheme | `tests_colorscheme.rs` |
| `test_runtime_text_recompilation` | scene_eval (text compiler cache) | `tests_scene_eval.rs` |
| `test_keyframe_scoped_variables_create_tracks` | frame_env / variable tracks | `tests_variable_tracks.rs` |
| `test_animated_scene_has_keyframes` | animation_track / build | `tests_animation_track.rs` |
| `test_keyframe_scoped_variables_injected_into_frame_env` | frame_env / variable tracks | `tests_variable_tracks.rs` |
| `test_reactive_binding_desugars_to_modifier` | build (reactive binding) | `tests_build.rs` |
| `test_hierarchical_assignment_target` | build / assignments | `tests_build.rs` |
| `graph_axes_invisible_before_fadein` | build (Graph, fade-in action) | `tests_build.rs` |
| `always_overrides_keyframes_warning` | build (diagnostics) | `tests_build_diagnostics.rs` |
| `always_overrides_keyframes_no_warning_without_track` | build (diagnostics) | `tests_build_diagnostics.rs` |
| `always_overrides_keyframes_no_warning_without_conflict` | build (diagnostics) | `tests_build_diagnostics.rs` |
| `absolute_position_on_layout_managed_child_warning` | build (diagnostics) | `tests_build_diagnostics.rs` |
| `absolute_position_on_layout_managed_child_no_warning_without_at` | build (diagnostics) | `tests_build_diagnostics.rs` |
| `equation_container_builds_with_fragment_children` | build (Equation/Fragment) | `tests_build.rs` |
| `equation_fragment_dot_path_assignment` | build (Equation/Fragment) | `tests_build.rs` |
| `test_container_metadata_gap_helpers` | `mod.rs` (gap/padding helpers) | `tests_container_helpers.rs` |
| `test_stack_align_start_and_end` | layout | `tests_layout.rs` |
| `test_baseline_alignment_via_layout_engine` | layout | `tests_layout.rs` |
| `test_percentage_child_sizing_row` | taffy_layout | `tests_taffy_layout.rs` |
| `test_min_max_constraints` | taffy_layout | `tests_taffy_layout.rs` |
| `test_parse_size_spec_from_property` | taffy_layout | `tests_taffy_layout.rs` |
| `test_keyframe_times_s_collects_all_fields` | `mod.rs` (`keyframe_times_s`) | `tests_keyframe_times.rs` |
| `test_keyframe_times_s_includes_highlight_fields` | `mod.rs` (`keyframe_times_s`) | `tests_keyframe_times.rs` |
| `test_keyframe_times_s_returns_unique_times` | `mod.rs` (`keyframe_times_s`) | `tests_keyframe_times.rs` |
| `test_keyframe_times_s_returns_seconds_not_milliseconds` | `mod.rs` (`keyframe_times_s`) | `tests_keyframe_times.rs` |
| `test_keyframe_times_s_includes_background_color` | `mod.rs` (`keyframe_times_s`) | `tests_keyframe_times.rs` |
| `test_keyframe_times_s_includes_filter_fields` | `mod.rs` (`keyframe_times_s`) | `tests_keyframe_times.rs` |
| `test_keyframe_times_s_includes_plot_param_tracks` | `mod.rs` (`keyframe_times_s`) | `tests_keyframe_times.rs` |
| `test_keyframe_times_s_empty_when_no_keyframes` | `mod.rs` (`keyframe_times_s`) | `tests_keyframe_times.rs` |
| `test_fixed_size_layout_still_works` | layout (legacy compat) | `tests_layout.rs` |

**Note:** `test_fixed_size_layout_still_works` has no `#[test]` attribute in
the current file (it's a free fn) — it is effectively dead. Flag this in the
plan; either re-add `#[test]` during the move or delete it. See Task 1.0.

## Design decision: keep a thin `tests.rs` aggregator

`mod.rs` declares `#[cfg(test)] mod tests;` pointing at the **file** `tests.rs`.
Rust file-modules cannot transparently re-export submodules declared in sibling
files unless `tests.rs` itself declares them. The cleanest approach that keeps
`mod.rs` untouched and avoids touching every sibling module:

- `tests.rs` becomes a thin aggregator that declares submodules, each backed by
  a sibling file `tests_<group>.rs`:
  ```rust
  // timeline/tests.rs  — test aggregator
  use super::*;

  #[cfg(test)]
  mod scene_eval;
  #[cfg(test)]
  mod property_lookup;
  // … etc
  ```
  Each `tests_<group>.rs` lives in `crates/animatix/src/timeline/tests/` — but
  file-modules with submodules require a directory. Rust 2018 allows
  `tests.rs` + `tests/` directory siblings, OR converting `tests.rs` into
  `tests/mod.rs`.

**Chosen layout (least churn, matches Rust conventions):**
1. Convert `tests.rs` → `tests/mod.rs` (move the file).
2. Add `tests/<group>.rs` files.
3. `tests/mod.rs` declares each `mod <group>;` and keeps shared helpers
   (`keyframe_times_s_timeline`) + the `use super::*;` / `use crate::ast::…`.

Wait — `mod.rs` line 1098 is `mod tests;`. With Rust 2018, `mod tests;` resolves
to **either** `tests.rs` **or** `tests/mod.rs`. Moving `tests.rs` →
`tests/mod.rs` is a pure rename; `mod.rs` is untouched. ✓

Each child test file then needs `use super::*;` (to get the aggregator's
re-exports, which themselves come from `use super::*;` in `tests/mod.rs`
re-exporting `crate::timeline::*`) **and** the `use crate::ast::{BinaryOp, Property};`.

To avoid repeating `use crate::ast::…` in every child, put it in
`tests/mod.rs` and have children do `use super::*;`. Since `tests/mod.rs` will
`use super::*;` (pulling timeline items) **and** `use crate::ast::{BinaryOp, Property};`,
children doing `use super::*;` get both. ✓

## Plan

### Task 1.0 — Audit dead test & confirm inventory
- **Files:** read-only on `crates/animatix/src/timeline/tests.rs`
- **Change:** Confirm `test_fixed_size_layout_still_works` (line ~1607) lacks
  `#[test]`. Decision: re-add `#[test]` during the move (it has real
  assertions and is the only legacy-layout regression). If the team prefers
  deletion, note it. **Default: re-add `#[test]`.**
- **Deps:** none
- **Verify:** `grep -n "fn test_fixed_size_layout_still_works" crates/animatix/src/timeline/tests.rs`
- **Complexity:** small

### Task 1.1 — Create `tests/` directory and move `tests.rs` → `tests/mod.rs`
- **Files:** 
  - create `crates/animatix/src/timeline/tests/mod.rs` (content = current `tests.rs`)
  - delete `crates/animatix/src/timeline/tests.rs`
- **Change:** Pure file move. `mod.rs:1098` (`mod tests;`) still resolves.
  Keep all test bodies in `tests/mod.rs` for this step — do NOT split yet.
- **Deps:** 1.0
- **Verify:** `cargo test -p animatix --no-run` compiles; `cargo test -p animatix` passes with the same count as before.
- **Complexity:** small

### Task 1.2 — Extract `tests_scene_eval.rs`
- **Files:** 
  - new `crates/animatix/src/timeline/tests/scene_eval.rs`
  - edit `crates/animatix/src/timeline/tests/mod.rs`
- **Change:** Move `static_scene_cache_populated_after_first_evaluate`,
  `static_scene_skips_frame_env`, `test_runtime_text_recompilation` into
  `scene_eval.rs`. File opens with `use super::*;`. Add
  `#[cfg(test)] mod scene_eval;` to `tests/mod.rs`.
- **Deps:** 1.1
- **Verify:** `cargo test -p animatix scene_eval` runs the 3 tests.
- **Complexity:** small

### Task 1.3 — Extract `tests_colorscheme.rs`
- **Files:** new `tests/colorscheme.rs`; edit `tests/mod.rs`
- **Change:** Move the 4 `test_colorscheme_*` tests. They construct AST with
  `Stmt::LetDecl`/`Construct`/`Config` and use `Property` (from
  `crate::ast`) — available via `use super::*;`.
- **Deps:** 1.1
- **Verify:** `cargo test -p animatix colorscheme`
- **Complexity:** small

### Task 1.4 — Extract `tests_modifiers.rs`
- **Files:** new `tests/modifiers.rs`; edit `tests/mod.rs`
- **Change:** Move `test_apply_modifier_stmt_supports_conditionals_statelessly`.
  Uses `load_standard_library`, `Timeline::build_frame_env_internal`,
  `apply_modifier_stmt`, `Value`, `BinaryOp`, `Stmt::Conditional`/`Assignment`.
  All via `use super::*;` + `super::*` re-export of `crate::ast::{BinaryOp, Property}`.
- **Deps:** 1.1
- **Verify:** `cargo test -p animatix modifiers`
- **Complexity:** small

### Task 1.5 — Extract `tests_property_lookup.rs`
- **Files:** new `tests/property_lookup.rs`; edit `tests/mod.rs`
- **Change:** Move `test_for_iter_values_supports_tuple_literals`. Uses
  `Environment::new`, `for_iter_values`, `Value`, `Expr::Tuple`.
- **Deps:** 1.1
- **Verify:** `cargo test -p animatix property_lookup`
- **Complexity:** small

### Task 1.6 — Extract `tests_variable_tracks.rs`
- **Files:** new `tests/variable_tracks.rs`; edit `tests/mod.rs`
- **Change:** Move `test_keyframe_scoped_variables_create_tracks` and
  `test_keyframe_scoped_variables_injected_into_frame_env`. These share heavy
  AST construction; consider a local `fn` helper inside the new file for the
  shared `Ellipse`+`freq`+`always` scaffold (optional, not required).
- **Deps:** 1.1
- **Verify:** `cargo test -p animatix variable_tracks`
- **Complexity:** small

### Task 1.7 — Extract `tests_build.rs`
- **Files:** new `tests/build.rs`; edit `tests/mod.rs`
- **Change:** Move `test_reactive_binding_desugars_to_modifier`,
  `test_hierarchical_assignment_target`, `graph_axes_invisible_before_fadein`,
  `equation_container_builds_with_fragment_children`,
  `equation_fragment_dot_path_assignment`. These use
  `animatix_syntax::parser::parse_source` — import path stays the same
  (`animatix_syntax::…` is a crate-level dep, available everywhere).
- **Deps:** 1.1
- **Verify:** `cargo test -p animatix build`
- **Complexity:** small

### Task 1.8 — Extract `tests_build_diagnostics.rs`
- **Files:** new `tests/build_diagnostics.rs`; edit `tests/mod.rs`
- **Change:** Move the 5 `always_overrides_keyframes_*` and
  `absolute_position_on_layout_managed_child_*` tests. They assert on
  `animatix_syntax::diagnostics::DiagnosticCode::*` — paths unchanged.
- **Deps:** 1.1
- **Verify:** `cargo test -p animatix build_diagnostics`
- **Complexity:** small

### Task 1.9 — Extract `tests_layout.rs` and `tests_taffy_layout.rs`
- **Files:** new `tests/layout.rs`, new `tests/taffy_layout.rs`; edit `tests/mod.rs`
- **Change:** 
  - `layout.rs`: `test_stack_align_start_and_end`,
    `test_baseline_alignment_via_layout_engine`, `test_fixed_size_layout_still_works`
    (re-add `#[test]` per Task 1.0).
  - `taffy_layout.rs`: `test_percentage_child_sizing_row`,
    `test_min_max_constraints`, `test_parse_size_spec_from_property`.
  - These use `crate::timeline::layout::ChildExtent`,
    `crate::timeline::taffy_layout::*`, `crate::timeline::{ContainerMetadata, LayoutEngine, LayoutType, PlacementMode}` — keep the local `use` blocks already present inside each test body.
- **Deps:** 1.1
- **Verify:** `cargo test -p animatix layout` and `cargo test -p animatix taffy_layout`
- **Complexity:** small

### Task 1.10 — Extract `tests_container_helpers.rs` and `tests_keyframe_times.rs`
- **Files:** new `tests/container_helpers.rs`, new `tests/keyframe_times.rs`; edit `tests/mod.rs`
- **Change:**
  - `container_helpers.rs`: `test_container_metadata_gap_helpers` (uses
    `gap_uniform`, `padding_uniform`).
  - `keyframe_times.rs`: all 8 `test_keyframe_times_s_*` tests **plus** the
    shared helper `fn keyframe_times_s_timeline() -> Timeline`. The helper
    mutates `timeline.background_color.keyframes_mut()` — keep it local to
    this file. Uses `AnimationTrack::new`, `ActorKindId`, `Easing`,
    `PropertyTrack`, `collect_all_keyframe_times` (implicitly via
    `Timeline::keyframe_times_s`).
- **Deps:** 1.1
- **Verify:** `cargo test -p animatix keyframe_times` and `cargo test -p animatix container_helpers`
- **Complexity:** small

### Task 1.11 — Final `tests/mod.rs` cleanup & full verification
- **Files:** `crates/animatix/src/timeline/tests/mod.rs`
- **Change:** After all extractions, `tests/mod.rs` should contain only:
  ```rust
  use super::*;
  use crate::ast::{BinaryOp, Property};

  #[cfg(test)] mod scene_eval;
  #[cfg(test)] mod colorscheme;
  #[cfg(test)] mod modifiers;
  #[cfg(test)] mod property_lookup;
  #[cfg(test)] mod variable_tracks;
  #[cfg(test)] mod build;
  #[cfg(test)] mod build_diagnostics;
  #[cfg(test)] mod layout;
  #[cfg(test)] mod taffy_layout;
  #[cfg(test)] mod container_helpers;
  #[cfg(test)] mod keyframe_times;
  ```
  Remove any now-unused `use` items. If `BinaryOp`/`Property` are no longer
  referenced in `mod.rs` itself (only in children, which do their own
  `use super::*;`), they can stay (children rely on them) — keep them.
- **Deps:** 1.2–1.10
- **Verify:** 
  - `cargo test -p animatix --no-fail-fast` — all tests pass.
  - `cargo test -p animatix 2>&1 | grep "test result"` — test count matches pre-split baseline (record baseline first).
  - `cargo check -p animatix` — 0 errors.
- **Complexity:** small

## Risks (Workstream 1)
- **Name collisions:** child module names (`build`, `layout`) mirror real
  timeline submodules — but they live under `tests::`, so `tests::build` ≠
  `timeline::build`. No collision. Still, if any child test does
  `use crate::timeline::build::*;` it resolves to the real submodule, not the
  test sibling. Current tests use fully-qualified `crate::timeline::layout::…`
  paths, so this is safe.
- **`#[cfg(test)]` propagation:** file modules declared inside a
  `#[cfg(test)] mod tests` automatically inherit `cfg(test)`; the explicit
  `#[cfg(test)]` on each `mod` line is belt-and-suspenders (harmless).
- **Test count regression:** must record baseline before Task 1.1.
- **The `keyframe_times_s_timeline` helper** clears
  `background_color.keyframes_mut()` — keep it co-located with the
  `keyframe_times_s` tests (Task 1.10), not in `mod.rs`.

---

# Workstream 2 — GUI registry adoption

## Goal
Replace three hand-maintained property field lists in the GUI with the
registry-driven APIs from `animatix::timeline` so the GUI never drifts from
the canonical `PROPERTY_REGISTRY`.

## Available registry APIs (confirmed in source)
From `crates/animatix/src/timeline/mod.rs` re-exports and
`dispatch.rs` / `property_registry.rs`:
- `collect_all_keyframe_times(track: &AnimationTrack) -> Vec<f64>` (mod.rs:256)
  — already registry-driven (iterates `allowed_property_indices(track.kind)`).
- `property_keyframe_times(track, field) -> Vec<u64>` (dispatch.rs:809)
- `property_has_keyframes(track, field) -> bool` (dispatch.rs:794)
- `property_keyframe_count(track, field) -> usize`
- `track.field_ref(field: ActorField) -> Option<TrackFieldRef>` (dispatch.rs:455)
  with `TrackFieldRef::evaluate_value(time_ms) -> Option<PropertyValue>` and
  `keyframe_times()`.
- `allowed_property_indices(kind) -> Vec<usize>` (property_registry.rs)
- `PROPERTY_REGISTRY: &[PropertySchema]` (sorted, binary-searchable)
- `lookup_property(name) -> Option<&'static PropertySchema>`
- `AnimationTrack::max_keyframe_time() -> Option<u64>` (dispatch.rs,
  registry-driven) — **already exists and iterates all PROPERTY_REGISTRY fields
  + plot_param_tracks**.

## Key reuse insight
`AnimationTrack::max_keyframe_time()` (in `dispatch.rs`, confirmed) already
computes the max keyframe time across **all** registry fields plus
`plot_param_tracks`. This is exactly what `document.rs::track_max_ms` does by
hand. The GUI can call `track.max_keyframe_time().unwrap_or(0)` directly.

## Field coverage audit

### `document.rs::track_max_ms` (lines ~747-769) — 23 fields
Hand-listed fields:
`position, motion_offset, rotation, scale, placement_mode, position_binding,
size, line_from, line_to, arc_angles, color, shape_type, opacity,
stroke_width, stroke_color, stroke_progress, fill_opacity, text_content,
text_paths, vector_paths, image, points`

**Missing from the hand-list vs. registry** (i.e. `track_max_ms` currently
under-reports duration for these): `head_size, line_cap, line_join,
font_family, font_size, font_weight, font_style, line_height, letter_spacing,
word_spacing, text_max_width, text_align, overflow, ascent, descent, baseline,
highlight_color, highlight_opacity, highlight_padding, highlight_radius,
morph_options, transform, layout_size, min_width, min_height, max_height,
filter_blur, filter_brightness, filter_contrast, filter_saturate,
filter_hue_rotate, filter_sepia, audio_source, audio_volume`.

`AnimationTrack::max_keyframe_time()` covers **all** of these (it iterates the
full `PROPERTY_REGISTRY`). So switching to it is both a simplification **and**
a bug fix (more accurate duration). This is intentional adoption, not a
regression — but flag the behavior change.

### `source_store.rs::push_kf_props` list (lines ~133-154) — 22 fields
Hand-listed: `position, motion_offset, rotation, scale, size, color, opacity,
stroke_width, stroke_color, stroke_progress, fill_opacity, text_content,
font_family, font_size, shape_type, line_from, line_to, arc_angles, points,
commands, layout_size, vector_paths`.

Missing vs. registry: `head_size, line_cap, line_join, font_weight,
font_style, line_height, letter_spacing, word_spacing, text_max_width,
text_align, overflow, ascent, descent, baseline, highlight_*, morph_options,
transform, min_*, max_height, filter_*, audio_*`. Same situation: registry
covers strictly more.

### `spreadsheet.rs` (lines ~430-501) — 10 properties
`SPREADSHEET_PROPERTIES` (line ~31) is already a curated const list:
`position, size, rotation, scale, opacity, color, stroke_width, stroke_color,
stroke_progress, fill_opacity`. The `get_property_gui_value` and
`has_property_track` functions hand-match these 10 names to track fields.

**This is intentionally a curated subset** (the comment at line ~24 says
"Only properties that are commonly used and visually meaningful are included
here."). So the registry migration here is NOT "enumerate everything" — it's
"stop hand-matching names to fields; use `track.field_ref(schema.field)` +
`schema.read_source.read()` to read values generically." The curated list of
10 names stays; the lookup becomes registry-driven.

## Plan

### Task 2.1 — `document.rs`: replace `track_max_ms` with `track.max_keyframe_time()`
- **Files:** `crates/animatix-gui/src/document.rs`
- **Change:**
  - Delete `latest_keyframe_ms` (line 739) and `track_max_ms` (lines 747-769).
  - In `timeline_duration_seconds` (line ~771), replace
    `.map(track_max_ms).max()` with
    `.map(|t| t.max_keyframe_time().unwrap_or(0)).max()`.
  - The `use animatix::timeline::{AnimationTrack, PropertyTrack, …}` import
    (line 7) — `AnimationTrack` stays; `PropertyTrack` may become unused if
    `latest_keyframe_ms` was its only user. Check and remove if dead.
- **Deps:** none
- **Verify:** `cargo test -p animatix-gui`; manually check that GUI duration
  display matches for a `.amx` file with animated `font_size` or `head_size`
  (previously under-reported).
- **Complexity:** small

### Task 2.2 — `source_store.rs`: replace `push_kf_props` list with registry iteration
- **Files:** `crates/animatix-gui/src/app/stores/source_store.rs`
- **Change:**
  - Keep the `push_kf_props` helper signature but drive it from the registry.
    Replace the 22 hand-listed `push_kf_props(&mut result, &track.X, "name")`
    calls with:
    ```rust
    let indices = animatix::timeline::allowed_property_indices(track.kind);
    for idx in indices {
        let schema = &animatix::timeline::PROPERTY_REGISTRY[idx];
        for ms in animatix::timeline::property_keyframe_times(track, schema.field) {
            result.push((ms, schema.name));
        }
    }
    result.sort_by_key(|(ms, _)| *ms);
    result.dedup_by(|a, b| a.0 == b.0);
    ```
  - This changes the `&'static str` name source from compile-time literals to
    `schema.name: &'static str` — type-compatible. ✓
  - **Important:** `property_keyframe_times` for group fields
    (`ActorField::PositionBindingGroup`, `VectorShapeGroup`, `PlotDomainGroup`,
    `ContainerLayoutGroup`, `NoStorage`) returns empty (no track), so they're
    harmless to include. `allowed_property_indices` filters by `applicable`,
    which already excludes `Never`-applicable props — good.
  - Remove the now-unused `KeyframeSource` trait + impl (lines ~183-194) if no
    other caller. Grep first.
- **Deps:** none (independent of 2.1)
- **Verify:** `cargo test -p animatix-gui`; verify the per-actor keyframe
  markers in the GUI timeline now show markers for `head_size`/`font_weight`
  animations that were previously invisible.
- **Complexity:** small

### Task 2.3 — `spreadsheet.rs`: drive `get_property_gui_value` + `has_property_track` via registry
- **Files:** `crates/animatix-gui/src/app/panels/inspector/spreadsheet.rs`
- **Change:**
  - Keep `SPREADSHEET_PROPERTIES` (curated 10-name list) — do NOT expand it.
    This is an intentional UI subset.
  - Rewrite `get_property_gui_value(track, prop_name, time_ms)` to:
    ```rust
    fn get_property_gui_value(track, prop_name, time_ms) -> Option<GuiPropertyValue> {
        let schema = animatix::timeline::lookup_property(prop_name)?;
        let pv = animatix::timeline::read_property_value(track, schema.field, time_ms)?;
        match pv {
            animatix::timeline::PropertyValue::Vec2(v) => {
                if prop_name == "size" { Some(GuiPropertyValue::Vec2([v[0]*2.0, v[1]*2.0])) }
                else { Some(GuiPropertyValue::Vec2(v)) }
            }
            animatix::timeline::PropertyValue::F32(v) => Some(GuiPropertyValue::Float(v)),
            animatix::timeline::PropertyValue::Color(v) | PropertyValue::Vec4(v) => Some(GuiPropertyValue::Color(v)),
            _ => None,
        }
    }
    ```
    Note: the existing code special-cases `size` to double (because the track
    stores half-size). Preserve that. `read_property_value` reads the raw
    `Size` field; for `size` we double. For other Vec2 props (`position`) we
    don't. This matches current behavior.
  - Rewrite `has_property_track(track, prop_name)` to:
    ```rust
    fn has_property_track(track, prop_name) -> bool {
        animatix::timeline::lookup_property(prop_name)
            .map(|schema| animatix::timeline::property_has_keyframes(track, schema.field))
            .unwrap_or(false)
    }
    ```
    Wait — current `has_property_track` checks `.is_some()` (track exists),
    not "has keyframes". `property_has_keyframes` checks count > 0 which is
    equivalent to "track exists with ≥1 keyframe". But a track field can be
    `Some` with zero keyframes? `PropertyTrack` always has ≥1 keyframe once
    created (via `ensure`). So `property_has_keyframes` ≈ `field_ref().is_some()`
    in practice. To be safe and semantically identical, use
    `track.field_ref(schema.field).is_some()` instead:
    ```rust
    fn has_property_track(track, prop_name) -> bool {
        animatix::timeline::lookup_property(prop_name)
            .map(|schema| track.field_ref(schema.field).is_some())
            .unwrap_or(false)
    }
    ```
    `field_ref` is public on `AnimationTrack` (dispatch.rs:455). Need to
    confirm it's exported — `TrackFieldRef` is re-exported via
    `pub use dispatch::{AnimationTrack, TrackFieldRef, TrackFieldMut};` in
    mod.rs. The method `field_ref` is `pub fn` on the impl. ✓
  - There's also a `format_property_value` / `read_property`-style display
    function around line ~430 (the `match prop_name` returning formatted
    strings). Audit it too — if it duplicates the field matching, route it
    through `read_property_value` + a `PropertyValue → String` formatter.
- **Deps:** none
- **Verify:** `cargo test -p animatix-gui`; open the spreadsheet panel in the
  GUI and confirm values render identically for Rect/Text/Line actors.
- **Complexity:** medium (need to handle the `size`×2 special case and audit
  the display formatter)

### Task 2.4 — Cross-check: confirm no intentionally-excluded fields regress
- **Files:** read-only audit across the 3 GUI files
- **Change:** Document that:
  - `document.rs` and `source_store.rs` now report **all** animated fields
    (superset of before) — this is desired (fixes under-reporting).
  - `spreadsheet.rs` stays curated to 10 properties by design.
- **Deps:** 2.1, 2.2, 2.3
- **Verify:** `cargo test -p animatix-gui --no-fail-fast`; manual GUI smoke
  test with `examples/20_feature_reel.amx`.
- **Complexity:** small

## Risks (Workstream 2)
- **`PropertyValue` enum shape:** `animatix::timeline::PropertyValue` (from
  `property_engine`) has variants `F32`, `Vec2`, `Vec4`, `Color`, `String`,
  `U32`, `Transform`, `PlacementMode`, `MorphOptions`, `PointList`,
  `CommandList`. The GUI's `GuiPropertyValue` has only `Vec2, Float, Color,
  Text, StringList, PointList`. The spreadsheet only curates 10 props that
  map to `Vec2/Float/Color`, so the `match` in Task 2.3 needs a `_ => None`
  fallback (already in plan). Confirm `PropertyValue::Color` and `Vec4` both
  exist (registry uses both — `Color` variant exists per
  `TrackFieldRef::evaluate_value` returning `PropertyValue::Color` for Vec4
  fields). The match must cover `Color | Vec4`.
- **`size` doubling:** must preserve; registry read returns half-size.
- **`field_ref` visibility:** confirmed `pub fn` on `AnimationTrack` impl in
  `dispatch.rs`; `TrackFieldRef` re-exported from `timeline::mod.rs`.
- **Behavior change in duration:** Task 2.1 makes `timeline_duration_seconds`
  return longer durations for files animating previously-unlisted fields.
  This is a fix, but could surprise users whose compositions were timed to
  the old (shorter) value. Flag in commit message.

---

# Workstream 3 — Callout and Legend primitives

## Goal
Add two new primitives — `Callout` (annotated arrow + label) and `Legend`
(swatches + labels) — following the existing primitive registration pattern.

## Architecture recap (from `primitives/mod.rs`)
Adding a primitive requires:
1. `primitives/<name>.rs` implementing `Primitive` trait.
2. `&<NAME>::CONST` added to the `PRIMITIVES` array in `primitives/mod.rs`.
3. A variant in `ActorKindId` (`timeline/actor_kind.rs`) — **required** because
   enums are matched exhaustively.
4. If it's a shape, a variant in `ShapeKind` (`timeline/actor_kind.rs`).
5. For animated properties: `ActorField` variants + `PROPERTY_REGISTRY` rows
   (`timeline/property_registry.rs`) + storage in the appropriate tier
   sub-struct (`timeline/animation_track.rs`).
6. `ActorKindId::from_type_name` works automatically via `find_primitive`.
7. `actor_kind_registry` / `ActorKindMeta` auto-generated from `PRIMITIVES`.

No parser changes needed — the parser uses `type_ident()` (uppercase ident)
and `find_primitive(ty)` for dispatch. New type names are picked up
automatically. ✓ (confirmed: `actor_kind.rs::from_type_name` calls
`find_primitive`).

## Design decisions

### Callout — composite shape (arrow shaft + text label)
A `Callout` is fundamentally an `Arrow` with an attached text label rendered
at a configurable offset. Two viable architectures:

**Option A — New `ShapeKind::Callout` variant (full shape integration).**
Add `VectorShapeState::Callout(CalloutState)` and a `ShapeKind::Callout`.
Pro: morph-compatible, unified with shape pipeline. Con: text rendering inside
the shape pipeline is awkward — shapes emit `VelloPath`s, not `RenderCommand::Text`.
The shape path would need a separate text pass. The existing `Arrow` is
stroke-only; callout needs fill (label bg) + text. This fights the shape
abstraction.

**Option B — New `ActorKindId::Callout` non-shape primitive (recommended).**
Like `Equation`/`Fragment`: a dedicated `ActorKindId` variant, not a
`ShapeKind`. The primitive's `evaluate()` returns multiple `RenderCommand`s:
`Paths` (arrow shaft + head + optional label-background rect) and `Text`
(label glyphs). This matches how `Equation` composes rect-highlight + text.
Reuses arrow geometry by calling `ARROW.render(...)` or factoring arrow
geometry into a shared helper.

**Chosen: Option B.** Callout is a composite actor, not a pure shape. It
composes arrow geometry + text. This mirrors `Equation` (container-like
composite) and lets us emit `RenderCommand::Paths` + `RenderCommand::Text`.

### Legend — container of swatches + labels
A `Legend` is a layout container (`Row`/`Col`) whose children are swatch+label
pairs. Two viable architectures:

**Option A — New container primitive `Legend` with custom child rendering.**
`ActorKindId::Legend`, `is_container() = true`, children are `Swatch`+`Text`
pairs. Pro: flexible, composable. Con: requires a new `Swatch` child primitive
too, plus container metadata wiring. Large surface.

**Option B — New non-container `Legend` primitive that renders swatches+labels
from a data property.** The legend takes a `entries` property (list of
`(color, label)` tuples) and renders N swatches + N text labels in a row/col.
No children, no layout container integration. Pro: self-contained, simple
build, simple render. Con: not composable with the layout system (but legends
are usually leaf UI elements, so this is fine).

**Chosen: Option B.** A `Legend` is a leaf actor with an `entries` data
property. It renders swatches (rects) + labels (text) using the existing text
compiler. This avoids container-metadata complexity and a new `Swatch`
primitive. The legend's own `at`/`size` position it; internal layout is
computed in `evaluate()`.

This means `Legend` needs text compilation at frame time — it will use
`TextCompileCtx` like `Text`/`Equation`.

## Syntax design (.amx DSL)

### Callout
```animatix
c1: Callout, from: (100, 100), to: (300, 200), label: "peak", label_at: (320, 200), head_size: 12, color: accent.primary
```
Properties:
- `from`, `to` — arrow endpoints (Vec2) — reuse `ActorField::LineFrom`/`LineTo`.
- `head_size` — arrowhead size (f32) — reuse `ActorField::HeadSize`.
- `label` — the text string (String) — **new field** `ActorField::CalloutLabel`
  OR reuse `ActorField::TextContent`. Reusing `TextContent` lets the existing
  text-recompile assignment path (`recompile_text_at_assignment`) work for
  `label` assignments. **Reuse `TextContent`.**
- `label_at` — label anchor position (Vec2) — **new field**
  `ActorField::CalloutLabelAt`. Distinct from `at` (actor origin) because the
  label sits at the arrow tip area.
- `label_offset` — (Vec2) optional offset from `label_at`, default (8, -8).
  Could fold into `label_at`. Keep simple: just `label_at`.
- `font_size`, `font_family`, `color`, `stroke` — inherited from text/style
  tiers via registry `applicable`.

### Legend
```animatix
leg: Legend, at: (100, 800), entries: {("red", "high"), ("yellow", "mid"), ("green", "low")}, font_size: 24, direction: "row"
```
Properties:
- `entries` — list of (color, label) pairs. **New value type** needed, OR
  parse as `PointList`-like. Simplest: parse as a tuple-list expression at
  build time and store as a build-time-only `BuildTimeOnly` property on
  `NoStorage`, then bake the swatch colors + labels into... where? They need
  to be animatable-ish (or at least re-readable at frame time). 

  **Refined approach:** Store `entries` as a **new `ActorField::LegendEntries`
  track of type `Vec<(String, String)>`** (color-name, label). Add a new
  `ValueType::EntryList`. This requires:
  - new `PropertyValue::EntryList(Vec<(String,String)>)` variant,
  - `Interpolate` impl (piecewise-constant like `PointList`),
  - `TrackFieldRef::EntryList` variant + dispatch in `field_ref`/`field_mut`,
  - parser support for the tuple-list literal already exists
    (`Expr::Tuple` of `Expr::Tuple`s) — evaluation via `for_iter_values`-style
    unwrapping in the primitive's `build()`.

  This is the largest part of Workstream 3. To reduce risk, consider a
  **Phase 1**: non-animated entries stored as a build-time `Vec` on the track
  (a non-registry field on a new `LegendTracks` sub-struct), rendered from
  that struct. Animation of `entries` is a later phase. **Recommend Phase 1:
  static entries on a sub-struct.**

- `direction` — "row" | "col" (String) — build-time config on sub-struct.
- `font_size`, `font_family`, `color` — text styling via registry.
- `swatch_size` — (f32) size of each color swatch. New field
  `ActorField::LegendSwatchSize` (f32, animated) — or build-time on sub-struct.
  Make it animated (registry field) since it's a simple f32.
- `gap` — reuse... `gap` is currently `ContainerLayoutGroup`-only. For a
  non-container, add `ActorField::LegendGap` (f32) or reuse a style field.
  Simplest: new `ActorField::LegendGap` (f32, animated).

## Plan

### Task 3.1 — Add `ActorKindId` variants
- **Files:** `crates/animatix/src/timeline/actor_kind.rs`
- **Change:** Add `Callout` and `Legend` variants to `ActorKindId` enum.
  Update `ActorKindId::from_type_name` — no change needed (it delegates to
  `find_primitive`). Update any exhaustive `match` over `ActorKindId` that
  lacks a `_` arm. Grep for `match.*kind` / `ActorKindId::` across the crate
  to find compile-breakages; most use `_ =>` fallback.
- **Deps:** none
- **Verify:** `cargo check -p animatix` (will fail until primitives exist,
  but surfaces all match sites to fix).
- **Complexity:** small

### Task 3.2 — Add `ActorField` variants + registry rows for Callout
- **Files:** `crates/animatix/src/timeline/property_registry.rs`
- **Change:**
  - Add `ActorField::CalloutLabelAt` (Vec2).
  - Add registry rows (keep sorted by name):
    - `label` → reuse `TextContent` field, `Applicable::ActorKinds(&[A::Callout, A::Text, A::Code, A::Typst])` — but `text`/`code`/`math` already cover Text/Code/Typst. To avoid collision, make `label` applicable only to `Callout` and map to `ActorField::TextContent` (alias). Add:
      `schema!("label", ValueType::String, F::ASSIGNABLE_A, ActorField::TextContent, None, Applicable::ActorKinds(&[A::Callout]), |_| PropertyValue::String(String::new()))`
    - `label_at` → `ActorField::CalloutLabelAt`, `Applicable::ActorKinds(&[A::Callout])`, Vec2, default `[0.0, 0.0]`.
  - `from`/`to`/`head_size` already exist in the registry
    (`Applicable::ShapeKinds(&[S::Line, S::Arrow])` for from/to;
    `ShapeKinds(&[S::Arrow])` for head_size). Callout is NOT a `ShapeKind`,
    so these don't apply. **Must widen applicability** to include `A::Callout`:
    change `from`/`to` to `Applicable::ActorKinds(&[A::Callout])` ∪ shapes.
    But `Applicable` has no union. Options: (a) add a new `Applicable` variant
    `ShapesOrCallout`, or (b) change `from`/`to` to `Everything` (too broad),
    or (c) add `Callout` to a new combined variant. Cleanest: extend the
    `Applicable` enum with `ShapesPlusCallout` (and `ArrowsPlusCallout` for
    head_size). Or simpler: add a `Callout`-specific duplicate... no, names
    must be unique in the registry.

    **Decision:** widen the existing `from`/`to`/`head_size` schema
    `applicable` by adding new `Applicable` variants:
    - `LineArrowOrCallout` for `from`/`to`
    - `ArrowOrCallout` for `head_size`
    Implement `includes()` for both.
  - Add `CalloutLabelAt` to `ActorField::default_value()` → `Vec2([0.0, 0.0])`.
  - Add storage: new field on `GeometryTracks` (`callout_label_at: Option<PropertyTrack<[f32;2]>>`) OR a new `CalloutTracks` sub-struct. Since it's one field, add to `GeometryTracks` for now. Add to `AnimationTrack::new()` default and `Clone` (automatic via `#[derive]`).
  - Add `field_ref`/`field_mut` arms for `CalloutLabelAt` in `dispatch.rs`
    mapping to `geometry.callout_label_at` → `TrackFieldRef::Vec2`.
- **Deps:** 3.1
- **Verify:** `cargo test -p animatix` (registry sorted test must still pass).
- **Complexity:** medium

### Task 3.3 — Create `primitives/callout.rs`
- **Files:** new `crates/animatix/src/primitives/callout.rs`; edit `primitives/mod.rs`
- **Change:**
  - Implement `CalloutPrimitive` with `CONST: CalloutPrimitive`.
  - `type_name = "Callout"`, `category = ActorCategory::Shape` (it's
    shape-like visually) — but it emits Text too. `ActorCategory::Shape` is
    fine; `Equation`/`Fragment` don't use a special category. Keep `Shape`.
    Actually `is_shape()` should be `false` (it's not a pure vector shape;
    it composes). Set `is_shape() = false`. `category = Shape` still works
    for palette grouping.
  - `kind_id() = ActorKindId::Callout`.
  - `build()` — call `ctx.timeline.process_actor_decl` path? No — shapes go
    through `build/actor.rs::process_actor_decl` which handles
    `primitive.is_shape()`. Since `is_shape() = false`, it dispatches via
    `ActorKind` (actor_kind.rs). So `build()` must populate the track itself:
    create the track, set `kind`, insert keyframes for `from`/`to`/
    `head_size`/`label`/`label_at`/`color`/`stroke`. This duplicates logic
    from `process_actor_decl`. 

    **Better:** factor the common "insert shape keyframes" into a reusable
    `Timeline` method, OR — simpler — make `Callout` a shape variant after
    all. Reconsider Option A.

    **Revised decision:** Given the build-pipeline coupling, the lowest-risk
    path is to make `Callout` go through the shape build pipeline by setting
    `is_shape() = true` and adding `ShapeKind::Callout` + `VectorShapeState::Callout`.
    The text label is rendered in `evaluate()` (which returns
    `RenderCommand`), NOT in `render()` (which returns `VelloPath`). The
    shape pipeline builds the arrow shaft/head paths; `evaluate()` adds the
    text. This is exactly how `Arrow` works (`render()` for shape,
    `evaluate()` for frame-time composition) — `Arrow.evaluate()` already
    exists and calls `evaluate_shape_render`. We extend Callout's `evaluate()`
    to also emit a `RenderCommand::Text`.

    So **revise Task 3.1/3.2**: add `ShapeKind::Callout`, add
    `VectorShapeState::Callout(CalloutState)` with `{ from, to, head_size,
    label_at }`, and the primitive is a shape. `label` (text content) still
    maps to `ActorField::TextContent` (stored in `text.text_content`), and
    `evaluate()` compiles it via `TextCompileCtx` like `Text` does.

    This means Task 3.2's `Applicable` widening for `from`/`to`/`head_size`
    becomes `ShapeKinds(&[S::Line, S::Arrow, S::Callout])` etc. — much
    cleaner, no new `Applicable` variants.
  - `render()` — draw arrow shaft + head (reuse `ARROW.render` geometry by
    factoring, or duplicate the ~40 lines from `arrow.rs`). Recommend
    factoring: extract `build_arrow_path(from, to, head_size) -> BezPath` into
    `primitives/arrow.rs` or `timeline/shapes` and call from both.
  - `evaluate()` — call `evaluate_shape_render` for the arrow, then compile
    label text via `evaluate_text_paths` (from `primitives/mod.rs`) and push
    `RenderCommand::Text`. Return `Some(vec![Paths{…}, Text{…}])`.
  - `default_props()` — `from`, `to`, `head_size`, `label`, `label_at`, `color`.
  - `apply_property()` — handle `label_at` (Vec2). `from`/`to`/`head_size`
    handled by the generic shape property engine via registry group
    (`VectorShapeState` group) — but `Callout` isn't in that group's handler.
    Add `Callout` to `GroupHandlerId::VectorShapeState` resolution in
    `property_groups.rs` (wherever `from`/`to`/`head_size` are written to the
    shape state). Grep for `VectorShapeState::Arrow` writes.
  - Add `&CALL` to `PRIMITIVES` in `primitives/mod.rs`. Add `mod callout; pub use callout::CALL;`.
- **Deps:** 3.1, 3.2 (revised to shape-based)
- **Verify:** `cargo test -p animatix`; add a unit test in `callout.rs` that
  builds a Callout from AST and checks `track.shape.line_from` is set.
- **Complexity:** large

### Task 3.4 — Add `ShapeKind::Callout` + `VectorShapeState::Callout` + shape plumbing
- **Files:** 
  - `crates/animatix/src/timeline/actor_kind.rs` (add `ShapeKind::Callout`)
  - `crates/animatix/src/timeline/shapes/mod.rs` (add `CalloutState` struct + `VectorShapeState::Callout` variant + `new()`/`size_mut()`/`extract` arms)
  - `crates/animatix/src/timeline/shapes/mod.rs` `shape_type_for_actor` / `build_vector_shape_vello_path` (add Callout arm — delegate to arrow geometry)
  - `crates/animatix/src/timeline/build/shape.rs` + `build/actor.rs` (add Callout to `match &mut vector_shape_state` arms that init `from`/`to`)
  - `crates/animatix/src/timeline/shapes/mod.rs` `apply_vector_shape_property` (add Callout arm for `from`/`to`/`head_size`/`label_at`)
- **Change:** `CalloutState { from: [f32;2], to: [f32;2], head_size: f32, label_at: [f32;2] }`. All shape-pipeline match arms get a `VectorShapeState::Callout(c) => …` case mirroring `Arrow`.
- **Deps:** 3.1
- **Verify:** `cargo check -p animatix` compiles; `cargo test -p animatix shapes`.
- **Complexity:** medium

### Task 3.5 — Callout rendering: factor arrow geometry
- **Files:** `crates/animatix/src/primitives/arrow.rs`, new `primitives/callout.rs`
- **Change:** Extract `pub fn build_arrow_bez_path(from: [f32;2], to: [f32;2], head_size: f32) -> kurbo::BezPath` from `ArrowPrimitive::render`. Use in both `Arrow::render` and `Callout::render`. Keep `Arrow` behavior identical.
- **Deps:** 3.4
- **Verify:** `cargo test -p animatix arrow`; visual diff of an existing Arrow example.
- **Complexity:** small

### Task 3.6 — Callout `evaluate()` with text label
- **Files:** `primitives/callout.rs`
- **Change:** Implement `evaluate()` to:
  1. Sample `from`/`to`/`head_size`/`label_at`/`color`/`stroke_color`/`stroke_width` from the track (with overrides).
  2. Build arrow `VelloPath` via `build_arrow_bez_path`.
  3. Compile `label` text via `evaluate_text_paths(ctx, text_ctx, TextKind::Text, 48.0)`.
  4. Return `Some(vec![RenderCommand::Paths{…}, RenderCommand::Text{…}])`.
  Note: the text transform — `RenderCommand::Text` is executed with the
  actor's `local_transform`. The label should appear at `label_at` in the
  actor's local space. Since `local_transform` already includes the actor's
  `at`/position, `label_at` is relative to the actor origin. The text
  compiler produces centered glyph paths; to place at `label_at` we need an
  extra translate. `RenderCommand::execute` applies `*transform` uniformly.
  Options: (a) bake `label_at` into the glyph paths (translate them before
  returning), or (b) add a transform field to `RenderCommand::Text`.
  Simplest (a): translate each glyph path by `label_at` in `evaluate()`
  before wrapping in `RenderCommand::Text`. Use `kurbo::Affine::translate`.
- **Deps:** 3.3, 3.5
- **Verify:** Render a `.amx` with a Callout in the GUI; verify arrow + label
  both appear and label moves when `label_at` is animated.
- **Complexity:** medium

### Task 3.7 — Callout tests + docs
- **Files:** 
  - `crates/animatix/src/primitives/callout.rs` (inline `#[cfg(test)] mod tests`)
  - `examples/callout.amx` (new demo)
  - `docs/spec.md` (add Callout to primitive list, §9)
- **Change:** Tests: (1) parse + build a Callout, assert track fields; (2)
  `evaluate()` returns 2 commands (Paths + Text); (3) `label_at` animation
  moves the label. Spec: add `Callout` row with `from, to, head_size, label,
  label_at`.
- **Deps:** 3.6
- **Verify:** `cargo test -p animatix callout`; `cargo run -p animatix-gui -- examples/callout.amx`.
- **Complexity:** small

### Task 3.8 — Legend: `ActorKindId::Legend` + sub-struct + fields
- **Files:** 
  - `crates/animatix/src/timeline/actor_kind.rs` (add `ActorKindId::Legend`)
  - `crates/animatix/src/timeline/animation_track.rs` (add `LegendTracks` sub-struct: `entries: Vec<(String,String)>` (build-time, non-animated), `direction: String`, `swatch_size: f32`, `gap: f32`)
  - `crates/animatix/src/timeline/dispatch.rs` (add `legend: LegendTracks` field to `AnimationTrack`, init in `new()`)
  - `crates/animatix/src/timeline/property_registry.rs` (add `ActorField::LegendSwatchSize`, `LegendGap`, `LegendDirection` + registry rows; `entries` is `BuildTimeOnly`/`NoStorage` since it's a Vec on the sub-struct, parsed in `build()`)
- **Change:** `LegendTracks` holds build-time-baked entries + a few animated
  f32/String fields. `entries` parsed from the tuple-list expression in
  `Legend::build()` and stored on `track.legend.entries`.
- **Deps:** 3.1
- **Verify:** `cargo check -p animatix`.
- **Complexity:** medium

### Task 3.9 — Create `primitives/legend.rs`
- **Files:** new `crates/animatix/src/primitives/legend.rs`; edit `primitives/mod.rs`
- **Change:**
  - `LegendPrimitive` + `const LEGEND`.
  - `type_name = "Legend"`, `category = ActorCategory::Shape` (visual), `is_shape() = false`, `kind_id = ActorKindId::Legend`.
  - `build()` — create track, set `kind = Legend`, parse `entries` expression
    (expect `Expr::Tuple` of `Expr::Tuple([color_expr, label_expr])`), evaluate
    each via `evaluate_expr_with_lookup_diagnostic` → store `Vec<(String,String)>`
    on `track.legend.entries`. Insert keyframes for `swatch_size`/`gap`/
    `font_size`/`color` via the generic engine (or manually).
  - `evaluate()` — for each entry: build a swatch `VelloPath` (rect of
    `swatch_size`) at the accumulated x/y offset, compile the label text via
    `evaluate_text_paths`, translate glyph paths to the swatch's right.
    Accumulate into `Vec<RenderCommand>` (alternating `Paths` + `Text`).
    Direction row → horizontal accumulation; col → vertical.
  - `default_props()` — `entries: {("red","A"),("blue","B")}`, `at`, `font_size: 24`, `swatch_size: 16`, `gap: 8`, `direction: "row"`.
  - Add `&LEGEND` to `PRIMITIVES`.
- **Deps:** 3.8
- **Verify:** `cargo test -p animatix legend`.
- **Complexity:** large

### Task 3.10 — Legend tests + docs
- **Files:** `primitives/legend.rs` (inline tests), `examples/legend.amx`, `docs/spec.md`
- **Change:** Tests: (1) parse `entries` tuple-list → correct `Vec`; (2)
  `evaluate()` returns `2*N` commands for N entries; (3) direction col →
  vertical layout. Spec: add `Legend` row with `entries, direction,
  swatch_size, gap`.
- **Deps:** 3.9
- **Verify:** `cargo test -p animatix legend`; GUI render of `examples/legend.amx`.
- **Complexity:** small

### Task 3.11 — Update registry-completeness tests
- **Files:** `crates/animatix/src/primitives/mod.rs` (the `every_kind_id_has_meta` test), `crates/animatix/src/timeline/animation_track.rs` (`actor_kind_registry_is_complete` test)
- **Change:** Add `ActorKindId::Callout` (if not a `ShapeKind`) and
  `ActorKindId::Legend` to the enumerated lists in these tests. If Callout is
  a `ShapeKind`, add `ShapeKind::Callout` to the shape-kinds list instead.
- **Deps:** 3.3, 3.9
- **Verify:** `cargo test -p animatix` (the registry completeness tests pass).
- **Complexity:** small

## Risks (Workstream 3)
- **Callout build-pipeline coupling:** The shape build pipeline
  (`build/actor.rs::process_actor_decl`) is the path of least resistance for
  any shape-like actor. Going against it (non-shape `Callout`) duplicates
  ~200 lines of track-init logic. The plan resolves this by making Callout a
  `ShapeKind` — but then `VectorShapeState` gains a variant that must be
  handled in ~6 match sites. This is mechanical but spread across files.
  **Mitigation:** Task 3.4 enumerates all sites; grep `VectorShapeState::`
  to find them.
- **Text transform in `RenderCommand::Text`:** The label placement via
  path-translation (Task 3.6 option a) is a hack. If Callout and Legend both
  need it, consider adding `RenderCommand::Text { paths, transform_offset:
  [f32;2] }` — but that changes the enum and `execute()`. Keep the hack for
  Phase 1; revisit if more text-composite primitives emerge.
- **`entries` parsing:** The tuple-list `(("red","A"),("blue","B"))` parses as
  `Expr::Tuple(vec![Expr::Tuple(vec![Expr::Str, Expr::Str]), …])`. Color
  entries may also be `Expr::Ident("accent.primary")` or `Expr::Tuple(Num×3)`.
  The `build()` must resolve color expressions via `parse_color_in_env` →
  store the resolved color string OR the raw expression. Storing the raw
  color name string is simplest; rendering resolves it. But colorschemes can
  change at runtime? No — colorscheme is build-time. Resolve to a color
  string/key at build time and store the key; resolve to `[f32;4]` at
  frame-time via `timeline.colorscheme`. Store as `Vec<([f32;4], String)>`?
  Then it's not animatable. For Phase 1 static entries, store
  `Vec<([f32;4], String)>` resolved at build. Simpler. Update Task 3.8/3.9
  accordingly: `entries: Vec<([f32;4], String)>`.
- **`Applicable` enum exhaustiveness:** Adding Callout to `from`/`to`/
  `head_size` applicability means editing those 3 registry rows' `applicable`
  field. Since Callout is a `ShapeKind`, this is
  `ShapeKinds(&[S::Line, S::Arrow, S::Callout])` etc. — no new enum variants.
- **GUI registry adoption interaction:** Workstream 2 makes the GUI
  registry-driven. New Callout/Legend properties (Callout: `label`, `label_at`;
  Legend: `swatch_size`, `gap`) will automatically appear in the GUI
  keyframe lists and spreadsheet (if applicable). The spreadsheet's curated
  10-property list won't show them unless added — that's fine. Flag that
  Workstream 2 should land before Workstream 3 to avoid hand-list
  re-maintenance.
- **`tree-sitter-animatix` grammar:** New type names `Callout`/`Legend` are
  uppercase idents — the grammar already tokenizes these as type idents. No
  grammar change needed unless we want dedicated highlight keywords (optional).

---

# Cross-workstream ordering

Recommended execution order (minimizes rework):

1. **Workstream 1** (test split) — purely mechanical, no API change. Do first
   to reduce noise in later diffs.
2. **Workstream 2** (GUI registry adoption) — small, high-value, and should
   land before Workstream 3 so new primitives' properties are auto-exposed.
3. **Workstream 3** (Callout + Legend) — largest; benefits from a stable
   registry-driven GUI.

Within Workstream 3, do Callout (3.1–3.7) before Legend (3.8–3.10) because
Callout exercises the shape-pipeline extension (informs Legend's non-shape
approach). Task 3.11 (registry tests) lands last.

## Final verification (all workstreams)
```
cargo check                                          # 0 errors
cargo test --no-fail-fast                            # all passing
cargo test -p animatix-gui --no-fail-fast
cargo run -p animatix-gui -- examples/callout.amx    # renders
cargo run -p animatix-gui -- examples/legend.amx     # renders
```
