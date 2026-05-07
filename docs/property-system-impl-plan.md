# Unified Property System — Implementation Plan (v2)

## Status

DRAFT — awaiting implementation.

---

## Goal

Eliminate the ~8-file, ~30-line-per-property boilerplate required to add an animated property. Moving from the current manual struct + scattered match arms to a single-definition system: one `#[prop]` attribute per property generates the struct field, typed accessors, registry entry, and all generic dispatch logic.

**Non-goal:** Runtime property registration. The property set is architecturally closed (intrinsic to the rendering model). No plugin system exists, and the design philosophy is *"extensibility through declarative data, not executable code."*

**Consequence:** We reject `HashMap<String, TrackValue>` in favor of a zero-cost struct with a compile-time-generated registry. HashMap lookups in scene evaluation (every frame, every actor, ~16 properties) would introduce ~50–100ns per read with no compensating benefit.

---

## Architecture: Manual Struct + `#[derive(TrackProperties)]`

### Core Principle

Keep `AnimationTrack` as an ordinary Rust struct with typed `Option<PropertyTrack<T>>` fields. Use a proc derive macro that reads `#[prop(...)]` attributes on each field and generates:

1. `PropertyKey` enum — compile-time property identifiers
2. `PROPERTIES` static registry — metadata for generic code (inspector, edits, serialization)
3. Enum-dispatch methods — `match` on `PropertyKey` for behavior (display, apply, collect keyframes)
4. Convenience accessors — `track.position(time_ms)` for hot paths

The struct remains standard Rust; only auxiliary code is generated. Full rust-analyzer support is preserved.

---

## Core Types (No Changes from Current System)

### `PropertyTrack<T>`

```rust
#[derive(Clone)]
pub struct PropertyTrack<T> {
    pub keyframes: BTreeMap<u64, (T, Easing)>,
    pub default_value: T,
}
```

Already generic over `T: Interpolate + Clone`. No changes.

### `TrackAccessor<T>`

```rust
pub trait TrackAccessor<T: Interpolate + Clone> {
    fn get(&self, time_ms: u64, default: T) -> T;
    fn ensure(&mut self, default: T) -> &mut PropertyTrack<T>;
    fn last(&self, default: T) -> T;
    fn last_time(&self) -> Option<u64>;
    fn has_keyframe_at(&self, time_ms: u64) -> bool;
}
```

Implemented for `Option<PropertyTrack<T>>`. No changes.

### `Interpolate`

```rust
pub trait Interpolate {
    fn interpolate(&self, other: &Self, t: f32) -> Self;
}
```

Existing impls: `f32`, `[f32; 2]`, `[f32; 4]`, `u32`, `PlacementMode`, `SceneAnchor`, `PositionBinding`, `MorphOptions`, `Vec<TextPath>`, `Vec<VelloPath>`, `Option<SceneImage>`, `String`, `Vec<String>`, `Vec<[f32; 2]>`. No changes.

---

## The `AnimationTrack` Struct (Manual)

```rust
// crates/animatix/src/timeline/track.rs

#[derive(Clone, TrackProperties)]
pub struct AnimationTrack {
    pub label: String,

    // ── Transform ──────────────────────────
    #[prop(name = "position", source = "at", group = "Transform", default = [0.0, 0.0])]
    pub position: Option<PropertyTrack<[f32; 2]>>,

    #[prop(name = "motion_offset", source = "motion_offset", group = "Transform", default = [0.0, 0.0])]
    pub motion_offset: Option<PropertyTrack<[f32; 2]>>,

    #[prop(name = "rotation", source = "rotation", group = "Transform", default = 0.0)]
    pub rotation: Option<PropertyTrack<f32>>,

    #[prop(name = "scale", source = "scale", group = "Transform", default = 1.0)]
    pub scale: Option<PropertyTrack<f32>>,

    // ── Shape ──────────────────────────────
    #[prop(name = "shape_type", source = "shape", group = "Shape", default = ShapeType::Rect)]
    pub shape_type: Option<PropertyTrack<ShapeType>>,

    #[prop(name = "line_from", source = "line_from", group = "Shape", default = [0.0, 0.0])]
    pub line_from: Option<PropertyTrack<[f32; 2]>>,

    #[prop(name = "line_to", source = "line_to", group = "Shape", default = [0.0, 0.0])]
    pub line_to: Option<PropertyTrack<[f32; 2]>>,

    #[prop(name = "arc_angles", source = "arc_angles", group = "Shape", default = [0.0, 0.0])]
    pub arc_angles: Option<PropertyTrack<[f32; 2]>>,

    #[prop(name = "points", source = "points", group = "Shape", default = vec![])]
    pub points: Option<PropertyTrack<Vec<[f32; 2]>>>,

    // ── Style ──────────────────────────────
    #[prop(name = "color", source = "color", group = "Style", default = [1.0, 1.0, 1.0, 1.0])]
    pub color: Option<PropertyTrack<[f32; 4]>>,

    #[prop(name = "opacity", source = "opacity", group = "Style", default = 1.0)]
    #[prop(widget = FloatSlider { min = 0.0, max = 1.0 })]
    pub opacity: Option<PropertyTrack<f32>>,

    #[prop(name = "stroke_width", source = "stroke_width", group = "Style", default = 0.0)]
    pub stroke_width: Option<PropertyTrack<f32>>,

    #[prop(name = "stroke_color", source = "stroke_color", group = "Style", default = [1.0, 1.0, 1.0, 1.0])]
    pub stroke_color: Option<PropertyTrack<[f32; 4]>>,

    #[prop(name = "stroke_progress", source = "stroke_progress", group = "Style", default = 1.0)]
    #[prop(widget = FloatSlider { min = 0.0, max = 1.0 })]
    pub stroke_progress: Option<PropertyTrack<f32>>,

    #[prop(name = "fill_opacity", source = "fill_opacity", group = "Style", default = 1.0)]
    #[prop(widget = FloatSlider { min = 0.0, max = 1.0 })]
    pub fill_opacity: Option<PropertyTrack<f32>>,

    // ── Content ────────────────────────────
    #[prop(name = "text_content", source = "text", group = "Content", default = String::new())]
    pub text_content: Option<PropertyTrack<String>>,

    #[prop(name = "text_paths", source = "text_paths", group = "Content", default = vec![])]
    #[prop(widget = ReadOnly)]
    pub text_paths: Option<PropertyTrack<Vec<TextPath>>>,

    #[prop(name = "vector_paths", source = "vector_paths", group = "Content", default = vec![])]
    #[prop(widget = ReadOnly)]
    pub vector_paths: Option<PropertyTrack<Vec<VelloPath>>>,

    #[prop(name = "image", source = "image", group = "Content", default = None)]
    #[prop(widget = ReadOnly)]
    pub image: Option<PropertyTrack<Option<SceneImage>>>,

    // ── Layout ─────────────────────────────
    #[prop(name = "size", source = "size", group = "Layout", default = DEFAULT_LAYOUT_HALF_SIZE)]
    #[prop_custom(apply = apply_size_edit, display = display_size_full)]
    pub size: Option<PropertyTrack<[f32; 2]>>,

    #[prop(name = "placement_mode", source = "placement_mode", group = "Layout", default = PlacementMode::Absolute)]
    #[prop(widget = ReadOnly)]
    pub placement_mode: Option<PropertyTrack<PlacementMode>>,

    #[prop(name = "position_binding", source = "position_binding", group = "Layout", default = PositionBinding::None)]
    #[prop(widget = ReadOnly)]
    pub position_binding: Option<PropertyTrack<PositionBinding>>,

    #[prop(name = "morph_options", source = "morph_options", group = "Layout", default = MorphOptions::default())]
    pub morph_options: Option<PropertyTrack<MorphOptions>>,

    // ── Direct fields (not in registry) ────
    pub layout_size: LayoutSizeState,
    pub svg_paths: Vec<VelloPath>,
    pub first_seen_ms: u64,
    pub children: Vec<String>,
}
```

### `#[prop]` Attribute Grammar

```rust
#[prop(
    name = "string",           // Inspector-facing name
    source = "string",         // Source-language property name (for source edits)
    group = "string",          // Inspector group (Transform, Shape, Style, Content, Layout)
    default = expr,            // Default value expression
)]

// Optional overrides:
#[prop(widget = WidgetHint::...)]           // Defaults based on type
#[prop_custom(apply = fn_path)]             // Custom edit handler
#[prop_custom(display = fn_path)]           // Custom display handler
#[prop_custom(has_keyframes = fn_path)]     // Custom keyframe detection
#[prop_custom(collect = fn_path)]           // Custom keyframe collector
#[prop_custom(serialize = fn_path)]         // Custom serializer
```

Fields without `#[prop]` are treated as direct metadata fields and excluded from the registry.

---

## Generated Code (By the Derive Macro)

### 1. `PropertyKey` Enum

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PropertyKey {
    Position,
    MotionOffset,
    Rotation,
    Scale,
    ShapeType,
    LineFrom,
    LineTo,
    ArcAngles,
    Points,
    Color,
    Opacity,
    StrokeWidth,
    StrokeColor,
    StrokeProgress,
    FillOpacity,
    TextContent,
    TextPaths,
    VectorPaths,
    Image,
    Size,
    PlacementMode,
    PositionBinding,
    MorphOptions,
}

impl PropertyKey {
    pub const fn name(self) -> &'static str;
    pub const fn source_name(self) -> &'static str;
    pub const fn group(self) -> &'static str;
    pub const COUNT: usize;
}
```

### 2. `PropertyMeta` Registry (Cold Metadata Only)

```rust
pub struct PropertyMeta {
    pub key: PropertyKey,
    pub name: &'static str,
    pub source_name: &'static str,
    pub group: &'static str,
    pub widget: WidgetHint,
}

pub static PROPERTIES: &[PropertyMeta] = &[
    PropertyMeta { key: PropertyKey::Position, name: "position", source_name: "at", group: "Transform", widget: WidgetHint::Vec2 },
    PropertyMeta { key: PropertyKey::Rotation, name: "rotation", source_name: "rotation", group: "Transform", widget: WidgetHint::FloatDegrees },
    // ... etc
];
```

### 3. Enum-Dispatch Methods (Behavior)

```rust
impl AnimationTrack {
    /// Evaluate display value for inspector. Called per-property when rendering the inspector.
    pub fn get_display(&self, key: PropertyKey, time_ms: u64) -> Option<PropertyDisplayValue> {
        match key {
            PropertyKey::Position => {
                self.position.as_ref().map(|p| PropertyDisplayValue::Vec2(p.evaluate(time_ms)))
            }
            PropertyKey::Rotation => {
                self.rotation.as_ref().map(|p| PropertyDisplayValue::Float(p.evaluate(time_ms)))
            }
            PropertyKey::Color => {
                self.color.as_ref().map(|p| PropertyDisplayValue::Color(p.evaluate(time_ms)))
            }
            PropertyKey::ShapeType => {
                self.shape_type.as_ref().map(|p| PropertyDisplayValue::Enum(p.evaluate(time_ms).to_string()))
            }
            // ... etc for all properties
        }
    }

    /// Apply an inspector edit. Returns true if the edit succeeded.
    pub fn apply_edit(&mut self, key: PropertyKey, time_ms: u64, value: &PropertyValue) -> bool {
        match key {
            PropertyKey::Position => {
                let PropertyValue::Vec2(v) = value else { return false };
                self.position.ensure([0.0, 0.0]).add_keyframe(time_ms, *v, Easing::Linear);
                true
            }
            PropertyKey::Size => apply_size_edit(self, time_ms, value),
            PropertyKey::Rotation => {
                let PropertyValue::Float(v) = value else { return false };
                self.rotation.ensure(0.0).add_keyframe(time_ms, *v, Easing::Linear);
                true
            }
            // ... etc
        }
    }

    /// Check if a property has any keyframes.
    pub fn has_keyframes(&self, key: PropertyKey) -> bool {
        match key {
            PropertyKey::Position => self.position.is_some(),
            PropertyKey::Rotation => self.rotation.is_some(),
            // ... etc
        }
    }

    /// Collect keyframes for a property as (time_ms, value_string).
    pub fn collect_keyframes(&self, key: PropertyKey) -> Vec<(u64, String)> {
        match key {
            PropertyKey::Position => {
                self.position.as_ref().map(|pt| {
                    pt.keyframes.iter().map(|(&t, (v, _))| {
                        (t, format!("[{:.2}, {:.2}]", v[0], v[1]))
                    }).collect()
                }).unwrap_or_default()
            }
            // ... etc
        }
    }

    /// Push keyframe times into `out` (avoids per-property Vec allocations).
    pub fn collect_keyframe_times(&self, key: PropertyKey, out: &mut Vec<u64>) {
        match key {
            PropertyKey::Position => {
                if let Some(pt) = &self.position {
                    out.extend(pt.keyframes.keys().copied());
                }
            }
            // ... etc
        }
    }

    /// Serialize current value for source-code edits.
    pub fn serialize_value(&self, key: PropertyKey, time_ms: u64) -> Option<String> {
        match key {
            PropertyKey::Position => {
                let v = self.position(time_ms);
                Some(format!("[{}, {}]", v[0], v[1]))
            }
            // ... etc
        }
    }
}
```

### 4. Convenience Accessors (Hot Path)

```rust
impl AnimationTrack {
    #[inline]
    pub fn position(&self, time_ms: u64) -> [f32; 2] {
        self.position.get(time_ms, [0.0, 0.0])
    }

    #[inline]
    pub fn motion_offset(&self, time_ms: u64) -> [f32; 2] {
        self.motion_offset.get(time_ms, [0.0, 0.0])
    }

    #[inline]
    pub fn rotation(&self, time_ms: u64) -> f32 {
        self.rotation.get(time_ms, 0.0)
    }

    #[inline]
    pub fn scale(&self, time_ms: u64) -> f32 {
        self.scale.get(time_ms, 1.0)
    }

    #[inline]
    pub fn size(&self, time_ms: u64) -> [f32; 2] {
        self.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE)
    }

    #[inline]
    pub fn color(&self, time_ms: u64) -> [f32; 4] {
        self.color.get(time_ms, [1.0, 1.0, 1.0, 1.0])
    }

    #[inline]
    pub fn opacity(&self, time_ms: u64) -> f32 {
        self.opacity.get(time_ms, 1.0)
    }

    // ... etc for all properties

    #[inline]
    pub fn position_mut(&mut self) -> &mut PropertyTrack<[f32; 2]> {
        self.position.ensure([0.0, 0.0])
    }

    #[inline]
    pub fn rotation_mut(&mut self) -> &mut PropertyTrack<f32> {
        self.rotation.ensure(0.0)
    }

    // ... etc
}
```

---

## Special Cases

### `size` — Half-Extents vs. Full Size

The internal `size` track stores half-extents (for layout). The inspector displays and edits full size. Custom handlers:

```rust
fn apply_size_edit(track: &mut AnimationTrack, time_ms: u64, value: &PropertyValue) -> bool {
    let PropertyValue::Vec2(v) = value else { return false };
    let half = [v[0] / 2.0, v[1] / 2.0];
    track.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(time_ms, half, Easing::Linear);
    true
}

fn display_size_full(track: &AnimationTrack, time_ms: u64) -> Option<PropertyDisplayValue> {
    track.size.as_ref().map(|pt| {
        let v = pt.evaluate(time_ms);
        PropertyDisplayValue::Vec2([v[0] * 2.0, v[1] * 2.0])
    })
}
```

### `rotation` — Degrees UI, Radians Internal

Stored as radians in `PropertyTrack<f32>`. Displayed as degrees in inspector.

```rust
fn display_rotation_degrees(track: &AnimationTrack, time_ms: u64) -> Option<PropertyDisplayValue> {
    track.rotation.as_ref().map(|pt| {
        PropertyDisplayValue::Float(pt.evaluate(time_ms).to_degrees())
    })
}

fn apply_rotation_degrees(track: &mut AnimationTrack, time_ms: u64, value: &PropertyValue) -> bool {
    let PropertyValue::Float(deg) = value else { return false };
    track.rotation.ensure(0.0).add_keyframe(time_ms, deg.to_radians(), Easing::Linear);
    true
}
```

### `offset` — Derived Property (Manual Special Case)

`offset` is the only "derived" property. It reads/writes a nested field inside `PositionBinding::SceneAnchor`. Do NOT add generic `derived: true` support to the macro. Special-case it in the 1–2 call sites that need it.

```rust
// In inspector / edit handling, before iterating PROPERTIES:
fn add_offset_property(track: &AnimationTrack, time_ms: u64, group: &mut PropertyGroup) {
    if let Some(TrackValue::PositionBinding(pb)) = track.position_binding.as_ref() {
        if let PositionBinding::SceneAnchor { offset, .. } = pb.evaluate(time_ms) {
            group.properties.push(PropertyEntry {
                name: "offset",
                source_name: "offset",
                value: PropertyDisplayValue::Vec2(offset),
                has_keyframes: false, // offset itself has no keyframes
                widget_hint: WidgetHint::Vec2,
            });
        }
    }
}

fn apply_offset_edit(track: &mut AnimationTrack, time_ms: u64, value: &PropertyValue) -> bool {
    let PropertyValue::Vec2(v) = value else { return false };
    if let Some(pb) = track.position_binding.as_mut() {
        let current = pb.evaluate(time_ms);
        if let PositionBinding::SceneAnchor { anchor, .. } = current {
            pb.add_keyframe(time_ms, PositionBinding::SceneAnchor { anchor, offset: *v }, Easing::Linear);
            return true;
        }
    }
    false
}
```

### `layout_size` — Direct Field

Not annotated with `#[prop]`. Excluded from `PropertyKey`, `PROPERTIES`, and all generated dispatch. Stays a direct `LayoutSizeState` field on `AnimationTrack`. Any code that needs it accesses it directly.

---

## Integration Points

### Inspector (`animatix-gui/src/app/inspector.rs`)

```rust
fn build_property_groups(track: &AnimationTrack, time_ms: u64) -> Vec<PropertyGroup> {
    let mut groups: IndexMap<&'static str, PropertyGroup> = IndexMap::new();

    for meta in PROPERTIES.iter() {
        let Some(display) = track.get_display(meta.key, time_ms) else { continue };
        groups.entry(meta.group)
            .or_insert_with(|| PropertyGroup { name: meta.group, properties: Vec::new() })
            .properties.push(PropertyEntry {
                name: meta.name.to_string(),
                source_name: meta.source_name.to_string(),
                value: display,
                has_keyframes: track.has_keyframes(meta.key),
                widget_hint: meta.widget.clone(),
            });
    }

    // Special-case: offset (derived, not in PROPERTIES)
    if let Some(group) = groups.get_mut("Transform") {
        add_offset_property(track, time_ms, group);
    }

    groups.into_values().collect()
}
```

`render_editable_property_row` dispatches on `WidgetHint` only. No property-name matches.

### Edit Handling (`animatix-gui/src/app.rs`)

```rust
fn handle_property_edit(&mut self, edit: PropertyEdit) {
    // ... undo snapshot ...
    if let Some(ref mut timeline) = self.document.timeline {
        if let Some(track) = timeline.tracks.get_mut(&edit.actor) {
            // Special-case: offset
            if edit.property == "offset" {
                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                apply_offset_edit(track, time_ms, &edit.value);
            } else if let Some(meta) = PROPERTIES.iter().find(|p| p.name == edit.property) {
                let time_ms = (self.preview.current_time_s * 1000.0) as u64;
                track.apply_edit(meta.key, time_ms, &edit.value);
            }
            timeline.invalidate_frame_cache();
        }
    }
    // ... source edit using meta.source_name and track.serialize_value(meta.key, time_ms) ...
}
```

### Keyframe Collection

```rust
fn collect_keyframes(track: &AnimationTrack) -> Vec<(f64, String, String, String)> {
    let mut all = Vec::new();
    for meta in PROPERTIES.iter() {
        for (time_ms, value_str) in track.collect_keyframes(meta.key) {
            all.push((time_ms as f64 / 1000.0, meta.name.to_string(), value_str, String::new()));
        }
    }
    all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    all
}
```

### Timeline Helpers

```rust
impl Timeline {
    pub fn keyframe_times_s(&self) -> Vec<f64> {
        let mut times = Vec::new();
        for track in self.tracks.values() {
            for meta in PROPERTIES.iter() {
                track.collect_keyframe_times(meta.key, &mut times);
            }
        }
        times.sort_unstable();
        times.dedup();
        times.into_iter().map(|ms| ms as f64 / 1000.0).collect()
    }
}

impl AnimationTrack {
    pub fn max_keyframe_time(&self) -> Option<u64> {
        PROPERTIES.iter()
            .filter_map(|meta| {
                let mut times = Vec::new();
                self.collect_keyframe_times(meta.key, &mut times);
                times.into_iter().max()
            })
            .max()
    }
}
```

---

## `PropertyValue` Expansion

The GUI-side `PropertyValue` enum needs a `ShapeType` variant:

```rust
pub enum PropertyValue {
    Vec2([f32; 2]),
    Float(f32),
    Color([f32; 4]),
    Text(String),
    ShapeType(ShapeType),  // NEW
}
```

`WidgetHint::Enum(&SHAPE_VARIANTS)` renders a dropdown. On edit, the selected string is parsed to `ShapeType` and wrapped in `PropertyValue::ShapeType`.

---

## Implementation Phases

### Phase 0: Derive Macro Skeleton (No Properties Migrated)

**Goal:** Write the `TrackProperties` derive macro with correct attribute parsing and code generation. Validate on a dummy struct before touching `AnimationTrack`.

**Steps:**
1. Create new crate: `crates/animatix-macros/` (proc-macro crate).
2. Implement `#[derive(TrackProperties)]` that parses `#[prop(...)]` attributes.
3. Generate `PropertyKey`, `PropertyMeta`, `PROPERTIES`, and all dispatch methods.
4. Test with a minimal dummy struct in `animatix-macros/tests/`.
5. Verify generated code compiles and `PROPERTIES` has correct length.

**Deliverable:** `animatix-macros` crate with passing tests.

---

### Phase 1: Proof of Concept (5 Properties)

**Goal:** Migrate a small subset of properties end-to-end to validate the macro, registry, and generic code paths.

**Properties:** `position`, `rotation`, `opacity`, `color`, `size`

**Steps:**
1. Add `#[derive(TrackProperties)]` and `#[prop(...)]` attributes to `AnimationTrack` for the 5 properties.
2. Delete the old manual struct fields for those 5 properties.
3. In `scene_eval.rs`: Replace `track.position.get(time_ms, ...)` with `track.position(time_ms)`.
4. In `build.rs`: Replace `track.position.ensure(...)` with `track.position_mut()`.
5. In `inspector.rs`: Replace manual `build_property_groups` for those 5 with generic registry iteration.
6. In `app.rs`: Replace edit handling for those 5 with `track.apply_edit(key, ...)`.
7. Run all tests. Verify inspector renders, edits apply, scene evaluates correctly.

**Deliverable:** 5 properties fully migrated. All tests pass. Performance validated (no regression in `cargo bench` if available, or manual timing).

---

### Phase 2: Migrate Remaining Core Properties (15 Properties)

**Properties:** `motion_offset`, `scale`, `shape_type`, `line_from`, `line_to`, `arc_angles`, `points`, `stroke_width`, `stroke_color`, `stroke_progress`, `fill_opacity`, `text_content`, `text_paths`, `vector_paths`, `image`

**Steps:**
1. Add `#[prop]` attributes to the remaining fields.
2. Update `scene_eval.rs`, `runtime.rs`, `layout.rs`, `mod.rs` to use convenience accessors.
3. Update `build.rs`, `actions/*.rs` to use `ensure_*` / `*_mut` accessors.
4. Remove old manual code in `inspector.rs`, `app.rs`, `workspace.rs`.
5. Run all tests.

**Deliverable:** 20 core properties migrated.

---

### Phase 3: Migrate Layout Properties (3 Properties)

**Properties:** `placement_mode`, `position_binding`, `morph_options`

**Steps:**
1. Add `#[prop]` attributes. These are mostly read-only in the inspector.
2. Update any code that reads them.
3. Run all tests.

**Deliverable:** All 23 registry properties migrated.

---

### Phase 4: Special Cases & Cleanup

**Steps:**
1. Implement `offset` special-case in inspector and edit handlers.
2. Verify `layout_size` direct field still works everywhere.
3. Update `source_edit.rs` to handle `PropertyValue::ShapeType`.
4. Simplify `document.rs::track_max_ms` to use `AnimationTrack::max_keyframe_time`.
5. Delete all dead code (old struct fields, old match arms, old `build_property_groups`, old `handle_property_edit`).
6. Run full test suite.

**Deliverable:** Zero dead code. All special cases handled.

---

### Phase 5: Validation & Documentation

**Steps:**
1. Add a test: "adding a new property requires only a `#[prop]` attribute."
2. Benchmark hot-path access: `track.position(time_ms)` vs. old `track.position.get(time_ms, ...)`.
3. Verify rust-analyzer autocomplete works on `AnimationTrack` fields.
4. Update internal docs.

**Deliverable:** Design validated. Ready for production use.

---

## Files Modified

| File | Action |
|------|--------|
| `crates/animatix-macros/Cargo.toml` | **New crate**: proc-macro dependencies |
| `crates/animatix-macros/src/lib.rs` | **New file**: `TrackProperties` derive macro |
| `crates/animatix-macros/tests/*.rs` | **New files**: macro test cases |
| `crates/animatix/Cargo.toml` | Add `animatix-macros` dependency |
| `crates/animatix/src/timeline/track.rs` | Add `#[derive(TrackProperties)]`, add `#[prop]` attrs |
| `crates/animatix/src/timeline/scene_eval.rs` | Migrate to convenience accessors |
| `crates/animatix/src/timeline/runtime.rs` | Migrate to convenience accessors |
| `crates/animatix/src/timeline/layout.rs` | Migrate to convenience accessors |
| `crates/animatix/src/timeline/mod.rs` | Simplify `keyframe_times_s`, `actor_world_affine` |
| `crates/animatix/src/timeline/build.rs` | Migrate to `*_mut` accessors |
| `crates/animatix/src/timeline/actions/*.rs` | Migrate to `*_mut` accessors |
| `crates/animatix-gui/src/app/inspector.rs` | Generic `build_property_groups`, `WidgetHint` dispatch |
| `crates/animatix-gui/src/app.rs` | Registry-driven edit handling |
| `crates/animatix-gui/src/app/workspace.rs` | Drag handlers use registry or accessors |
| `crates/animatix-gui/src/source_edit.rs` | Handle `PropertyValue::ShapeType` |
| `crates/animatix-gui/src/document.rs` | Simplify `track_max_ms` |

---

## Adding a Property (After)

### Existing type (e.g., `blur_radius`):

```rust
// In AnimationTrack:
#[prop(name = "blur_radius", source = "blur_radius", group = "Style", default = 0.0)]
pub blur_radius: Option<PropertyTrack<f32>>,
```

→ Add the field. The macro generates everything else. Done.

### New type (e.g., `Gradient`):

1. Implement `Interpolate for Gradient`.
2. Add to `AnimationTrack`:
   ```rust
   #[prop(name = "gradient", source = "gradient", group = "Style", default = Gradient::default())]
   pub gradient: Option<PropertyTrack<Gradient>>,
   ```
3. Add `PropertyValue::Gradient(Gradient)` variant (GUI-side).
4. Add `WidgetHint::Gradient` + render support (GUI-side).

---

## Performance

| Operation | Current | After | Notes |
|-----------|---------|-------|-------|
| Property read (hot path) | Field offset + `Option::map` | Same | Convenience accessor inlines to identical code |
| Property write (build) | `Option::get_or_insert_with` | Same | `*_mut()` accessor inlines to identical code |
| Inspector iteration | 22 manual `if let` checks | 22 enum match arms | Same complexity, centralized |
| Keyframe collection | 22 manual collections | 22 enum match arms | Same |
| `keyframe_times_s` | 22 `extend` calls | 22 `collect_keyframe_times` calls | Same |

**No regression.** The generated code is structurally identical to hand-written code; the macro only eliminates human error and boilerplate.

---

## Decisions

1. **No runtime property registration.** The property set is closed. A compile-time macro is simpler and faster.
2. **Derive on manual struct, not full struct generation.** Preserves rust-analyzer support and explicit type visibility.
3. **Enum dispatch (match on `PropertyKey`), not fn pointers.** Better cache locality, inlineable, exhaustively checked at compile time.
4. **`offset` is manually special-cased.** One derived property does not justify generic `derived: true` macro complexity.
5. **`layout_size` stays a direct field.** Not in registry. Code that needs it accesses it directly.
6. **`WidgetHint` drives inspector widget selection.** Not property name. New widget types can be added without changing property definitions.
7. **Phased migration with PoC first.** Validate the macro on 5 properties before committing all 23.

---

## Risks & Mitigations

| Risk | Mitigation |
|------|-----------|
| **Macro is hard to debug** | Phase 0 validates on a dummy struct before touching `AnimationTrack`. Start with simple attribute parsing, expand incrementally. |
| **Migration is large** | Phased: 5 properties first, then 15, then 3. Each phase is a self-contained PR. |
| **Performance regression** | Benchmark Phase 1 (PoC). Hot-path accessors inline to identical code; any regression means the macro generated suboptimal code, which we fix before proceeding. |
| **rust-analyzer breakage** | Derive on manual struct preserves field autocomplete. Only generated items (`PropertyKey`, `PROPERTIES`) are macro-produced; rust-analyzer handles these well. |
| **Serialization format change** | `PROPERTIES` array defines stable iteration order. If disk format depends on order, freeze the order explicitly. |
| **Team unfamiliar with proc macros** | The macro is ~150 lines of standard `syn`/`quote` code. Well-documented examples exist. Alternatively, start with a `build.rs` code generator and upgrade to proc-macro later. |

---

## Appendix: Alternative — `build.rs` Code Generator

If the team is uncomfortable with proc macros, the same design can be implemented as a `build.rs` script:

1. Define properties in a `.toml` or `.json` file:
   ```toml
   [[property]]
   name = "position"
   source = "at"
   group = "Transform"
   rust_type = "[f32; 2]"
   default = "[0.0, 0.0]"
   ```

2. `build.rs` reads the file and generates `generated_track.rs`.

3. `track.rs` includes the generated file: `include!(concat!(env!("OUT_DIR"), "/generated_track.rs"));`

This avoids proc-macro complexity entirely at the cost of:
- No attribute syntax on the struct
- Generated code is in a separate file
- Slightly less ergonomic

**Recommendation:** Use proc-macro (cleaner, more idiomatic), but keep `build.rs` as a fallback if proc-macro development stalls.
