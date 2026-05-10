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

// ─────────────────────────────────────────────────────────────
// Applicability — which actor kinds a property is valid for
// ─────────────────────────────────────────────────────────────

/// Declares which actor kinds a property applies to.
///
/// This eliminates the need for a separate `allowed_property_indices` match
/// block.  When adding a new property, you specify its applicability right
/// here in the registry entry.  The inspector and keyframe table use
/// `schema.applicable.includes(kind)` to decide whether to show the property.
#[derive(Clone, Copy, Debug)]
pub enum Applicable {
    /// Applies to every actor kind including Group.
    Everything,
    /// Applies to all actor kinds except Group (style / size properties).
    EveryActorExceptGroup,
    /// Applies to all shape kinds.
    AllShapes,
    /// Applies to all shapes except Line (fill-related properties).
    AllShapesExceptLine,
    /// Applies to specific shape kinds.
    ShapeKinds(&'static [super::ShapeKind]),
    /// Applies to specific non-shape actor kinds.
    ActorKinds(&'static [super::ActorKindId]),
    /// Never shown in the inspector (build-time only, aliases, compounds).
    Never,
}

impl Applicable {
    pub fn includes(self, kind: super::ActorKindId) -> bool {
        use super::ActorKindId::*;
        use super::ShapeKind;
        match self {
            Applicable::Everything => true,
            Applicable::EveryActorExceptGroup => !matches!(kind, Group),
            Applicable::AllShapes => matches!(kind, Shape(_)),
            Applicable::AllShapesExceptLine => {
                matches!(kind, Shape(sk) if sk != ShapeKind::Line)
            }
            Applicable::ShapeKinds(kinds) => {
                matches!(kind, Shape(sk) if kinds.contains(&sk))
            }
            Applicable::ActorKinds(kinds) => kinds.contains(&kind),
            Applicable::Never => false,
        }
    }
}

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

    /// Which actor kinds this property is applicable to.
    pub applicable: Applicable,
}

// ─────────────────────────────────────────────────────────────
// The registry — sorted by name for binary search
// ─────────────────────────────────────────────────────────────

/// The complete, authoritative registry of every property in Animatix.
///
/// **Must be sorted by `.name`** for `lookup_property()` binary search.
/// A `#[test]` below verifies this invariant.
use PropertyFlags as F;
use super::ActorKindId as A;
use super::ShapeKind as S;

pub static PROPERTY_REGISTRY: &[PropertySchema] = &[
    PropertySchema { name: "align",        value_type: ValueType::String,     flags: F::empty(),                  field: ActorField::ContainerLayoutGroup, group: Some(GroupMembership { group_id: GroupHandlerId::ContainerLayout }), applicable: Applicable::ActorKinds(&[A::Row, A::Col, A::Grid]) },
    PropertySchema { name: "anchor",       value_type: ValueType::SceneAnchor,flags: F::ASSIGNABLE_AI,             field: ActorField::PositionBindingGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PositionBinding }), applicable: Applicable::Everything },
    PropertySchema { name: "arc_angles",   value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_A,             field: ActorField::ArcAngles,          group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::Never },
    PropertySchema { name: "at",           value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_AI,             field: ActorField::PositionBindingGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PositionBinding }), applicable: Applicable::Everything },
    PropertySchema { name: "background_color", value_type: ValueType::Color,  flags: F::ASSIGNABLE_AI,             field: ActorField::Color,              group: None,                             applicable: Applicable::Never },
    PropertySchema { name: "code",         value_type: ValueType::String,     flags: F::ANIMATED,                 field: ActorField::TextContent,        group: None,                             applicable: Applicable::ActorKinds(&[A::Code]) },
    PropertySchema { name: "color",        value_type: ValueType::Color,      flags: F::ASSIGNABLE_AI,             field: ActorField::Color,              group: None,                             applicable: Applicable::EveryActorExceptGroup },
    PropertySchema { name: "cols",         value_type: ValueType::U32,        flags: F::empty(),                  field: ActorField::ContainerLayoutGroup, group: Some(GroupMembership { group_id: GroupHandlerId::ContainerLayout }), applicable: Applicable::ActorKinds(&[A::Grid]) },
    PropertySchema { name: "commands",     value_type: ValueType::CommandList,flags: F::empty(),                  field: ActorField::VectorShapeGroup,    group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::Never },
    PropertySchema { name: "fill_opacity", value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::FillOpacity,        group: None,                             applicable: Applicable::AllShapesExceptLine },
    PropertySchema { name: "font_family",  value_type: ValueType::String,     flags: F::ASSIGNABLE,               field: ActorField::FontFamily,          group: None,                             applicable: Applicable::ActorKinds(&[A::Text, A::Math, A::Code]) },
    PropertySchema { name: "font_size",    value_type: ValueType::F32,        flags: F::ASSIGNABLE_A,             field: ActorField::FontSize,            group: None,                             applicable: Applicable::ActorKinds(&[A::Text, A::Math, A::Code]) },
    PropertySchema { name: "from",         value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_AI,             field: ActorField::LineFrom,           group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::ShapeKinds(&[S::Line, S::Arrow]) },
    PropertySchema { name: "func",         value_type: ValueType::BuildTimeOnly, flags: F::empty(),               field: ActorField::PlotDomainGroup,     group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), applicable: Applicable::ActorKinds(&[A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]) },
    PropertySchema { name: "gap",          value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::ContainerLayoutGroup, group: Some(GroupMembership { group_id: GroupHandlerId::ContainerLayout }), applicable: Applicable::ActorKinds(&[A::Row, A::Col, A::Grid]) },
    PropertySchema { name: "latex",        value_type: ValueType::String,     flags: F::ANIMATED,                 field: ActorField::TextContent,        group: None,                             applicable: Applicable::Never },
    PropertySchema { name: "math",         value_type: ValueType::String,     flags: F::ANIMATED,                 field: ActorField::TextContent,        group: None,                             applicable: Applicable::ActorKinds(&[A::Math]) },
    PropertySchema { name: "max_depth",    value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::PlotDomainGroup,     group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), applicable: Applicable::ActorKinds(&[A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]) },
    PropertySchema { name: "offset",       value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_AI,             field: ActorField::PositionBindingGroup, group: Some(GroupMembership { group_id: GroupHandlerId::PositionBinding }), applicable: Applicable::Everything },
    PropertySchema { name: "opacity",      value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Opacity,            group: None,                             applicable: Applicable::EveryActorExceptGroup },
    PropertySchema { name: "points",       value_type: ValueType::PointList,  flags: F::ASSIGNABLE_A,             field: ActorField::Points,             group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::Never },
    PropertySchema { name: "position",     value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_AI,             field: ActorField::Position,           group: None,                             applicable: Applicable::Everything },
    PropertySchema { name: "radius",       value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Size,               group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::ShapeKinds(&[S::Circle, S::Dot, S::RegularPolygon]) },
    PropertySchema { name: "radius_x",     value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Size,               group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::ShapeKinds(&[S::Ellipse, S::Arc]) },
    PropertySchema { name: "radius_y",     value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Size,               group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::ShapeKinds(&[S::Ellipse, S::Arc]) },
    PropertySchema { name: "resolution",   value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::PlotDomainGroup,     group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), applicable: Applicable::ActorKinds(&[A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]) },
    PropertySchema { name: "rotation",     value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Rotation,           group: None,                             applicable: Applicable::Everything },
    PropertySchema { name: "scale",        value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::Scale,              group: None,                             applicable: Applicable::Everything },
    PropertySchema { name: "sides",        value_type: ValueType::U32,        flags: F::empty(),                  field: ActorField::VectorShapeGroup,    group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::Never },
    PropertySchema { name: "size",         value_type: ValueType::Vec2,       flags: F::ALL,                      field: ActorField::Size,               group: None,                             applicable: Applicable::EveryActorExceptGroup },
    PropertySchema { name: "start_angle",  value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::ArcAngles,          group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::ShapeKinds(&[S::Arc]) },
    PropertySchema { name: "stroke",       value_type: ValueType::Color,      flags: F::ASSIGNABLE_AI,             field: ActorField::StrokeColor,        group: None,                             applicable: Applicable::AllShapes },
    PropertySchema { name: "stroke_color", value_type: ValueType::Color,      flags: F::ASSIGNABLE_AI,             field: ActorField::StrokeColor,        group: None,                             applicable: Applicable::Never },
    PropertySchema { name: "stroke_progress",value_type: ValueType::F32,      flags: F::ASSIGNABLE_AI,             field: ActorField::StrokeProgress,     group: None,                             applicable: Applicable::AllShapes },
    PropertySchema { name: "stroke_width", value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::StrokeWidth,        group: None,                             applicable: Applicable::AllShapes },
    PropertySchema { name: "sweep_angle",  value_type: ValueType::F32,        flags: F::ASSIGNABLE_AI,             field: ActorField::ArcAngles,          group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::ShapeKinds(&[S::Arc]) },
    PropertySchema { name: "t_domain",     value_type: ValueType::Vec2,       flags: F::empty(),                  field: ActorField::PlotDomainGroup,     group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), applicable: Applicable::ActorKinds(&[A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]) },
    PropertySchema { name: "text",         value_type: ValueType::String,     flags: F::ASSIGNABLE_A,             field: ActorField::TextContent,        group: None,                             applicable: Applicable::ActorKinds(&[A::Text]) },
    PropertySchema { name: "tip_length",   value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::VectorShapeGroup,    group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::ShapeKinds(&[S::Arrow]) },
    PropertySchema { name: "tip_width",    value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::VectorShapeGroup,    group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::ShapeKinds(&[S::Arrow]) },
    PropertySchema { name: "to",           value_type: ValueType::Vec2,       flags: F::ASSIGNABLE_AI,             field: ActorField::LineTo,             group: Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), applicable: Applicable::ShapeKinds(&[S::Line, S::Arrow]) },
    PropertySchema { name: "tolerance",    value_type: ValueType::F32,        flags: F::empty(),                  field: ActorField::PlotDomainGroup,     group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), applicable: Applicable::ActorKinds(&[A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]) },
    PropertySchema { name: "url",          value_type: ValueType::String,     flags: F::ASSIGNABLE,               field: ActorField::ImageData,           group: None,                             applicable: Applicable::ActorKinds(&[A::Image, A::Svg]) },
    PropertySchema { name: "width",        value_type: ValueType::F32,        flags: F::ASSIGNABLE_A,             field: ActorField::StrokeWidth,        group: None,                             applicable: Applicable::Never },
    PropertySchema { name: "x_domain",     value_type: ValueType::Vec2,       flags: F::empty(),                  field: ActorField::PlotDomainGroup,     group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), applicable: Applicable::ActorKinds(&[A::Graph, A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]) },
    PropertySchema { name: "y_domain",     value_type: ValueType::Vec2,       flags: F::empty(),                  field: ActorField::PlotDomainGroup,     group: Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), applicable: Applicable::ActorKinds(&[A::Graph, A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]) },
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
pub fn allowed_property_indices(kind: super::ActorKindId) -> Vec<usize> {
    PROPERTY_REGISTRY
        .iter()
        .enumerate()
        .filter(|(_, schema)| schema.applicable.includes(kind))
        .map(|(i, _)| i)
        .collect()
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
