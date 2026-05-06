# Type-Erased Property System — Implementation Plan

## Goal

Replace the current architecture (typed struct fields + scattered match arms) with a
uniform, trait-based system where adding a property is a single registration and
no existing code needs modification.

---

## Current Architecture (What We're Replacing)

### Pain Points

Adding a property today requires touching **6 independent locations**:

| # | File | What |
|---|------|------|
| 1 | `timeline/track.rs` | Add `Option<PropertyTrack<T>>` field to `AnimationTrack` |
| 2 | `app/workspace.rs` | Add variant to `PropertyValue` enum (if new type) |
| 3 | `property_registry.rs` | Add `PropertyDescriptor` entry |
| 4 | `source_edit.rs` | Add serialization arm (if new type) |
| 5 | `app/inspector.rs` | Add widget dispatch arm |
| 6 | `property_registry.rs` | Add `PropertyKind` variant (if new type) |

These are independent and can drift — forgetting any one produces a silent bug.

### Data Flow (Current)

```
AnimationTrack (typed fields)
    → inspector iterates fields manually
    → builds PropertyEntry with PropertyDisplayValue
    → match on display value → widget
    → PropertyEdit { property: String, value: PropertyValue }
    → handle_property_edit
    → property_registry lookup (HashMap)
    → update_timeline_default / update_timeline_keyframe (function pointers)
    → serialize_property_value (separate match)
    → source text edit
```

---

## Target Architecture

### Core Types

```rust
// ─── Track Storage ──────────────────────────────────────────────────────

/// Type-erased track value. Each variant wraps a PropertyTrack<T>.
#[derive(Clone)]
pub enum TrackValue {
    Vec2(PropertyTrack<[f32; 2]>),
    Float(PropertyTrack<f32>),
    Color(PropertyTrack<[f32; 4]>),
    Text(PropertyTrack<String>),
    Shape(PropertyTrack<ShapeType>),
    Binding(PropertyTrack<PositionBinding>),
    // Future: add variants as needed
}

impl TrackValue {
    /// Create a new track with the given default value.
    pub fn new_from_value(value: &PropertyValue) -> Self { ... }

    /// Add a keyframe at the given time.
    pub fn add_keyframe(&mut self, time_ms: u64, value: &PropertyValue, easing: Easing) { ... }

    /// Set the default value.
    pub fn set_default(&mut self, value: &PropertyValue) { ... }

    /// Evaluate at the given time, returning a PropertyValue.
    pub fn evaluate(&self, time_ms: u64) -> PropertyValue { ... }

    /// Whether this track has any keyframes.
    pub fn has_keyframes(&self) -> bool { ... }

    /// Serialize the current default value to source text.
    pub fn serialize_default(&self) -> String { ... }
}

// ─── Property Descriptor ────────────────────────────────────────────────

/// Metadata for a single editable property.
pub struct PropertyDescriptor {
    /// Internal name (e.g. "position", "stroke_width").
    pub name: &'static str,
    /// Source text name (e.g. "position" → "at").
    pub source_name: &'static str,
    /// The key used to look up the track in AnimationTrack::tracks.
    /// Usually same as `name`, but can differ (e.g. "at" → "position").
    pub track_key: &'static str,
    /// Widget hint for the inspector.
    pub widget: WidgetHint,
    /// Optional custom update logic (for special cases).
    pub custom_update: Option<fn(&mut AnimationTrack, time_ms: u64, &PropertyValue) -> bool>,
}

/// Widget hint for the inspector.
pub enum WidgetHint {
    Vec2,                    // Two number fields
    Float,                   // Single number field with drag
    FloatSlider { min: f32, max: f32 },  // Slider
    FloatDegrees,            // Number field with ° suffix (rotation)
    Color,                   // Color picker
    Text,                    // Text input
    Enum(&'static [&'static str]),  // Dropdown selector
    ReadOnly,                // Display only
}

// ─── AnimationTrack (Simplified) ────────────────────────────────────────

#[derive(Clone)]
pub struct AnimationTrack {
    pub label: String,
    /// All property tracks, keyed by property name.
    pub tracks: HashMap<String, TrackValue>,
    /// Scene graph children.
    pub children: Vec<String>,
    /// First appearance time (visibility gate).
    pub first_seen_ms: u64,
    /// Layout-specific state (not a regular property).
    pub layout_size: LayoutSizeState,
    /// Static SVG paths (not a regular property).
    pub svg_paths: Vec<VelloPath>,
}
```

### Property Registration

```rust
// ─── Registry ───────────────────────────────────────────────────────────

/// All registered properties. Built once at startup.
pub static PROPERTIES: LazyLock<Vec<PropertyDescriptor>> = LazyLock::new(|| {
    vec![
        // Simple properties (no custom logic)
        prop("size",          "size",          "size",          WidgetHint::Vec2),
        prop("line_from",     "line_from",     "line_from",     WidgetHint::Vec2),
        prop("line_to",       "line_to",       "line_to",       WidgetHint::Vec2),
        prop("arc_angles",    "arc_angles",    "arc_angles",    WidgetHint::Vec2),
        prop("motion_offset", "motion_offset", "motion_offset", WidgetHint::Vec2),
        prop("rotation",      "rotation",      "rotation",      WidgetHint::FloatDegrees),
        prop("scale",         "scale",         "scale",         WidgetHint::Float),
        prop("opacity",       "opacity",       "opacity",       WidgetHint::FloatSlider { min: 0.0, max: 1.0 }),
        prop("stroke_width",  "stroke_width",  "stroke_width",  WidgetHint::Float),
        prop("stroke_progress","stroke_progress","stroke_progress", WidgetHint::FloatSlider { min: 0.0, max: 1.0 }),
        prop("fill_opacity",  "fill_opacity",  "fill_opacity",  WidgetHint::FloatSlider { min: 0.0, max: 1.0 }),
        prop("color",         "color",         "color",         WidgetHint::Color),
        prop("stroke_color",  "stroke_color",  "stroke_color",  WidgetHint::Color),
        prop("text_content",  "text",          "text_content",  WidgetHint::Text),
        prop("shape_type",    "shape",         "shape_type",    WidgetHint::Enum(&[
            "Rect", "Circle", "Line", "Ellipse", "Arc", "Polygon", "Path", "Arrow", "Graph", "Plot",
        ])),

        // Special properties (custom update logic)
        prop_custom("position", "at", "position", WidgetHint::Vec2, update_position),
        prop_custom("at",       "at", "position", WidgetHint::Vec2, update_at),
        prop_custom("offset",   "offset", "position_binding", WidgetHint::Vec2, update_offset),
    ]
});

fn prop(name: &'static str, source: &'static str, track: &'static str, widget: WidgetHint) -> PropertyDescriptor {
    PropertyDescriptor {
        name, source_name: source, track_key: track, widget,
        custom_update: None,
    }
}

fn prop_custom(name: &'static str, source: &'static str, track: &'static str,
               widget: WidgetHint, update: fn(&mut AnimationTrack, u64, &PropertyValue) -> bool
) -> PropertyDescriptor {
    PropertyDescriptor {
        name, source_name: source, track_key: track, widget,
        custom_update: Some(update),
    }
}
```

### How Special Cases Work

**Position (binding-aware routing):**
```rust
fn update_position(track: &mut AnimationTrack, time_ms: u64, value: &PropertyValue) -> bool {
    if let PropertyValue::Vec2(v) = value {
        let pt = track.tracks.entry("position".into())
            .or_insert_with(|| TrackValue::Vec2(PropertyTrack::new(*v)));
        if let TrackValue::Vec2(pt) = pt {
            pt.add_keyframe(time_ms, *v, Easing::Linear);
        }
        true
    } else { false }
}
```

**Size (half-extents transform):**
```rust
// Registered with custom_update that divides by 2:
fn update_size(track: &mut AnimationTrack, time_ms: u64, value: &PropertyValue) -> bool {
    if let PropertyValue::Vec2(v) = value {
        let half = [v[0] / 2.0, v[1] / 2.0];
        let pt = track.tracks.entry("size".into())
            .or_insert_with(|| TrackValue::Vec2(PropertyTrack::new(half)));
        if let TrackValue::Vec2(pt) = pt {
            pt.add_keyframe(time_ms, half, Easing::Linear);
        }
        true
    } else { false }
}
```

**Offset (mutates position_binding track):**
```rust
fn update_offset(track: &mut AnimationTrack, time_ms: u64, value: &PropertyValue) -> bool {
    if let PropertyValue::Vec2(v) = value {
        if let Some(TrackValue::Binding(pb)) = track.tracks.get_mut("position_binding") {
            let current = pb.evaluate(time_ms);
            if let PositionBinding::SceneAnchor { anchor, .. } = current {
                pb.add_keyframe(time_ms, PositionBinding::SceneAnchor { anchor, offset: *v }, Easing::Linear);
                return true;
            }
        }
    }
    false
}
```

### Inspector (Simplified)

```rust
fn build_property_groups(track: &AnimationTrack, time_ms: u64) -> Vec<PropertyGroup> {
    let mut groups = Vec::new();

    for desc in PROPERTIES.iter() {
        let Some(track_value) = track.tracks.get(desc.track_key) else { continue };

        let display_value = match track_value {
            TrackValue::Vec2(pt) => {
                let v = pt.evaluate(time_ms);
                PropertyDisplayValue::Vec2(format!("{}", v[0]), format!("{}", v[1]))
            }
            TrackValue::Float(pt) => {
                let v = pt.evaluate(time_ms);
                PropertyDisplayValue::Scalar(format!("{}", v))
            }
            TrackValue::Color(pt) => {
                let v = pt.evaluate(time_ms);
                PropertyDisplayValue::Color(v)
            }
            TrackValue::Text(pt) => {
                let v = pt.evaluate(time_ms);
                PropertyDisplayValue::Text(v)
            }
            // ... other variants
        };

        groups.push(PropertyEntry {
            name: desc.name.to_string(),
            source_name: desc.source_name.to_string(),
            value: display_value,
            has_keyframes: track_value.has_keyframes(),
            widget_hint: desc.widget.clone(),
        });
    }

    groups
}

fn render_editable_property_row(ui, actor_label, entry, actions, keyframe_mode) {
    // Dispatch based on widget hint, not property name
    match &entry.widget_hint {
        WidgetHint::Vec2 => { /* vec2_input widget */ }
        WidgetHint::Float => { /* float_input widget */ }
        WidgetHint::FloatSlider { min, max } => { /* slider widget */ }
        WidgetHint::FloatDegrees => { /* rotation widget with deg↔rad conversion */ }
        WidgetHint::Color => { /* color picker widget */ }
        WidgetHint::Text => { /* text input widget */ }
        WidgetHint::Enum(variants) => { /* dropdown selector */ }
        WidgetHint::ReadOnly => { /* readonly display */ }
    }
}
```

### Serialization (On TrackValue)

```rust
impl TrackValue {
    /// Serialize a PropertyValue to source text.
    pub fn serialize_value(value: &PropertyValue) -> String {
        match value {
            PropertyValue::Vec2([x, y]) => {
                if x.fract() == 0.0 && y.fract() == 0.0 {
                    format!("({}, {})", *x as i32, *y as i32)
                } else {
                    format!("({}, {})", x, y)
                }
            }
            PropertyValue::Float(v) => {
                if v.fract() == 0.0 { format!("{}", *v as i32) } else { format!("{}", v) }
            }
            PropertyValue::Color([r, g, b, a]) => { /* existing color logic */ }
            PropertyValue::Text(s) => { format!("\"{}\"", s.replace('"', "\\\"")) }
        }
    }
}
```

---

## What Changes Per File

### `crates/animatix/src/timeline/track.rs`
- **Remove**: All individual `Option<PropertyTrack<T>>` fields from `AnimationTrack`
- **Add**: `tracks: HashMap<String, TrackValue>` field
- **Add**: `TrackValue` enum definition
- **Keep**: `PropertyTrack<T>`, `TrackAccessor` trait, `Interpolate` trait
- **Keep**: `first_seen_ms`, `children`, `layout_size`, `svg_paths` (non-property fields)

### `crates/animatix/src/timeline/mod.rs`
- **Update**: `Timeline::build()` to insert into `tracks` HashMap instead of setting fields
- **Update**: `Timeline::evaluate()` to iterate `tracks` or look up by key
- **Update**: `Timeline::invalidate_frame_cache()` — no change needed

### `crates/animatix/src/timeline/scene_eval.rs`
- **Update**: `evaluate_node()` to look up tracks from `track.tracks.get("position")` etc.
- **Update**: Runtime value injection to iterate `track.tracks`

### `crates/animatix/src/timeline/build.rs`
- **Update**: Property assignment to use `track.tracks.insert(key, TrackValue::...)`
- **Update**: All `track.position`, `track.size` etc. references to `track.tracks.get_mut("position")`

### `crates/animatix-gui/src/property_registry.rs`
- **Remove**: `PropertyKind` enum (no longer needed — `TrackValue` carries the type)
- **Remove**: `PropertyValue` import dependency
- **Simplify**: `PropertyDescriptor` — remove `kind`, add `widget: WidgetHint`, add `track_key`
- **Simplify**: Update functions — use `track.tracks.entry(key)` instead of `track.field`
- **Add**: `WidgetHint` enum
- **Add**: `PROPERTIES` as `Vec<PropertyDescriptor>` instead of `HashMap`

### `crates/animatix-gui/src/app/workspace.rs`
- **Keep**: `PropertyValue` enum (still used for PropertyEdit communication)
- **Keep**: `PropertyEdit` struct
- **Update**: Drag handlers to use `track.tracks.get_mut(...)` instead of `track.position` etc.

### `crates/animatix-gui/src/app/inspector.rs`
- **Update**: `build_property_groups()` to iterate `PROPERTIES` registry instead of track fields
- **Update**: `render_editable_property_row()` to dispatch on `WidgetHint` instead of property name
- **Remove**: Hardcoded match arms for each property name

### `crates/animatix-gui/src/source_edit.rs`
- **Move**: `serialize_property_value` to `TrackValue::serialize_value` (or keep as standalone)
- **Keep**: `apply_source_edit`, `insert_property_after_span`, `insert_keyframe_block` (unchanged)

### `crates/animatix-gui/src/app.rs`
- **Simplify**: `handle_property_edit` — use registry lookup, no more match on PropertyValue
- **Simplify**: `handle_keyframe_edit` — same
- **Simplify**: `flush_pending_edits` — same

---

## Migration Strategy

### Phase 1: Add TrackValue enum (non-breaking)
- Define `TrackValue` in `track.rs`
- Implement `TrackValue::new_from_value`, `add_keyframe`, `set_default`, `evaluate`, `serialize_value`
- Add `tracks: HashMap<String, TrackValue>` to `AnimationTrack` **alongside** existing fields
- Populate both during build

### Phase 2: Migrate readers (non-breaking)
- Update `scene_eval.rs` to read from `track.tracks` instead of typed fields
- Update `inspector.rs` to read from `track.tracks` via registry
- Verify all tests pass

### Phase 3: Migrate writers (non-breaking)
- Update `build.rs` to write to `track.tracks` instead of typed fields
- Update `app.rs` drag handlers to write to `track.tracks`
- Verify all tests pass

### Phase 4: Remove old fields (breaking)
- Remove individual `Option<PropertyTrack<T>>` fields from `AnimationTrack`
- Remove `PropertyValue` enum (if no longer needed) or keep for `PropertyEdit`
- Remove `PropertyKind` enum
- Clean up dead code

### Phase 5: Polish
- Add `WidgetHint` to registry
- Update inspector to use `WidgetHint` dispatch
- Remove hardcoded property name matches from inspector

---

## Special Cases to Handle

| Property | Special Behavior | How |
|----------|-----------------|-----|
| `position` | Binding-aware routing | `custom_update` function checks binding type |
| `at` | Alias for position | Same `track_key: "position"`, different `custom_update` |
| `offset` | Mutates `position_binding` track | `custom_update` reaches into binding track |
| `size` | Half-extents transform | `custom_update` divides by 2 |
| `rotation` | Radians in track, degrees in UI | `WidgetHint::FloatDegrees` handles conversion |
| `shape_type` | String ↔ ShapeType enum | `WidgetHint::Enum` + custom serialization |
| `placement_mode` | Not editable via inspector | Not registered in PROPERTIES |
| `position_binding` | Not directly editable | Not registered (accessed via `at`/`offset`) |
| `layout_size` | Layout-specific, not a track | Stays as separate field on AnimationTrack |
| `svg_paths` | Static, not a track | Stays as separate field |
| `first_seen_ms` | Metadata, not a track | Stays as separate field |
| `children` | Scene graph, not a track | Stays as separate field |

---

## Performance Considerations

- **HashMap lookup**: ~50ns for ~20 entries. Negligible.
- **Downcast (enum match)**: ~1-2ns. Negligible.
- **Track iteration for inspector**: One pass over PROPERTIES, one HashMap lookup per property. Same cost as current field access.
- **Source text manipulation**: Unchanged (still the bottleneck, still addressed by batching).
- **No regression expected.**

---

## Files to Create/Modify

| File | Action |
|------|--------|
| `crates/animatix/src/timeline/track.rs` | Add `TrackValue`, modify `AnimationTrack` |
| `crates/animatix/src/timeline/mod.rs` | Update track access patterns |
| `crates/animatix/src/timeline/build.rs` | Update track creation |
| `crates/animatix/src/timeline/scene_eval.rs` | Update track reading |
| `crates/animatix-gui/src/property_registry.rs` | Rewrite with new architecture |
| `crates/animatix-gui/src/app/inspector.rs` | Rewrite property enumeration |
| `crates/animatix-gui/src/app.rs` | Simplify edit handlers |
| `crates/animatix-gui/src/app/workspace.rs` | Update drag handlers |
| `crates/animatix-gui/src/source_edit.rs` | Move serialization (minor) |

---

## What Adding a Property Looks Like (After)

**Existing type (e.g., new Float property "blur_radius"):**
1. Add `PropertyDescriptor` entry to `PROPERTIES` vec — **1 line**
2. Done. Inspector, serializer, timeline update all pick it up automatically.

**New type (e.g., Gradient):**
1. Add `PropertyValue::Gradient(...)` variant
2. Add `TrackValue::Gradient(PropertyTrack<Gradient>)` variant
3. Implement `Interpolate` for `Gradient`
4. Add `WidgetHint::Gradient` variant + inspector widget
5. Add serialization in `TrackValue::serialize_value`
6. Add `PropertyDescriptor` entry to `PROPERTIES`

Still more work for new types, but it's all in **well-defined, localized places** — not scattered across 6 files.
