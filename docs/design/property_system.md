# Property System Design

> **Status:** Implemented  
> **Applies to:** `crates/animatix/src/timeline/`  
> **Driver:** Eliminate N×M match-block explosion, add type safety, make property extension a single-point change

---

## 1. Original Problem: The Pain (now resolved)

The property system was originally implemented as **7+ cross-file `match prop.name.as_str()` blocks** that had to be kept in sync:

| File | Lines | Concern | Status |
|------|-------|---------|--------|
| `build.rs` line 893 | ~20 | Plot-domain pre-pass (ActorKind dispatch) | **Merged with main pass** |
| `build.rs` line 1298 | ~80 | First property pass (domain props: `x_domain`, `func`, etc.) | **Streamlined** |
| `build.rs` line 1501 | ~130 | Second property pass (shape props: `color`, `size`, `stroke_width`, etc.) | **Streamlined with registry** |
| `declarations_text.rs` line 145 | ~100 | Text/Math/Code props (`font_size`, `text`, `latex`, etc.) | **Preserved** (domain logic for typesetting) |
| `media.rs` line 171 | ~30 | Svg/Image props (`url`, `scale`, `size`) | **Preserved** (domain logic for media loading) |
| `assignments.rs` line 108 | ~600 | Runtime property mutation | **Replaced with registry-driven engine** ✅ |
| `runtime.rs` | ~150 | Per-frame environment injection | **Replaced with centralized injector** ✅ |

**Each block duplicated:**
- The list of recognized property names (stringly-typed)
- Expression-to-value parsing
- The storage-field write
- Keyframe timing boilerplate (snapshot-start, insert-end, preserve-delayed)
- Default-value handling

**Adding one property** now requires editing 3-5 files (schema row + field + optional engine arm). **Adding one actor type** requires 7 touch points — all compiler-enforced.

---

## 2. The Property Schema (Core Data Model)

Every property in the system is described by one static record — pure data, no function pointers:

```rust
// ───────────────────────────────────────────────────────────
// docs/design/property_system.md — Schema
// ───────────────────────────────────────────────────────────

/// The complete description of one property in the system.
/// This is the *single source of truth* for every property.
struct PropertySchema {
    /// Canonical name as it appears in source text.
    name: &'static str,

    /// The value type determines parsing, interpolation, default, inject patterns.
    value_type: ValueType,

    /// Feature flags that change how the engine processes this property.
    flags: PropertyFlags,

    /// For ANIMATED properties: which field on the storage tier to write to.
    /// For BUILD_TIME_ONLY properties: which side-effect handler to invoke.
    field: ActorField,

    /// For compound properties that depend on other properties:
    /// which resolution group this belongs to.
    /// None for simple independent properties.
    group: Option<GroupMembership>,
}

// ───────────────────────────────────────────────────────────

/// The set of all possible value types a property can carry.
/// Adding a new variant here is rare — only when a fundamentally
/// new kind of animated value is introduced.
enum ValueType {
    // Scalars
    F32,
    U32,

    // Vectors
    Vec2,        // [f32; 2]
    Vec4,        // [f32; 4]

    // Colors (RGBA with gamma-aware handling)
    Color,

    // Strings
    String,

    // Enums
    ShapeType,         // Rect | Circle | Line | Ellipse | Arc | Polygon | Path | Arrow
    PlacementMode,     // LayoutManaged | Manual
    SceneAnchor,       // Center | TopLeft | Top | TopRight | Left | Right | BottomLeft | Bottom | BottomRight
    PositionBinding,   // Absolute | SceneAnchor{..} | ScenePercent{..} | ContainerDefault{..} | ContainerPercent{..}

    // Complex types (require custom interpolation)
    MorphOptions,
    TextPaths,
    VelloPaths,
    ImagePayload,

    // Structural (arrays of primitives)
    PointList,         // Vec<[f32; 2]>
    Vec2List,          // Vec<[f32; 2]>
    CommandList,       // Path commands

    /// Marker for properties that produce side effects,
    /// not animated values (e.g. `func`, `x_domain`, `commands`).
    BuildTimeOnly,
}

// ───────────────────────────────────────────────────────────

bitflags! {
    struct PropertyFlags: u8 {
        /// Property animates over time (has keyframes).
        /// If absent, the property is BUILD_TIME_ONLY.
        const ANIMATED           = 0b0001;

        /// Property affects layout of parent container.
        /// Changing it triggers layout re-computation.
        const LAYOUT_AFFECTING   = 0b0010;

        /// Property can be assigned at runtime (via `=` syntax).
        /// If absent, it can only be set at declaration time.
        const ASSIGNABLE         = 0b0100;

        /// Property is injected into the runtime Environment
        /// for use in expressions like `label.color`.
        const INJECTABLE         = 0b1000;
    }
}

// ───────────────────────────────────────────────────────────

/// Which storage field or side-effect handler this property maps to.
///
/// This is a flat enum over ALL possible storage locations.
/// The match dispatch is written ONCE in the property engine.
enum ActorField {
    // ── Geometry tier ──
    Position,
    MotionOffset,
    Size,
    LayoutSize,
    Rotation,
    Scale,
    PlacementMode,
    PositionBinding,

    // ── Style tier ──
    Color,
    Opacity,
    StrokeWidth,
    StrokeColor,
    StrokeProgress,
    FillOpacity,
    MorphOptions,

    // ── Shape payload fields ──
    ShapeType,
    LineFrom,
    LineTo,
    ArcAngles,
    Points,
    VectorPaths,

    // ── Text payload fields ──
    TextContent,
    TextPaths,

    // ── Media payload fields ──
    ImageData,
    SvgPaths,

    // ── Composite resolution groups (handled by GroupHandler, not direct write) ──
    PositionBindingGroup,
    VectorShapeGroup,
    PlotDomainGroup,
    ContainerLayoutGroup,
}

// ───────────────────────────────────────────────────────────

/// For compound properties that resolve together.
/// Members of a group are collected, then processed at once.
struct GroupMembership {
    group_id: GroupHandlerId,
}

enum GroupHandlerId {
    /// at + anchor + offset → PositionBinding
    PositionBinding,
    /// radius, sides, from, to, start_angle, sweep_angle, points, commands
    /// → VectorShapeState (which produces size, line_from, line_to, arc_angles + vector_paths)
    VectorShapeState,
    /// x_domain, y_domain, t_domain, func, tolerance, max_depth, resolution
    /// → stored in env, consumed by plot curve builder
    PlotDomain,
    /// gap, align, cols → container metadata + layout
    ContainerLayout,
}
```

### 2.1 Registry Table

The full property registry is a static slice sorted by name for binary search:

```rust
/// The complete, authoritative registry of every property in Animatix.
/// Sorted by .name for O(log n) lookup.
pub(crate) static PROPERTY_REGISTRY: &[PropertySchema] = &[
    // ── Universal geometry ──
    PropertySchema { name: "anchor",      value_type: ValueType::SceneAnchor,    flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::PositionBindingGroup, group: Some(GroupMembership { group_id: PositionBinding }) },
    PropertySchema { name: "at",          value_type: ValueType::Vec2,           flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::PositionBindingGroup, group: Some(GroupMembership { group_id: PositionBinding }) },
    PropertySchema { name: "offset",      value_type: ValueType::Vec2,           flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::PositionBindingGroup, group: Some(GroupMembership { group_id: PositionBinding }) },

    PropertySchema { name: "position",    value_type: ValueType::Vec2,           flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::Position,             group: None },
    PropertySchema { name: "rotation",    value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::Rotation,             group: None },
    PropertySchema { name: "scale",       value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::Scale,                group: None },
    PropertySchema { name: "size",        value_type: ValueType::Vec2,           flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::Size,                 group: None },

    // ── Universal style ──
    PropertySchema { name: "color",       value_type: ValueType::Color,          flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::Color,                group: None },
    PropertySchema { name: "fill_opacity",value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::FillOpacity,          group: None },
    PropertySchema { name: "opacity",     value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::Opacity,              group: None },
    PropertySchema { name: "stroke",      value_type: ValueType::Color,          flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::StrokeColor,          group: None },
    PropertySchema { name: "stroke_color",value_type: ValueType::Color,          flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::StrokeColor,          group: None },
    PropertySchema { name: "stroke_progress",value_type: ValueType::F32,         flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::StrokeProgress,       group: None },
    PropertySchema { name: "stroke_width",value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::StrokeWidth,          group: None },
    PropertySchema { name: "width",       value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::StrokeWidth,          group: None },

    // ── Shape-specific ──
    PropertySchema { name: "arc_angles",  value_type: ValueType::Vec2,           flags: ANIMATED | ASSIGNABLE,               field: ActorField::ArcAngles,            group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "commands",    value_type: ValueType::BuildTimeOnly,  flags: BUILD_TIME_ONLY,                     field: ActorField::VectorShapeGroup,     group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "from",        value_type: ValueType::Vec2,           flags: ANIMATED | ASSIGNABLE,               field: ActorField::LineFrom,             group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "points",      value_type: ValueType::PointList,      flags: ANIMATED | ASSIGNABLE,               field: ActorField::Points,               group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "radius",      value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE,               field: ActorField::Size,                 group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "radius_x",    value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE,               field: ActorField::Size,                 group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "radius_y",    value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE,               field: ActorField::Size,                 group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "sides",       value_type: ValueType::U32,            flags: BUILD_TIME_ONLY,                     field: ActorField::VectorShapeGroup,     group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "start_angle", value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE,               field: ActorField::ArcAngles,            group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "sweep_angle", value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE,               field: ActorField::ArcAngles,            group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "tip_length",  value_type: ValueType::F32,            flags: BUILD_TIME_ONLY,                     field: ActorField::VectorShapeGroup,     group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "tip_width",   value_type: ValueType::F32,            flags: BUILD_TIME_ONLY,                     field: ActorField::VectorShapeGroup,     group: Some(GroupMembership { group_id: VectorShapeState }) },
    PropertySchema { name: "to",          value_type: ValueType::Vec2,           flags: ANIMATED | ASSIGNABLE,               field: ActorField::LineTo,               group: Some(GroupMembership { group_id: VectorShapeState }) },

    // ── Text/Math/Code ──
    PropertySchema { name: "code",        value_type: ValueType::String,         flags: ANIMATED,                            field: ActorField::TextContent,          group: None },
    PropertySchema { name: "font_family", value_type: ValueType::String,         flags: ASSIGNABLE,                          field: ActorField::TextContent,          group: None },
    PropertySchema { name: "font_size",   value_type: ValueType::F32,            flags: ANIMATED | ASSIGNABLE,               field: ActorField::TextContent,          group: None },
    PropertySchema { name: "latex",       value_type: ValueType::String,         flags: ANIMATED,                            field: ActorField::TextContent,          group: None },
    PropertySchema { name: "math",        value_type: ValueType::String,         flags: ANIMATED,                            field: ActorField::TextContent,          group: None },
    PropertySchema { name: "text",        value_type: ValueType::String,         flags: ANIMATED | ASSIGNABLE,               field: ActorField::TextContent,          group: None },

    // ── Media ──
    PropertySchema { name: "scale",       value_type: ValueType::F32,            flags: BUILD_TIME_ONLY,                     field: ActorField::SvgPaths,             group: None },
    PropertySchema { name: "url",         value_type: ValueType::String,         flags: BUILD_TIME_ONLY,                     field: ActorField::SvgPaths,             group: None },

    // ── Container ──
    PropertySchema { name: "align",       value_type: ValueType::String,         flags: BUILD_TIME_ONLY,                     field: ActorField::ContainerLayoutGroup, group: Some(GroupMembership { group_id: ContainerLayout }) },
    PropertySchema { name: "cols",        value_type: ValueType::U32,            flags: BUILD_TIME_ONLY,                     field: ActorField::ContainerLayoutGroup, group: Some(GroupMembership { group_id: ContainerLayout }) },
    PropertySchema { name: "gap",         value_type: ValueType::F32,            flags: BUILD_TIME_ONLY,                     field: ActorField::ContainerLayoutGroup, group: Some(GroupMembership { group_id: ContainerLayout }) },

    // ── Plot domain ──
    PropertySchema { name: "func",        value_type: ValueType::BuildTimeOnly,  flags: BUILD_TIME_ONLY,                     field: ActorField::PlotDomainGroup,      group: Some(GroupMembership { group_id: PlotDomain }) },
    PropertySchema { name: "max_depth",   value_type: ValueType::F32,            flags: BUILD_TIME_ONLY,                     field: ActorField::PlotDomainGroup,      group: Some(GroupMembership { group_id: PlotDomain }) },
    PropertySchema { name: "resolution",  value_type: ValueType::F32,            flags: BUILD_TIME_ONLY,                     field: ActorField::PlotDomainGroup,      group: Some(GroupMembership { group_id: PlotDomain }) },
    PropertySchema { name: "t_domain",    value_type: ValueType::Vec2,           flags: BUILD_TIME_ONLY,                     field: ActorField::PlotDomainGroup,      group: Some(GroupMembership { group_id: PlotDomain }) },
    PropertySchema { name: "tolerance",   value_type: ValueType::F32,            flags: BUILD_TIME_ONLY,                     field: ActorField::PlotDomainGroup,      group: Some(GroupMembership { group_id: PlotDomain }) },
    PropertySchema { name: "x_domain",    value_type: ValueType::Vec2,           flags: BUILD_TIME_ONLY,                     field: ActorField::PlotDomainGroup,      group: Some(GroupMembership { group_id: PlotDomain }) },
    PropertySchema { name: "y_domain",    value_type: ValueType::Vec2,           flags: BUILD_TIME_ONLY,                     field: ActorField::PlotDomainGroup,      group: Some(GroupMembership { group_id: PlotDomain }) },

    // ── Scene-level ──
    PropertySchema { name: "background_color", value_type: ValueType::Color,     flags: ANIMATED | ASSIGNABLE | INJECTABLE,  field: ActorField::Color,                group: None },
];

/// Lookup a property schema by name. O(log n) binary search.
pub(crate) fn lookup_property(name: &str) -> Option<&'static PropertySchema> {
    PROPERTY_REGISTRY.binary_search_by_key(&name, |s| s.name).ok().map(|i| &PROPERTY_REGISTRY[i])
}
```

### 2.2 Per-Actor-Kind Property Validity

The registry is the universal set. Each actor kind declares which subset is valid:

```rust
impl ActorKindId {
    /// Returns the set of properties valid for this actor kind,
    /// referenced by index into PROPERTY_REGISTRY.
    pub(crate) fn allowed_properties(&self) -> &'static [usize] {
        use ActorKindId::*;
        match self {
            Shape(kind) => shape_allowed(kind),
            Text => &TEXT_PROPS,
            Math => &TEXT_PROPS,
            Code => &TEXT_PROPS,
            Image => &IMAGE_PROPS,
            Svg => &SVG_PROPS,
            Graph => &GRAPH_PROPS,
            CartesianPlot | PolarPlot | ParametricPlot | ImplicitPlot => &PLOT_CURVE_PROPS,
            Row | Col => &CONTAINER_PROPS,
            Grid => &GRID_PROPS,
            Stack => &STACK_PROPS,
            Group => &[],
        }
    }
}

/// Index constants — generated by a build script or macro,
/// but shown here explicitly for clarity.
const SIZE_IDX: usize = 3;
const COLOR_IDX: usize = 4;
// ...

static SHAPE_PROPS: &[usize] = &[
    POSITION_IDX, SIZE_IDX, COLOR_IDX, OPACITY_IDX,
    STROKE_WIDTH_IDX, STROKE_COLOR_IDX, STROKE_PROGRESS_IDX, FILL_OPACITY_IDX,
    ROTATION_IDX, SCALE_IDX,
    // Shape-specific:
    RADIUS_IDX, RADIUS_X_IDX, RADIUS_Y_IDX,
    FROM_IDX, TO_IDX, START_ANGLE_IDX, SWEEP_ANGLE_IDX,
    POINTS_IDX, COMMANDS_IDX,
    SIDES_IDX, TIP_LENGTH_IDX, TIP_WIDTH_IDX,
    // Shape specific omit text/media/plot props  ← compiler-enforced via disjoint index sets
];

static TEXT_PROPS: &[usize] = &[
    POSITION_IDX, COLOR_IDX, OPACITY_IDX, ROTATION_IDX, SCALE_IDX,
    TEXT_IDX, FONT_SIZE_IDX, FONT_FAMILY_IDX,
];

static CONTAINER_PROPS: &[usize] = &[
    SIZE_IDX, GAP_IDX, ALIGN_IDX, // Cols only for Grid
];
```

**Diagnostic benefit:** If a user writes `radius` on a `Text` actor, the engine looks up `radius` in the global registry (found), then checks `Text`'s allowed set (not found), and emits:

```
Error: Property 'radius' is not valid on 'Text'. Valid properties for 'Text' are:
  at, anchor, offset, position, color, opacity, rotation, scale, text, font_size, font_family
```

---

## 3. Three-Tier Storage Architecture

The `AnimationTrack` monolithic struct is split into three storage tiers plus a kind-specific payload enum:

```rust
// ───────────────────────────────────────────────────────────
// File: timeline/track.rs (restructured)
// ───────────────────────────────────────────────────────────

/// Stable, compile-time constant identifying an actor's type.
/// Set once at first declaration and never changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ActorKindId {
    Shape(ShapeKind),
    Text,
    Math,
    Code,
    Image,
    Svg,
    Graph,
    CartesianPlot,
    PolarPlot,
    ParametricPlot,
    ImplicitPlot,
    Row,
    Col,
    Grid,
    Stack,
    Group,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShapeKind {
    Rect, Circle, Ellipse, Line, Arc, Polygon, Path, Arrow,
    Dot, Square, RegularPolygon,
}

// ───────────────────────────────────────────────────────────

/// Tier 1: Always-present header.
/// Zero-heap-overhead metadata about the actor.
struct ActorHeader {
    label: String,
    kind: ActorKindId,
    first_seen_ms: u64,
    children: Vec<String>,
}

// ───────────────────────────────────────────────────────────

/// Tier 2a: Universal geometry/transform properties.
/// Every visible actor has these, stored directly on the track.
struct GeometryTier {
    position: Option<PropertyTrack<[f32; 2]>>,
    motion_offset: Option<PropertyTrack<[f32; 2]>>,
    size: Option<PropertyTrack<[f32; 2]>>,
    /// Layout half-extents, computed by parent container.
    /// `None` = not yet seeded by layout (replaces LayoutSizeState::Unseeded).
    layout_size: Option<PropertyTrack<[f32; 2]>>,
    rotation: Option<PropertyTrack<f32>>,
    scale: Option<PropertyTrack<f32>>,
    placement_mode: Option<PropertyTrack<PlacementMode>>,
    position_binding: Option<PropertyTrack<PositionBinding>>,
}

/// Tier 2b: Universal appearance properties.
struct StyleTier {
    color: Option<PropertyTrack<[f32; 4]>>,
    opacity: Option<PropertyTrack<f32>>,
    stroke_width: Option<PropertyTrack<f32>>,
    stroke_color: Option<PropertyTrack<[f32; 4]>>,
    stroke_progress: Option<PropertyTrack<f32>>,
    fill_opacity: Option<PropertyTrack<f32>>,
    morph_options: Option<PropertyTrack<MorphOptions>>,
}

/// Tier 3: Kind-specific payload.
/// Only ONE variant is inhabited per actor.
enum ActorPayload {
    /// No payload (Group container, theoretical future actor kinds)
    Empty,

    /// Shape types: Circle, Rect, Line, Arc, Polygon, Path, Arrow, etc.
    Shape {
        shape_type: Option<PropertyTrack<ShapeType>>,
        line_from: Option<PropertyTrack<[f32; 2]>>,
        line_to: Option<PropertyTrack<[f32; 2]>>,
        arc_angles: Option<PropertyTrack<[f32; 2]>>,
        points: Option<PropertyTrack<Vec<[f32; 2]>>>,
        /// Computed rendering paths. These are the canonical output
        /// that the renderer draws. Updated by VectorShapeState at keyframe boundaries.
        vector_paths: Option<PropertyTrack<Vec<VelloPath>>>,
    },

    /// Text, Math, Code
    Text {
        content: Option<PropertyTrack<String>>,
        text_paths: Option<PropertyTrack<Vec<TextPath>>>,
    },

    /// Image
    Image {
        image: Option<PropertyTrack<Option<SceneImage>>>,
    },

    /// Svg (paths are pre-rendered at build time, stored inline)
    Svg {
        svg_paths: Vec<VelloPath>,
    },

    /// Plot actors (Graph, CartesianPlot, PolarPlot, ParametricPlot, ImplicitPlot)
    Plot {
        vector_paths: Option<PropertyTrack<Vec<VelloPath>>>,
    },
}

/// The complete actor storage — replaces the old flat AnimationTrack.
struct AnimationTrack {
    header: ActorHeader,
    geometry: GeometryTier,
    style: StyleTier,
    payload: ActorPayload,
}
```

### 3.1 What this changes

| Current | Replaced by | Impact |
|---------|-------------|--------|
| `AnimationTrack.label` | `header.label` | Mechanical rename |
| `AnimationTrack.first_seen_ms` | `header.first_seen_ms` | Mechanical rename |
| `AnimationTrack.children` | `header.children` | Mechanical rename |
| `LayoutSizeState` enum | `geometry.layout_size: Option<PropertyTrack<[f32;2]>>` | Eliminates bespoke enum + 3 special methods |
| `shape_type`, `line_from`, `line_to`, `arc_angles`, `points`, `vector_paths` | `payload.Shape { .. }` fields | Compiler-enforced mutual exclusion with Text/Image/Svg |
| `text_content`, `text_paths` | `payload.Text { .. }` | Compiler-enforced mutual exclusion with Shape/Image |
| `image` | `payload.Image { .. }` | Compiler-enforced mutual exclusion with Shape/Text |
| `svg_paths` | `payload.Svg { .. }` | Compiler-enforced mutual exclusion with Shape/Text |
| No `kind` field | `header.kind: ActorKindId` | Enables cheap runtime dispatch without string/primitive-descriptor |
| `PrimitiveDescriptor::for_actor_type()` | `ActorKindId` (stored inline) | O(1) lookup vs O(n) string match |
| `evaluate_vector_paths()` | `payload.Shape { vector_paths }` or `payload.Text { text_paths }` | Exhaustive match at render time |

### 3.2 LayoutSizeState elimination

Before:
```rust
enum LayoutSizeState {
    Unseeded,
    Seeded(PropertyTrack<[f32; 2]>),
}

impl LayoutSizeState {
    fn preserve_instant_delayed_value(&mut self, default, t) { ... }
    fn ensure(&mut self, default) -> &mut PropertyTrack { ... }
}

// Plus three ad-hoc methods on AnimationTrack:
//   track.layout_size_get(t) → Option<[f32;2]>
//   track.ensure_layout_size(default) → &mut PropertyTrack
//   track.preserve_instant_delayed_value(default, t)
```

After:
```rust
/// geometry.layout_size is Option<PropertyTrack<[f32; 2]>>
/// None = unseeded, Some(..) = seeded by layout.

// The generic TrackAccessor trait already handles Option<PropertyTrack<T>>:
impl TrackAccessor<[f32; 2]> for Option<PropertyTrack<[f32; 2]>> {
    fn get(&self, t, default) -> T { /* as today */ }
    fn ensure(&mut self, default) -> &mut PropertyTrack<T> { /* as today */ }
    // ...
}

// No special-casing anywhere. Layout writes:
track.geometry.layout_size.ensure([50.0, 50.0]).add_keyframe(t, size, easing);

// Renderer reads:
let size = track.geometry.layout_size.get(t, [50.0, 50.0]);
```

---

## 4. The Generic Property Engine

With the schema and tiered storage in place, all property writes flow through **one** dispatch function:

```rust
/// Apply a property at DECLARATION time (inside an actor declaration).
/// Handles keyframe timing, group resolution, defaults, and diagnostics.
pub(crate) fn process_declaration_property(
    track: &mut AnimationTrack,
    prop_name: &str,
    prop_value: &Expr,
    env: &Environment,
    ctx: &mut DeclarationContext,  // collects groups, timing info, diagnostics
    diag: &mut Vec<Diagnostic>,
) {
    let schema = match lookup_property(prop_name) {
        Some(s) => s,
        None => {
            // Unknown property — report diagnostic
            diag.push(Diagnostic::warning(/* ... */));
            return;
        }
    };

    // Check if this property is valid for this actor kind.
    if !track.header.kind.allowed_properties().contains(&schema) {
        diag.push(Diagnostic::error(format!(
            "'{prop_name}' is not valid on '{:?}' actors",
            track.header.kind
        )));
        return;
    }

    // Compound properties are deferred until all group members are collected.
    if let Some(group) = &schema.group {
        ctx.deferred_groups
            .entry(group.group_id)
            .or_default()
            .push((prop_name, prop_value));
        return;
    }

    // BUILD_TIME_ONLY properties execute immediately as side effects.
    if !schema.flags.contains(PropertyFlags::ANIMATED) {
        execute_build_time_property(track, schema, prop_value, env, ctx, diag);
        return;
    }

    // Standard ANIMATED property — the 80% case.
    // Read current value, write start+end keyframes.
    let target = parse_value(schema.value_type, prop_value, env, diag);

    if ctx.duration_ms > 0 {
        snapshot_value(track, schema, ctx.t_start_ms);
    } else if ctx.delay_ms > 0 {
        preserve_delayed_value(track, schema, ctx.t_start_ms);
    }

    write_keyframe(track, schema, ctx.t_end_ms, target, ctx.easing);
}
```

> **The 7 match blocks collapse into this one function.** Adding a property is adding a row to `PROPERTY_REGISTRY` — the engine handles it automatically for most properties. For compound properties, add a `GroupHandlerId` variant + one handler implementation.

### 4.1 The Assignment Engine

Assignment gets the same treatment:

```rust
/// Run-time property assignment (the `=` syntax).
/// Reuses the same schema → field mapping as declaration.
pub(crate) fn process_assignment_property(
    track: &mut AnimationTrack,
    schema: &PropertySchema,
    prop_value: &Expr,
    env: &Environment,
    timing: &ParsedTimingModifiers,
    diag: &mut Vec<Diagnostic>,
) {
    // Assignments don't do build-time properties, groups, or scheme defaults.
    if !schema.flags.contains(PropertyFlags::ASSIGNABLE) {
        diag.push(Diagnostic::warning(format!(
            "'{}' cannot be assigned at runtime", schema.name
        )));
        return;
    }

    let target = parse_value(schema.value_type, prop_value, env, diag);

    if timing.duration_ms > 0 {
        snapshot_value(track, schema, timing.t_start_ms);
    } else if timing.delay_ms > 0 {
        preserve_delayed_value(track, schema, timing.t_start_ms);
    }

    write_keyframe(track, schema, timing.t_end_ms, target, timing.easing);
}
```

### 4.2 The Inject Engine (runtime environment)

```rust
/// Inject all tracked values into the runtime Environment so
/// expressions can reference `label.prop`.
///
/// This is a single dispatch over ActorField — no match over property names.
pub(crate) fn inject_properties_into_env(
    env: &mut Environment,
    label: &str,
    track: &AnimationTrack,
    time_ms: u64,
    scene_dims: Option<SceneDimensions>,
) {
    // ── Geometry tier ──
    inject_vec2(env, label, "at",        track.geometry.position, time_ms, [0.0, 0.0]);
    inject_vec2(env, label, "position",  track.geometry.position, time_ms, [0.0, 0.0]);
    inject_vec2(env, label, "shift",     track.geometry.motion_offset, time_ms, [0.0, 0.0]);
    inject_vec2(env, label, "size",      track.geometry.size, time_ms, [50.0, 50.0]);
    inject_scalar(env, label, "width",   track.geometry.size, time_ms, 100.0, |s| s[0]);
    inject_scalar(env, label, "height",  track.geometry.size, time_ms, 100.0, |s| s[1]);
    inject_scalar(env, label, "rotation", track.geometry.rotation, time_ms, 0.0);
    inject_scalar(env, label, "scale",   track.geometry.scale, time_ms, 1.0);

    // ── Style tier ──
    inject_color(env, label, "color",    track.style.color, time_ms, [1.0; 4]);
    inject_scalar(env, label, "opacity", track.style.opacity, time_ms, 1.0);
    inject_color(env, label, "stroke_color", track.style.stroke_color, time_ms, [1.0; 4]);
    inject_scalar(env, label, "stroke_width", track.style.stroke_width, time_ms, 2.0);
    inject_scalar(env, label, "stroke_progress", track.style.stroke_progress, time_ms, 1.0);
    inject_scalar(env, label, "fill_opacity", track.style.fill_opacity, time_ms, 1.0);

    // ── Payload tier ──
    match &track.payload {
        ActorPayload::Shape { arc_angles, .. } => {
            let [start, sweep] = arc_angles.get(time_ms, [0.0, std::f32::consts::PI]);
            env.set(&format!("{label}.start_angle"), Value::Num(start as f64));
            env.set(&format!("{label}.sweep_angle"), Value::Num(sweep as f64));

            let radius = track.geometry.size.get(time_ms, [50.0, 50.0])[0];
            env.set(&format!("{label}.radius"), Value::Num(radius as f64));
            env.set(&format!("{label}.radius_x"), Value::Num(radius as f64));
            env.set(&format!("{label}.radius_y"), Value::Num(radius as f64));
        }
        ActorPayload::Text { .. } => {
            let pos = track.geometry.position.get(time_ms, [0.0, 0.0]);
            env.set(&format!("{label}.radius"), Value::Num(pos[0] as f64));
        }
        _ => {}
    }
}
```

---

## 5. Group Handlers (Compound Resolution)

Group handlers are the mechanism for properties that need **cross-property coordination** before producing their effect. Each handler is a distinct function invoked once all group members have been collected:

```rust
// ───────────────────────────────────────────────────────────
// File: timeline/property_groups.rs
// ───────────────────────────────────────────────────────────

/// All collected values for a group resolution.
struct PropertyGroup {
    /// The name of this group (one of the GroupHandlerId variants).
    handler: GroupHandlerId,
    /// Raw parsed properties, in declaration order.
    props: Vec<(/* prop_name */ &'static str, /* parsed_value */ PropertyValue)>,
}

pub(crate) fn resolve_groups(
    track: &mut AnimationTrack,
    groups: Vec<PropertyGroup>,
    env: &Environment,
    ctx: &DeclarationContext,
    diag: &mut Vec<Diagnostic>,
) {
    for group in groups {
        match group.handler {
            GroupHandlerId::PositionBinding => {
                resolve_position_binding(track, &group.props, env, ctx, diag);
            }
            GroupHandlerId::VectorShapeState => {
                resolve_vector_shape_state(track, &group.props, env, ctx, diag);
            }
            GroupHandlerId::PlotDomain => {
                resolve_plot_domain(track, &group.props, env, ctx, diag);
            }
            GroupHandlerId::ContainerLayout => {
                resolve_container_layout(track, &group.props, env, ctx, diag);
            }
        }
    }
}

// ── Example: PositionBinding handler ──

fn resolve_position_binding(
    track: &mut AnimationTrack,
    props: &[(/* name */ &'static str, PropertyValue)],
    env: &Environment,
    ctx: &DeclarationContext,
    diag: &mut Vec<Diagnostic>,
) {
    let mut at_expr: Option<Expr> = None;
    let mut anchor_expr: Option<Expr> = None;
    let mut offset_expr: Option<Expr> = None;

    // Collect the three contributing properties
    for (name, _val) in props {
        // (Actually we store the raw Expr, because position binding
        //  resolution needs the unparsed expression. Group captures
        //  preserve the Expr, not the parsed value, for this reason.)
        match *name {
            "at" => at_expr = ...,
            "anchor" => anchor_expr = ...,
            "offset" => offset_expr = ...,
            _ => unreachable!(),
        }
    }

    // One call, exactly as today — but the call site exists only here.
    if let Some((binding, bound_pos)) = resolve_position_binding_with_lookup_diagnostic(
        at_expr.as_ref(),
        anchor_expr.as_ref(),
        offset_expr.as_ref(),
        env,
        diag,
        &track.header.label,
    ) {
        preserve_discrete_position_state_before(track, ctx.t_start_ms);
        set_track_position_binding(track, ctx.t_start_ms, binding);
        if let Some(bound_pos) = bound_pos {
            mark_track_manual_position(track, ctx.t_start_ms);
            track.geometry.position.ensure([0.0, 0.0])
                .add_keyframe(ctx.t_start_ms, bound_pos, Easing::Linear);
        } else {
            mark_track_manual_position(track, ctx.t_start_ms);
        }
    }
}
```

**Why groups aren't callback-based:** `GroupHandlerId` is an enum with ~5 variants. Adding a new group is:
1. Add a variant to `GroupHandlerId`
2. Implement the handler function
3. Add one arm to `resolve_groups()` — compiler warns if you forget
4. Associate properties with the group in `PROPERTY_REGISTRY`

No dynamic dispatch, no trait erasure, no `Box<dyn Fn>`.

---

## 6. Extensibility Guarantees

### 6.1 Adding a new simple property

> Example: `border_radius: F32` for `Rect`.

| Step | Changes | Files |
|------|---------|-------|
| 1 | Add `BorderRadius` to `ActorField` enum | `track.rs` |
| 2 | Add storage field in `GeometryTier` or `ActorPayload::Shape` | `track.rs` |
| 3 | Add one row to `PROPERTY_REGISTRY` | `property_registry.rs` |
| 4 | Add index to `Shape`'s `allowed_properties` list | `actor_kind.rs` |
| 5 | Handle `border_radius` in `inject_properties_into_env` (if injectable) | `runtime.rs` |
| **Total** | **5 lines across 4 files** | |
| **Old system** | ~7 match arms across 5 files, plus tests | |

### 6.2 Adding a new actor type

> Example: `Video` actor with `url`, `start_time`, `loop`, `volume`.

| Step | Changes | Files |
|------|---------|-------|
| 1 | Add `Video` variant to `ActorKindId` | `actor_kind.rs` |
| 2 | Add `ActorPayload::Video { .. }` variant | `track.rs` |
| 3 | Define storage fields in the payload variant | `track.rs` |
| 4 | Add properties to `PROPERTY_REGISTRY` | `property_registry.rs` |
| 5 | Add `VIDEO_PROPS` index list to `ActorKindId::allowed_properties()` | `actor_kind.rs` |
| 6 | Implement `ActorKind` trait (build-time dispatch) | `actor_kind.rs` |
| 7 | Add renderer match arm in `scene_eval.rs` for the new payload | `scene_eval.rs` |
| **Total** | **7 touch points** — all **compiler-enforced** | |
| **Old system** | ~7+ files, each requiring manual match-arm insertion, **no compiler help** | |

**The compiler catches misses:** When you add `ActorPayload::Video`, the renderer `match` on `&track.payload` gets a non-exhaustive pattern error. When you add `GroupHandlerId::VideoConfig`, `resolve_groups` also gets a non-exhaustive match. You cannot forget to wire up a new type.

### 6.3 Adding a per-subsystem specialization

Some properties need custom behavior in specific subsystems — the schema handles this via **small enums per subsystem**, not callbacks:

```rust
// ── GUI subsystem ──

/// How a property should be rendered in the GUI inspector.
/// Default is derived from ValueType; override when needed.
enum GuiControlKind {
    Default,              // Derive from ValueType
    ColorPicker,
    NumericInput { min: f64, max: f64, step: f64 },
    Vec2Input { min: f64, max: f64 },
    Dropdown(&'static [&'static str]),
    FilePicker { extensions: &'static [&'static str] },
    Compound(&'static str),  // Name of custom widget
}

// Extended schema for the GUI view:
struct GuiPropertyView {
    control: GuiControlKind,
    category: &'static str,   // "Geometry" | "Appearance" | "Text" | "Container"
    help_text: &'static str,
}

// ── Renderer subsystem ──

/// What the renderer should do with this property's value at draw time.
/// Most properties are NOT rendered directly — they contribute through
/// vector_paths, text_paths, or image. Only rendering-relevant properties appear.
enum RenderBehavior {
    /// Property contributes to the scene indirectly (most common).
    Indirect,
    /// Property is a vector path source to submit.
    DrawVectorPaths,
    /// Property is text glyphs to submit.
    DrawTextPaths,
    /// Property is an image to blit.
    DrawImage,
}

// ── Per-subsystem registry extension ──

/// Subsystem-specific views attached to each property.
/// These are sparse — most properties use Default for everything.
struct PropertySubsystemView {
    gui: GuiPropertyView,
    render: RenderBehavior,
    // Future: accessibility, serialization, etc.
}

/// The full property descriptor, combining schema + subsystem views.
struct FullPropertyDescriptor {
    schema: &'static PropertySchema,
    subsystems: PropertySubsystemView,
}
```

**The `RenderBehavior` enum lives in one file and has one match in the renderer.** Adding a new render behavior is adding one variant + one match arm in `scene_eval.rs`. Not 7 match blocks.

---

## 7. Migration Path

### Phase 1: Schema + Registry (backward compatible)

1. Create `property_registry.rs` with `PropertySchema`, `ValueType`, `ActorField`, `PropertyFlags`, `GroupHandlerId`
2. Create `lookup_property()` — static binary search over `PROPERTY_REGISTRY`
3. Create `parse_value()` — generic `Expr → PropertyValue` dispatch on `ValueType`
4. **No behavioral changes yet** — the registry exists alongside the old match blocks

### Phase 2: Tiered storage layout

1. Restructure `AnimationTrack` into `header + geometry + style + payload`
2. Replace `LayoutSizeState` with `Option<PropertyTrack<[f32;2]>>`
3. Keep backward-compat accessor methods so the old match blocks still compile
4. Add `ActorKindId` field — populate it at track creation time

### Phase 3: Generic engine (switch-over)

1. Write `process_declaration_property()` — the unified dispatch
2. Write `GroupHandler` implementations (one per `GroupHandlerId`)
3. One-by-one, migrate each `process_body`'s property match to call the engine instead
4. Delete old match blocks as they become dead code
5. Migrate `process_assignment_statement()` to use the engine
6. Migrate `process_text_declaration()` and `process_media_statement()`

**Each step is independently testable** — property-by-property, you can verify that the engine produces identical keyframes to the old match blocks.

### Phase 4: Cleanup

1. Remove backward-compat accessor methods
2. Remove `PrimitiveDescriptor::for_actor_type()` — replace all call sites with `ActorKindId`
3. Delete the now-unused old match-block code
4. Delete `VectorShapeState` if its logic is fully subsumed by `GroupHandlerId::VectorShapeState`

---

## 8. Testing Strategy

| Layer | What to test | How |
|-------|-------------|-----|
| **Schema** | Every property in `PROPERTY_REGISTRY` has valid field, correct flags, sorted order | Static assertion test (`#![test] all properties sorted`) |
| **Parser** | `parse_value()` correctly handles all `ValueType` variants | Property-level round-trip tests |
| **Engine** | For each property, engine produces identical keyframes to old match block | Snapshot test: same AST → compare keyframe maps |
| **Groups** | Compound resolution produces correct results for every `GroupHandlerId` | Integration test: `at + anchor + offset → correct PositionBinding` |
| **Renderer** | Each `ActorPayload` variant renders the right thing | Visual regression test |
| **GUI** | Each `GuiControlKind` produces the correct widget | Egui UI snapshot test |

---

## Appendix: Key File Changes

| File | Action | Purpose |
|------|--------|---------|
| `timeline/track.rs` | Restructure | Three-tier storage + ActorPayload enum |
| `timeline/mod.rs` | Add module | `pub mod property_registry;` |
| `timeline/property_registry.rs` | **New** | Schema definitions + `PROPERTY_REGISTRY` + `lookup_property()` |
| `timeline/property_groups.rs` | **New** | `GroupHandlerId` enum + `resolve_groups()` + handler implementations |
| `timeline/actor_kind.rs` | Extend | `ActorKindId` enum + `allowed_properties()` + `ShapeKind` enum |
| `timeline/build.rs` | Refactor | Replace 3 match blocks with `process_declaration_property()` calls |
| `timeline/assignments.rs` | Refactor | Replace 1 match block with `process_assignment_property()` calls |
| `timeline/declarations_text.rs` | Refactor | Replace 1 match block with engine calls |
| `timeline/media.rs` | Refactor | Replace 1 match block with engine calls |
| `timeline/runtime.rs` | Refactor | `inject_properties_into_env()` using `ActorField` dispatch |
| `timeline/scene_eval.rs` | Refactor | Render dispatch via `match track.payload { .. }` |
| `timeline/utils.rs` | Remove? | `resolve_color_in_env` → folded into `parse_value()` for `ValueType::Color` |
