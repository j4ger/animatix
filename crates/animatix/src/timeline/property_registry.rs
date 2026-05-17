//! # Property Registry
//!
//! The canonical schema system for all actor properties.
//!
//! Every property in Animatix is described by a `PropertySchema` entry in the
//! static `PROPERTY_REGISTRY`. Each schema specifies:
//!
//! - `name` — Canonical source-text name (`"color"`, `"radius"`, `"gap"`, `"padding"`)
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
    Commands,
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
    /// Applies to shapes and text/math/code (actors with fillable/colorable content).
    AllDrawables,
    /// Applies to shapes, image, plots, and containers (actors with meaningful bounds).
    SizedActors,
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
            Applicable::AllDrawables => {
                matches!(kind, Shape(_) | Text | Math | Code)
            }
            Applicable::SizedActors => {
                matches!(kind, Shape(_) | Image | Graph | CartesianPlot | PolarPlot | ParametricPlot | ImplicitPlot | Row | Col | Grid | Stack)
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

    /// Default value for this property when the actor does not declare it.
    /// Computed at runtime because some defaults depend on actor kind
    /// (e.g. `font_size` is 48 for Text, 36 for Math, 24 for Code).
    pub default_value: fn(super::ActorKindId) -> super::property_engine::PropertyValue,
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

macro_rules! schema {
    ($name:expr, $ty:expr, $flags:expr, $field:expr, $group:expr, $applicable:expr, $default:expr) => {
        PropertySchema {
            name: $name,
            value_type: $ty,
            flags: $flags,
            field: $field,
            group: $group,
            applicable: $applicable,
            default_value: $default,
        }
    };
}

pub static PROPERTY_REGISTRY: &[PropertySchema] = &[
    schema!("align",         ValueType::String,      F::empty(),                   ActorField::ContainerLayoutGroup, Some(GroupMembership { group_id: GroupHandlerId::ContainerLayout }), Applicable::ActorKinds(&[A::Row, A::Col, A::Grid]), |_| super::property_engine::PropertyValue::String("center".to_string())),
    schema!("anchor",        ValueType::SceneAnchor, F::ASSIGNABLE_AI,             ActorField::PositionBindingGroup, Some(GroupMembership { group_id: GroupHandlerId::PositionBinding }), Applicable::Everything, |_| super::property_engine::PropertyValue::String("center".to_string())),
    schema!("arc_angles",    ValueType::Vec2,        F::ASSIGNABLE_A,              ActorField::ArcAngles,           Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::Never, |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])),
    schema!("at",            ValueType::Vec2,        F::ASSIGNABLE_AI,             ActorField::PositionBindingGroup, Some(GroupMembership { group_id: GroupHandlerId::PositionBinding }), Applicable::Everything, |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])),
    schema!("background_color", ValueType::Color,    F::ASSIGNABLE_AI,             ActorField::Color,               None,                             Applicable::Never, |_| super::property_engine::PropertyValue::Color([0.0, 0.0, 0.0, 1.0])),
    schema!("code",          ValueType::String,      F::ANIMATED,                  ActorField::TextContent,         None,                             Applicable::ActorKinds(&[A::Code]), |_| super::property_engine::PropertyValue::String(String::new())),
    schema!("color",         ValueType::Color,       F::ASSIGNABLE_AI,             ActorField::Color,               None,                             Applicable::AllDrawables, |_| super::property_engine::PropertyValue::Color([1.0, 1.0, 1.0, 1.0])),
    schema!("cols",          ValueType::U32,         F::empty(),                   ActorField::ContainerLayoutGroup, Some(GroupMembership { group_id: GroupHandlerId::ContainerLayout }), Applicable::ActorKinds(&[A::Grid]), |_| super::property_engine::PropertyValue::U32(2)),
    schema!("commands",      ValueType::CommandList, F::ASSIGNABLE_A,              ActorField::Commands,            Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Path]), |_| super::property_engine::PropertyValue::CommandList(String::new())),
    schema!("fill_opacity",  ValueType::F32,         F::ASSIGNABLE_AI,             ActorField::FillOpacity,         None,                             Applicable::AllShapesExceptLine, |_| super::property_engine::PropertyValue::F32(1.0)),
    schema!("font_family",   ValueType::String,      F::ASSIGNABLE,                ActorField::FontFamily,          None,                             Applicable::ActorKinds(&[A::Text, A::Math, A::Code]), |_| super::property_engine::PropertyValue::String(crate::renderer::text::DEFAULT_FONT_FAMILY.to_string())),
    schema!("font_size",     ValueType::F32,         F::ASSIGNABLE_A,              ActorField::FontSize,            None,                             Applicable::ActorKinds(&[A::Text, A::Math, A::Code]), |kind| match kind { A::Text => super::property_engine::PropertyValue::F32(48.0), A::Math => super::property_engine::PropertyValue::F32(36.0), A::Code => super::property_engine::PropertyValue::F32(24.0), _ => super::property_engine::PropertyValue::F32(24.0) }),
    schema!("from",          ValueType::Vec2,        F::ASSIGNABLE_AI,             ActorField::LineFrom,            Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Line]), |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])),
    schema!("func",          ValueType::BuildTimeOnly, F::empty(),                 ActorField::PlotDomainGroup,     Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), Applicable::ActorKinds(&[A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]), |_| super::property_engine::PropertyValue::String(String::new())),
    schema!("gap",           ValueType::F32,         F::empty(),                   ActorField::ContainerLayoutGroup, Some(GroupMembership { group_id: GroupHandlerId::ContainerLayout }), Applicable::ActorKinds(&[A::Row, A::Col, A::Grid]), |_| super::property_engine::PropertyValue::F32(0.0)),
    schema!("latex",         ValueType::String,      F::ANIMATED,                  ActorField::TextContent,         None,                             Applicable::Never, |_| super::property_engine::PropertyValue::String(String::new())),
    schema!("math",          ValueType::String,      F::ANIMATED,                  ActorField::TextContent,         None,                             Applicable::ActorKinds(&[A::Math]), |_| super::property_engine::PropertyValue::String(String::new())),
    schema!("max_depth",     ValueType::F32,         F::empty(),                   ActorField::PlotDomainGroup,     Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), Applicable::ActorKinds(&[A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]), |_| super::property_engine::PropertyValue::F32(100.0)),
    schema!("offset",        ValueType::Vec2,        F::ASSIGNABLE_AI,             ActorField::PositionBindingGroup, Some(GroupMembership { group_id: GroupHandlerId::PositionBinding }), Applicable::Everything, |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])),
    schema!("opacity",       ValueType::F32,         F::ASSIGNABLE_AI,             ActorField::Opacity,             None,                             Applicable::EveryActorExceptGroup, |_| super::property_engine::PropertyValue::F32(1.0)),
    schema!("padding",       ValueType::F32,         F::empty(),                   ActorField::ContainerLayoutGroup, Some(GroupMembership { group_id: GroupHandlerId::ContainerLayout }), Applicable::ActorKinds(&[A::Row, A::Col, A::Grid, A::Stack]), |_| super::property_engine::PropertyValue::F32(0.0)),
    schema!("points",        ValueType::PointList,   F::ASSIGNABLE_A,              ActorField::Points,              Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Polygon]), |_| super::property_engine::PropertyValue::PointList(Vec::new())),
    schema!("position",      ValueType::Vec2,        F::ASSIGNABLE_AI,             ActorField::Position,            None,                             Applicable::Everything, |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])),
    schema!("radius",        ValueType::F32,         F::ASSIGNABLE_AI,             ActorField::Size,                Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Ellipse, S::Polygon]), |_| super::property_engine::PropertyValue::F32(50.0)),
    schema!("radius_x",      ValueType::F32,         F::ASSIGNABLE_AI,             ActorField::Size,                Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Ellipse]), |_| super::property_engine::PropertyValue::F32(50.0)),
    schema!("radius_y",      ValueType::F32,         F::ASSIGNABLE_AI,             ActorField::Size,                Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Ellipse]), |_| super::property_engine::PropertyValue::F32(50.0)),
    schema!("resolution",    ValueType::F32,         F::empty(),                   ActorField::PlotDomainGroup,     Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), Applicable::ActorKinds(&[A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]), |_| super::property_engine::PropertyValue::F32(100.0)),
    schema!("rotation",      ValueType::F32,         F::ASSIGNABLE_AI,             ActorField::Rotation,            None,                             Applicable::Everything, |_| super::property_engine::PropertyValue::F32(0.0)),
    schema!("scale",         ValueType::F32,         F::ASSIGNABLE_AI,             ActorField::Scale,               None,                             Applicable::Everything, |_| super::property_engine::PropertyValue::F32(1.0)),
    schema!("sides",         ValueType::U32,         F::empty(),                   ActorField::VectorShapeGroup,    Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Polygon]), |_| super::property_engine::PropertyValue::U32(3)),
    schema!("size",          ValueType::Vec2,        F::ALL,                       ActorField::Size,                None,                             Applicable::SizedActors, |_| super::property_engine::PropertyValue::Vec2([50.0, 50.0])),
    schema!("start_angle",   ValueType::F32,         F::ASSIGNABLE_AI,             ActorField::ArcAngles,           Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Ellipse]), |_| super::property_engine::PropertyValue::F32(0.0)),
    schema!("stroke",        ValueType::Color,       F::ASSIGNABLE_AI,             ActorField::StrokeColor,         None,                             Applicable::AllShapes, |_| super::property_engine::PropertyValue::Color([1.0, 1.0, 1.0, 1.0])),
    schema!("stroke_progress",ValueType::F32,        F::ASSIGNABLE_AI,             ActorField::StrokeProgress,      None,                             Applicable::AllShapes, |_| super::property_engine::PropertyValue::F32(1.0)),
    schema!("stroke_width",  ValueType::F32,         F::ASSIGNABLE_AI,             ActorField::StrokeWidth,         None,                             Applicable::AllShapes, |_| super::property_engine::PropertyValue::F32(1.0)),
    schema!("sweep_angle",   ValueType::F32,         F::ASSIGNABLE_AI,             ActorField::ArcAngles,           Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Ellipse]), |_| super::property_engine::PropertyValue::F32(std::f32::consts::PI)),
    schema!("t_domain",      ValueType::Vec2,        F::empty(),                   ActorField::PlotDomainGroup,     Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), Applicable::ActorKinds(&[A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]), |_| super::property_engine::PropertyValue::Vec2([0.0, 1.0])),
    schema!("text",          ValueType::String,      F::ASSIGNABLE_A,              ActorField::TextContent,         None,                             Applicable::ActorKinds(&[A::Text]), |_| super::property_engine::PropertyValue::String(String::new())),
    schema!("tip_length",    ValueType::F32,         F::empty(),                   ActorField::VectorShapeGroup,    Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Line]), |_| super::property_engine::PropertyValue::F32(10.0)),
    schema!("tip_width",     ValueType::F32,         F::empty(),                   ActorField::VectorShapeGroup,    Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Line]), |_| super::property_engine::PropertyValue::F32(10.0)),
    schema!("to",            ValueType::Vec2,        F::ASSIGNABLE_AI,             ActorField::LineTo,              Some(GroupMembership { group_id: GroupHandlerId::VectorShapeState }), Applicable::ShapeKinds(&[S::Line]), |_| super::property_engine::PropertyValue::Vec2([100.0, 0.0])),
    schema!("tolerance",     ValueType::F32,         F::empty(),                   ActorField::PlotDomainGroup,     Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), Applicable::ActorKinds(&[A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]), |_| super::property_engine::PropertyValue::F32(0.1)),
    schema!("url",           ValueType::String,      F::ASSIGNABLE,                ActorField::ImageData,           None,                             Applicable::ActorKinds(&[A::Image, A::Svg]), |_| super::property_engine::PropertyValue::String(String::new())),
    schema!("x_domain",      ValueType::Vec2,        F::empty(),                   ActorField::PlotDomainGroup,     Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), Applicable::ActorKinds(&[A::Graph, A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]), |_| super::property_engine::PropertyValue::Vec2([-5.0, 5.0])),
    schema!("y_domain",      ValueType::Vec2,        F::empty(),                   ActorField::PlotDomainGroup,     Some(GroupMembership { group_id: GroupHandlerId::PlotDomain }), Applicable::ActorKinds(&[A::Graph, A::CartesianPlot, A::PolarPlot, A::ParametricPlot, A::ImplicitPlot]), |_| super::property_engine::PropertyValue::Vec2([-5.0, 5.0])),
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
