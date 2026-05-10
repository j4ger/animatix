//! # Property Registry
//!
//! The canonical schema system for all actor properties.
//!
//! Every property in Animatix is described by a `PropertySchema` entry in the
//! static `PROPERTY_REGISTRY`. Each schema specifies:
//!
//! - `name` — Canonical source-text name (`"color"`, `"radius"`, `"gap"`)
//! - `value_type` — Which rust type the property value carries
//! - `flags` — Feature flags (ANIMATED, ASSIGNABLE, INJECTABLE, LAYOUT_AFFECTING)
//! - `field` — Which `ActorField` storage location this maps to
//! - `group` — For compound properties: which resolution group they belong to
//!
//! ## Dispatch flow
//!
//! Instead of 7+ match blocks over string property names, all property dispatch
//! goes through two steps:
//!
//! 1. `lookup_property(name)` → `&PropertySchema`  (O(log n) binary search)
//! 2. Match over `schema.field` / `schema.flags` / `schema.group`  (exhaustive enum)
//!
//! ## Adding a new property
//!
//! 1. Add an `ActorField` variant if a new storage field is needed
//! 2. Add storage to the appropriate tier in `track.rs`
//! 3. Add a row to `PROPERTY_REGISTRY`
//! 4. Add the property index to the actor kind's `allowed_properties()` list
//!
//! For simple animated properties (80% of cases), that's all — the generic engine
//! handles parsing, keyframing, assignments, and environment injection automatically.



// ─────────────────────────────────────────────────────────────
// Value types
// ─────────────────────────────────────────────────────────────

/// The set of all value types a property can carry.
///
/// This enum drives parsing, interpolation, default selection, and injection.
/// Adding a new variant is rare — only when a fundamentally new kind of
/// property value is introduced.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueType {
    F32,
    U32,
    Vec2,
    Vec4,
    Color,
    String,
    ShapeType,
    PlacementMode,
    SceneAnchor,
    PositionBinding,
    MorphOptions,
    PointList,
    CommandList,
    /// A property that produces builder-time side effects (no animated value).
    BuildTimeOnly,
}

// ─────────────────────────────────────────────────────────────
// Property flags
// ─────────────────────────────────────────────────────────────

/// Feature flags that change how the engine processes a property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PropertyFlags(u8);

impl PropertyFlags {
    pub const ANIMATED: Self = Self(0b0001);
    pub const ASSIGNABLE: Self = Self(0b0010);
    pub const INJECTABLE: Self = Self(0b0100);
    pub const LAYOUT_AFFECTING: Self = Self(0b1000);

    // Convenience combinations for use in static PROPORTY_REGISTRY
    pub const ASSIGNABLE_A: Self = Self(0b0011);     // ANIMATED | ASSIGNABLE
    pub const ASSIGNABLE_AI: Self = Self(0b0111);    // ANIMATED | ASSIGNABLE | INJECTABLE
    pub const ANIMATED_I: Self = Self(0b0101);       // ANIMATED | INJECTABLE
    pub const ALL: Self = Self(0b1111);              // all flags

    pub const fn empty() -> Self { Self(0) }

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Combine two flag sets (const, usable in statics).
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

// ─────────────────────────────────────────────────────────────
// Storage field identifier
// ─────────────────────────────────────────────────────────────

/// Identifies which storage location a property maps to.
///
/// This is a flat enum over ALL possible storage fields across all three tiers.
/// The engine uses a single match over `ActorField` to dispatch reads and writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActorField {
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

    // ── Shape payload ──
    ShapeType,
    LineFrom,
    LineTo,
    ArcAngles,
    Points,
    VectorPaths,

    // ── Text payload ──
    TextContent,
    TextPaths,
    FontFamily,
    FontSize,

    // ── Media payload ──
    ImageData,
    SvgPaths,

    // ── Compound resolution groups (handled by GroupHandler) ──
    PositionBindingGroup,
    VectorShapeGroup,
    PlotDomainGroup,
    ContainerLayoutGroup,
}

// ─────────────────────────────────────────────────────────────
// Group resolution
// ─────────────────────────────────────────────────────────────

/// Identifies a compound resolution handler.
///
/// Several properties depend on each other and must be resolved together.
/// Each group variant has one handler function in `property_groups.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GroupHandlerId {
    /// at + anchor + offset → PositionBinding
    PositionBinding,
    /// radius, sides, from, to, start_angle, sweep_angle, points, commands
    VectorShapeState,
    /// x_domain, y_domain, t_domain, func, tolerance, max_depth, resolution
    PlotDomain,
    /// gap, align, cols → container layout metadata
    ContainerLayout,
}

/// Describes a property's membership in a compound resolution group.
#[derive(Clone, Copy, Debug)]
pub struct GroupMembership {
    pub group_id: GroupHandlerId,
}

// ─────────────────────────────────────────────────────────────
// Property schema
// ─────────────────────────────────────────────────────────────

/// The complete description of one property in the system.
///
/// This is pure data — no function pointers. All dispatch logic is driven
/// by matching over the enum fields (ValueType, ActorField, GroupHandlerId).
#[derive(Clone, Copy, Debug)]
pub struct PropertySchema {
    /// Canonical name as it appears in source text.
    pub name: &'static str,

    /// The value type determines parsing, interpolation, default, and injection.
    pub value_type: ValueType,

    /// Feature flags.
    pub flags: PropertyFlags,

    /// Which storage field or side-effect handler this property maps to.
    pub field: ActorField,

    /// For compound properties: which resolution group this belongs to.
    /// None for simple independent properties.
    pub group: Option<GroupMembership>,
}

// ─────────────────────────────────────────────────────────────
// The registry — sorted by name for binary search
// ─────────────────────────────────────────────────────────────

/// The complete, authoritative registry of every property in Animatix.
///
/// **Must be sorted by `.name`** for `lookup_property()` binary search.
/// A `#[test]` below verifies this invariant.
use PropertyFlags as F;

pub static PROPERTY_REGISTRY: &[PropertySchema] = &[
    // ── Universal geometry ──
    PropertySchema { name: "align",        value_type: ValueType::String,     flags: F::empty(),                  field: ActorField::ContainerLayoutGroup, group: Some(GroupMembership { group_id: GroupHandlerId::ContainerLayout }) },
    PropertySchema { name: "anchor",       value_type: ValueType::SceneAnchor,flags: F::ASSIGNABLE_AI,             field: ActorField::PositionBindingGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PositionBinding }) },
    PropertySchema { name: "arc_angles",   value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_A,             field: ActorField::ArcAngles, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "at",           value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_AI,             field: ActorField::PositionBindingGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PositionBinding }) },
    PropertySchema { name: "background_color", value_type: ValueType::Color,  flags: F::ASSIGNABLE_AI,             field: ActorField::Color, group: None },
    PropertySchema { name: "code",         value_type: ValueType::String,     flags: F::ANIMATED,                 field: ActorField::TextContent, group: None },
    PropertySchema { name: "color",        value_type: ValueType::Color,      flags: F::ASSIGNABLE_AI,             field: ActorField::Color, group: None },
    PropertySchema { name: "cols",         value_type: ValueType::U32,        flags: F::empty(),                  field: ActorField::ContainerLayoutGroup, group: Some(GroupMembership { group_id: GroupHandlerId::ContainerLayout }) },
    PropertySchema { name: "commands",     value_type: ValueType::CommandList,flags: F::empty(),                  field: ActorField::VectorShapeGroup, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "fill_opacity", value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::FillOpacity, group: None },
    PropertySchema { name: "font_family",  value_type: ValueType::String,     flags: F::ASSIGNABLE,               field: ActorField::FontFamily, group: None },
    PropertySchema { name: "font_size",    value_type: ValueType::F32,        flags: F::ASSIGNABLE_A,             field: ActorField::FontSize, group: None },
    PropertySchema { name: "from",         value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_AI,             field: ActorField::LineFrom, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "func",         value_type: ValueType::BuildTimeOnly, flags: F::empty(),               field: ActorField::PlotDomainGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }) },
    PropertySchema { name: "gap",          value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::ContainerLayoutGroup, group: Some(GroupMembership { group_id: GroupHandlerId::ContainerLayout }) },
    PropertySchema { name: "latex",        value_type: ValueType::String,     flags: F::ANIMATED,                 field: ActorField::TextContent, group: None },
    PropertySchema { name: "math",         value_type: ValueType::String,     flags: F::ANIMATED,                 field: ActorField::TextContent, group: None },
    PropertySchema { name: "max_depth",    value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::PlotDomainGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }) },
    PropertySchema { name: "offset",       value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_AI,             field: ActorField::PositionBindingGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PositionBinding }) },
    PropertySchema { name: "opacity",      value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Opacity, group: None },
    PropertySchema { name: "points",       value_type: ValueType::PointList,  flags: F::ASSIGNABLE_A,             field: ActorField::Points, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "position",     value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_AI,             field: ActorField::Position, group: None },
    PropertySchema { name: "radius",       value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Size, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "radius_x",     value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Size, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "radius_y",     value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Size, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "resolution",   value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::PlotDomainGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }) },
    PropertySchema { name: "rotation",     value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Rotation, group: None },
    PropertySchema { name: "scale",        value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Scale, group: None },
    PropertySchema { name: "sides",        value_type: ValueType::U32,        flags: F::empty(),                  field: ActorField::VectorShapeGroup, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "size",         value_type: ValueType::Vec2,       flags: F::ALL,                      field: ActorField::Size, group: None },
    PropertySchema { name: "start_angle",  value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::ArcAngles, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "stroke",       value_type: ValueType::Color,      flags: F::ASSIGNABLE_AI,             field: ActorField::StrokeColor, group: None },
    PropertySchema { name: "stroke_color", value_type: ValueType::Color,      flags: F::ASSIGNABLE_AI,             field: ActorField::StrokeColor, group: None },
    PropertySchema { name: "stroke_progress",value_type: ValueType::F32,      flags: F::ASSIGNABLE_AI,             field: ActorField::StrokeProgress, group: None },
    PropertySchema { name: "stroke_width", value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::StrokeWidth, group: None },
    PropertySchema { name: "sweep_angle",  value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::ArcAngles, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "t_domain",     value_type: ValueType::Vec2,       flags: F::empty(),                  field: ActorField::PlotDomainGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }) },
    PropertySchema { name: "text",         value_type: ValueType::String,     flags: F::ASSIGNABLE_A,             field: ActorField::TextContent, group: None },
    PropertySchema { name: "tip_length",   value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::VectorShapeGroup, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "tip_width",    value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::VectorShapeGroup, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "to",           value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_AI,             field: ActorField::LineTo, group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }) },
    PropertySchema { name: "tolerance",    value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::PlotDomainGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }) },
    PropertySchema { name: "width",        value_type: ValueType::F32,        flags: F::ASSIGNABLE_A,             field: ActorField::StrokeWidth, group: None },
    PropertySchema { name: "x_domain",     value_type: ValueType::Vec2,       flags: F::empty(),                  field: ActorField::PlotDomainGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }) },
    PropertySchema { name: "y_domain",     value_type: ValueType::Vec2,       flags: F::empty(),                  field: ActorField::PlotDomainGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }) },
];

// ─────────────────────────────────────────────────────────────
// Lookup
// ─────────────────────────────────────────────────────────────

/// Look up a property schema by name.
///
/// Uses binary search over the sorted `PROPERTY_REGISTRY`.
/// Returns `None` if no property with that name exists.
pub fn lookup_property(name: &str) -> Option<&'static PropertySchema> {
    PROPERTY_REGISTRY
        .binary_search_by_key(&name, |s| s.name)
        .ok()
        .map(|i| &PROPERTY_REGISTRY[i])
}

// ─────────────────────────────────────────────────────────────
// Per-actor-kind allowed property indices
// ─────────────────────────────────────────────────────────────

/// Convenience: build a sorted set of allowed property indices for an actor kind.
/// Returns indices into PROPERTY_REGISTRY.
pub fn allowed_property_indices(
    kind: super::ActorKindId,
    sub_kind: Option<super::ShapeKind>,
) -> Vec<usize> {
    use super::ActorKindId::*;
    use super::ShapeKind;

    // Build a set of property names that are universally valid.
    let mut names: Vec<&'static str> = Vec::new();

    // Universal geometry
    names.extend_from_slice(&["at", "anchor", "offset", "position", "rotation", "scale", "size"]);

    // Universal style (most actors have these)
    match kind {
        Group => {}
        _ => {
            names.extend_from_slice(&["color", "opacity", "fill_opacity"]);
        }
    }

    match kind {
        Shape(sk) => {
            names.extend_from_slice(&[
                "radius", "radius_x", "radius_y",
                "from", "to",
                "start_angle", "sweep_angle", "arc_angles",
                "points", "commands",
                "stroke", "stroke_color", "stroke_width", "stroke_progress",
                "sides", "tip_length", "tip_width",
                "width", // → stroke_width alias
            ]);
            if sk == ShapeKind::Path {
                names.push("commands");
            }
            if sk == ShapeKind::Arrow {
                names.extend_from_slice(&["tip_length", "tip_width"]);
            }
        }
        Text | Math | Code => {
            names.extend_from_slice(&["font_size", "font_family"]);
            if kind == Text {
                names.push("text");
            }
            if kind == Math {
                names.extend_from_slice(&["latex", "math"]);
            }
            if kind == Code {
                names.push("code");
            }
            names.extend_from_slice(&["stroke", "stroke_color", "stroke_width", "stroke_progress"]);
        }
        Image => {
            names.extend_from_slice(&["url", "size"]);
        }
        Svg => {
            names.extend_from_slice(&["url", "scale"]);
        }
        Graph => {
            names.extend_from_slice(&["x_domain", "y_domain", "size", "color"]);
        }
        CartesianPlot | PolarPlot | ParametricPlot | ImplicitPlot => {
            names.extend_from_slice(&[
                "x_domain", "y_domain", "t_domain",
                "func", "tolerance", "max_depth", "resolution",
                "stroke", "stroke_color", "stroke_width",
            ]);
        }
        Row | Col => {
            names.extend_from_slice(&["gap", "align", "size"]);
        }
        Grid => {
            names.extend_from_slice(&["gap", "align", "cols", "size"]);
        }
        Stack => {
            names.extend_from_slice(&["size"]);
        }
        Group => {
            // Group is transparent — no standard properties
        }
    }

    // Resolve to indices
    let mut indices: Vec<usize> = names
        .iter()
        .filter_map(|name| {
            PROPERTY_REGISTRY
                .binary_search_by_key(name, |s| s.name)
                .ok()
        })
        .collect();
    indices.sort_unstable();
    indices.dedup();
    indices
}

// ─────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the registry is sorted by name (required for binary search).
    #[test]
    fn registry_is_sorted() {
        for window in PROPERTY_REGISTRY.windows(2) {
            assert!(
                window[0].name <= window[1].name,
                "PROPERTY_REGISTRY is not sorted: '{}' should come before '{}'",
                window[0].name,
                window[1].name
            );
        }
    }

    /// Verify every property can be looked up by name.
    #[test]
    fn every_property_is_lookupable() {
        for schema in PROPERTY_REGISTRY {
            let found = lookup_property(schema.name);
            assert!(
                found.is_some(),
                "Property '{}' cannot be looked up by name",
                schema.name
            );
            assert_eq!(found.unwrap().name, schema.name);
        }
    }

    /// Verify aliases point to the same field.
    #[test]
    fn stroke_is_alias_for_stroke_color() {
        let stroke = lookup_property("stroke").unwrap();
        let stroke_color = lookup_property("stroke_color").unwrap();
        assert_eq!(stroke.field, stroke_color.field);
    }

    #[test]
    fn width_is_alias_for_stroke_width() {
        let width = lookup_property("width").unwrap();
        let stroke_width = lookup_property("stroke_width").unwrap();
        assert_eq!(width.field, stroke_width.field);
    }

    /// Verify that no property name is duplicated.
    #[test]
    fn no_duplicate_names() {
        let mut names: Vec<&str> = PROPERTY_REGISTRY.iter().map(|s| s.name).collect();
        let original_len = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), original_len, "Duplicate property names detected");
    }
}
