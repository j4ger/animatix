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
    /// 32-bit floating-point value.
    F32,
    /// 32-bit unsigned integer value.
    U32,
    /// 2D vector value.
    Vec2,
    /// 4D vector value.
    Vec4,
    /// Color value (RGBA).
    Color,
    /// String value.
    String,
    /// Boolean value.
    Bool,
    /// Shape kind identifier.
    ShapeType,
    /// Placement mode for layout positioning.
    PlacementMode,
    /// Scene anchor point.
    SceneAnchor,
    /// Position binding configuration.
    PositionBinding,
    /// Morphing animation options.
    MorphOptions,
    /// Callout placement side.
    CalloutPlace,
    /// List of 2D points.
    PointList,
    /// List of drawing commands.
    CommandList,
    /// 2D affine transform.
    Transform,
    /// A property that produces builder-time side effects (no animated value).
    BuildTimeOnly,
    /// A tagged union of basic value types. Variants are tried in order.
    Union(&'static [ValueType]),
    /// A named sum type with optional payloads, e.g. `Bool | Str` choices.
    Sum(&'static [SumVariant]),
    /// A fixed set of allowed string choices.
    Enum(&'static [&'static str]),
}

/// Exact literal used to select a named sum variant before payload parsing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SumLiteral {
    /// Boolean literal discriminator.
    Bool(bool),
    /// String literal discriminator.
    Str(&'static str),
}

/// One named variant in a [`ValueType::Sum`] schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SumVariant {
    /// Canonical variant name.
    pub name: &'static str,
    /// Payload type carried by this variant.
    pub value_type: ValueType,
    /// Optional exact literal that selects this variant before generic parsing.
    pub literal: Option<SumLiteral>,
}

/// Named variants for the generic `legend` property.
pub static LEGEND_SUM_VARIANTS: &[SumVariant] = &[
    SumVariant {
        name: "auto",
        value_type: ValueType::Bool,
        literal: Some(SumLiteral::Bool(true)),
    },
    SumVariant {
        name: "hidden",
        value_type: ValueType::Bool,
        literal: Some(SumLiteral::Bool(false)),
    },
    SumVariant {
        name: "label",
        value_type: ValueType::String,
        literal: None,
    },
];

// ─────────────────────────────────────────────────────────────
// Property flags
// ─────────────────────────────────────────────────────────────

/// Feature flags that change how the engine processes a property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PropertyFlags(u8);

impl PropertyFlags {
    /// This property supports keyframe animation.
    pub const ANIMATED: Self = Self(0b0001);
    /// This property can be assigned from source expressions.
    pub const ASSIGNABLE: Self = Self(0b0010);
    /// This property can receive injected environment values.
    pub const INJECTABLE: Self = Self(0b0100);
    /// Changes to this property affect layout resolution.
    pub const LAYOUT_AFFECTING: Self = Self(0b1000);

    // Convenience combinations for use in static PROPERTY_REGISTRY
    /// `ANIMATED | ASSIGNABLE` combined.
    pub const ASSIGNABLE_A: Self = Self(0b0011); // ANIMATED | ASSIGNABLE
    /// `ANIMATED | ASSIGNABLE | INJECTABLE` combined.
    pub const ASSIGNABLE_AI: Self = Self(0b0111); // ANIMATED | ASSIGNABLE | INJECTABLE
    /// `ANIMATED | INJECTABLE` combined.
    pub const ANIMATED_I: Self = Self(0b0101); // ANIMATED | INJECTABLE
    /// All flags combined.
    pub const ALL: Self = Self(0b1111); // all flags

    /// Returns an empty flag set.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns `true` if all bits in `other` are set in `self`.
    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Combine two flag sets (const, usable in statics).
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

// ─────────────────────────────────────────────────────────────
// Frame-time read source
// ─────────────────────────────────────────────────────────────

/// Declares how a property's value is READ at frame time (environment
/// injection, `_animating_*` flags), separate from how it is WRITTEN at
/// build time (parsing, keyframing via `field`).
///
/// Most properties use `Field(field)` where `field` matches the schema's
/// write target. Aliases and virtual/derived sub-properties use other variants.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReadSource {
    /// Read directly from the given track field. Default for most properties.
    Field(ActorField),
    /// Extract a single component from a Vec2 field, optionally scaled.
    /// Used for virtual sub-properties like `width` = `size.x * 2`.
    Component {
        /// The Vec2 storage field (e.g. Size).
        field: ActorField,
        /// Component index: 0 for x, 1 for y.
        index: usize,
        /// Multiplier applied after extraction (e.g. 2.0 for width = size.x * 2).
        scale: f64,
    },
    /// Read from a different field than the write target.
    /// Used when the write target is a group handler (e.g. `at` →
    /// PositionBindingGroup) but the frame-time value lives in a
    /// concrete storage field (Position).
    Alias(ActorField),
    /// Not readable from the track at frame time.
    None_,
}

impl ReadSource {
    /// Read the current frame-time value from the track. Returns `None` for
    /// `None_` (not readable) or when the track field has no value.
    pub fn read(
        &self,
        track: &crate::timeline::AnimationTrack,
        time_ms: u64,
    ) -> Option<crate::timeline::PropertyValue> {
        match self {
            ReadSource::Field(f) | ReadSource::Alias(f) => {
                crate::timeline::dispatch::read_property_value(track, *f, time_ms)
            },
            ReadSource::Component {
                field,
                index,
                scale,
            } => crate::timeline::dispatch::read_property_value(track, *field, time_ms).map(|pv| {
                if let crate::timeline::PropertyValue::Vec2(v) = pv {
                    crate::timeline::PropertyValue::F32(v[*index] * *scale as f32)
                } else {
                    pv
                }
            }),
            ReadSource::None_ => None,
        }
    }

    /// The underlying storage field, used for `_animating_*` flag checks.
    /// Returns `None` for properties with no readable storage (`None_`).
    pub fn storage_field(&self) -> Option<ActorField> {
        match self {
            ReadSource::Field(f) | ReadSource::Alias(f) => Some(*f),
            ReadSource::Component { field, .. } => Some(*field),
            ReadSource::None_ => None,
        }
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
    /// Absolute position in scene coordinates.
    Position,
    /// Offset applied during motion animations.
    MotionOffset,
    /// Explicitly set size (may differ from layout size).
    Size,
    /// Size computed by the layout engine.
    LayoutSize,
    /// Rotation angle in radians.
    Rotation,
    /// Uniform scale factor.
    Scale,
    /// How the actor is placed relative to its container.
    PlacementMode,
    /// Binding that ties position to another actor or anchor.
    PositionBinding,

    // ── Style tier ──
    /// Fill color.
    Color,
    /// Overall opacity multiplier.
    Opacity,
    /// Width of the stroke outline.
    StrokeWidth,
    /// Color of the stroke outline.
    StrokeColor,
    /// How much of the stroke is drawn (0–1).
    StrokeProgress,
    /// Opacity of the fill interior.
    FillOpacity,
    /// Options for shape morphing.
    MorphOptions,

    // ── Shape payload ──
    /// Kind of shape (line, circle, rectangle, etc.).
    ShapeType,
    /// Start point of a line shape.
    LineFrom,
    /// End point of a line shape.
    LineTo,
    /// Start and sweep angles for arc shapes.
    ArcAngles,
    /// Vertices for polygon shapes.
    Points,
    /// Drawing commands for path shapes.
    Commands,
    /// Cached vector paths after tessellation.
    VectorPaths,
    /// Arrowhead size for arrow shapes.
    HeadSize,
    /// Line cap style (butt = 0, round = 1, square = 2).
    LineCap,
    /// Line join style (miter = 0, round = 1, bevel = 2).
    LineJoin,

    // ── Text payload ──
    /// Raw text content.
    TextContent,
    /// Cached text glyph paths.
    TextPaths,
    /// Font family name.
    FontFamily,
    /// Font size in points.
    FontSize,
    /// Font weight (100–900).
    FontWeight,
    /// Font style ("normal" | "italic").
    FontStyle,
    /// Line height multiplier.
    LineHeight,
    /// Letter spacing in points.
    LetterSpacing,
    /// Word spacing in points.
    WordSpacing,
    /// Max width for text wrapping (0 = no wrap).
    TextMaxWidth,
    /// Text alignment ("left", "center", "right", "justify").
    TextAlign,
    /// Overflow behavior ("visible", "clip", "ellipsis").
    Overflow,

    /// Character reveal progress (0-1) for typewriter effect.
    CharProgress,

    // ── Media payload ──
    /// Loaded image or video data.
    ImageData,
    /// Cached SVG tessellation paths.
    SvgPaths,
    /// Audio file path or URL.
    AudioSource,
    /// Audio volume multiplier.
    AudioVolume,

    // ── Callout / annotation ──
    /// Label position offset for callouts.
    LabelAt,
    /// Target actor path for targeted callout mode.
    CalloutTarget,
    /// Placement hint string for targeted callout.
    CalloutPlace,
    /// Standoff distance from tip to target.
    CalloutStandoff,
    /// Offset on the target side of the callout anchor.
    CalloutToOffset,

    // ── Font metrics (baseline alignment) ──
    /// Font ascent in scene units.
    Ascent,
    /// Font descent in scene units.
    Descent,
    /// Baseline offset from text center.
    Baseline,

    // ── Highlight properties ──
    /// Highlight background color for equation fragments.
    HighlightColor,
    /// Highlight opacity for equation fragments.
    HighlightOpacity,
    /// Highlight padding for equation fragments.
    HighlightPadding,
    /// Highlight corner radius for equation fragments.
    HighlightRadius,

    // ── Effects tier ──
    // ── Filter tier ──
    /// Gaussian blur radius.
    FilterBlur,
    /// Brightness multiplier.
    FilterBrightness,
    /// Contrast multiplier.
    FilterContrast,
    /// Saturation multiplier.
    FilterSaturate,
    /// Hue rotation in degrees.
    FilterHueRotate,
    /// Sepia intensity.
    FilterSepia,

    // ── Transform tier ──
    /// 2D affine transform matrix.
    Transform,

    // ── Min/Max size constraints (Phase 7) ──
    /// Minimum width constraint.
    MinWidth,
    /// Minimum height constraint.
    MinHeight,
    /// Maximum height constraint.
    MaxHeight,

    // ── Compound resolution groups (handled by GroupHandler) ──
    /// Compound group for position binding resolution.
    PositionBindingGroup,
    /// Compound group for vector shape state resolution.
    VectorShapeGroup,
    /// Compound group for plot domain resolution.
    PlotDomainGroup,
    /// Compound group for container layout resolution.
    ContainerLayoutGroup,
    /// Generic tagged union storage, keyed by canonical property name.
    Tagged(&'static str),
    /// No storage field (build-time only, props-backed).
    NoStorage,
}

impl ActorField {
    /// Returns the default `PropertyValue` for this field.
    ///
    /// Returns `None` for fields that don't support direct keyframing
    /// (group fields, compound types like PlacementMode/PositionBinding,
    /// and generated payloads like VectorPaths/TextPaths).
    pub fn default_value(self) -> Option<super::property_engine::PropertyValue> {
        use super::animation_track::{DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE};
        use super::property_engine::PropertyValue;
        Some(match self {
            // ── Geometry tier ──
            ActorField::Position => PropertyValue::Vec2([0.0, 0.0]),
            ActorField::MotionOffset => PropertyValue::Vec2([0.0, 0.0]),
            ActorField::Size => PropertyValue::Vec2(DEFAULT_LAYOUT_HALF_SIZE),
            ActorField::LayoutSize => PropertyValue::Vec2(DEFAULT_LAYOUT_HALF_SIZE),
            ActorField::Rotation => PropertyValue::F32(0.0),
            ActorField::Scale => PropertyValue::F32(1.0),
            ActorField::PlacementMode => return None,
            ActorField::PositionBinding => return None,

            // ── Style tier ──
            ActorField::Color => PropertyValue::Vec4(DEFAULT_WHITE),
            ActorField::Opacity => PropertyValue::F32(1.0),
            ActorField::StrokeWidth => PropertyValue::F32(2.0),
            ActorField::StrokeColor => PropertyValue::Vec4(DEFAULT_WHITE),
            ActorField::StrokeProgress => PropertyValue::F32(1.0),
            ActorField::FillOpacity => PropertyValue::F32(1.0),
            ActorField::MorphOptions => return None,

            // ── Effects tier ──
            // ── Filter tier ──
            ActorField::FilterBlur => PropertyValue::F32(0.0),
            ActorField::FilterBrightness => PropertyValue::F32(1.0),
            ActorField::FilterContrast => PropertyValue::F32(1.0),
            ActorField::FilterSaturate => PropertyValue::F32(1.0),
            ActorField::FilterHueRotate => PropertyValue::F32(0.0),
            ActorField::FilterSepia => PropertyValue::F32(0.0),

            ActorField::Transform => PropertyValue::Transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),

            ActorField::ShapeType => PropertyValue::U32(0),
            ActorField::LineFrom => PropertyValue::Vec2([-50.0, 0.0]),
            ActorField::LineTo => PropertyValue::Vec2([50.0, 0.0]),
            ActorField::ArcAngles => PropertyValue::Vec2([0.0, std::f32::consts::PI]),
            ActorField::Points => PropertyValue::PointList(Vec::new()),
            ActorField::Commands => PropertyValue::CommandList(String::new()),
            ActorField::HeadSize => PropertyValue::F32(10.0),
            ActorField::LineCap => PropertyValue::U32(0),
            ActorField::LineJoin => PropertyValue::U32(0),
            ActorField::VectorPaths => return None,

            // ── Text payload ──
            ActorField::TextContent => PropertyValue::String(String::new()),
            ActorField::TextPaths => return None,
            ActorField::CharProgress => PropertyValue::F32(1.0),
            ActorField::FontFamily => PropertyValue::String(String::new()),
            ActorField::FontSize => PropertyValue::F32(48.0),
            ActorField::FontWeight => PropertyValue::F32(400.0),
            ActorField::FontStyle => PropertyValue::String("normal".to_string()),
            ActorField::LineHeight => PropertyValue::F32(1.2),
            ActorField::LetterSpacing => PropertyValue::F32(0.0),
            ActorField::TextMaxWidth => PropertyValue::F32(0.0),
            ActorField::TextAlign => PropertyValue::String("left".to_string()),
            ActorField::Overflow => PropertyValue::String("visible".to_string()),
            ActorField::WordSpacing => PropertyValue::F32(0.0),

            // ── Font metrics ──
            ActorField::Ascent => PropertyValue::F32(0.0),
            ActorField::Descent => PropertyValue::F32(0.0),
            ActorField::Baseline => PropertyValue::F32(0.0),

            // ── Callout ──
            ActorField::LabelAt => PropertyValue::Vec2([0.0, 0.0]),
            ActorField::CalloutTarget => PropertyValue::String(String::new()),
            ActorField::CalloutPlace => {
                PropertyValue::CalloutPlace(crate::timeline::animation_track::CalloutPlace::Right)
            },
            ActorField::CalloutStandoff => PropertyValue::F32(40.0),
            ActorField::CalloutToOffset => PropertyValue::Vec2([0.0, 0.0]),

            // ── Highlight ──
            ActorField::HighlightColor => PropertyValue::Vec4([0.3, 0.5, 1.0, 1.0]),
            ActorField::HighlightOpacity => PropertyValue::F32(0.0),
            ActorField::HighlightPadding => PropertyValue::F32(4.0),
            ActorField::HighlightRadius => PropertyValue::F32(3.0),

            // ── Media payload ──
            ActorField::ImageData => return None,
            ActorField::SvgPaths => return None,
            ActorField::AudioSource => return None,
            ActorField::AudioVolume => PropertyValue::F32(1.0),

            // ── Min/Max size constraints ──
            ActorField::MinWidth => PropertyValue::F32(0.0),
            ActorField::MinHeight => PropertyValue::F32(0.0),
            ActorField::MaxHeight => PropertyValue::F32(f32::INFINITY),

            // ── Group fields ──
            ActorField::PositionBindingGroup
            | ActorField::VectorShapeGroup
            | ActorField::PlotDomainGroup
            | ActorField::ContainerLayoutGroup
            | ActorField::Tagged(_)
            | ActorField::NoStorage => return None,
        })
    }
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
    /// The compound resolution group this property belongs to.
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
    /// All actors with stroke-based path rendering (shapes + PlotCurve).
    AllStrokePaths,
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
    /// Returns `true` if this applicability includes the given actor kind.
    pub fn includes(self, kind: super::ActorKindId) -> bool {
        use super::ActorKindId::*;
        use super::ShapeKind;
        match self {
            Applicable::Everything => true,
            Applicable::EveryActorExceptGroup => !matches!(kind, Group),
            Applicable::AllShapes => matches!(kind, Shape(_)),
            Applicable::AllStrokePaths => {
                matches!(kind, Shape(_) | PlotCurve)
            },
            Applicable::AllShapesExceptLine => {
                matches!(kind, Shape(sk) if sk != ShapeKind::Line)
            },
            Applicable::AllDrawables => {
                matches!(kind, Shape(_) | Text | Typst | Code | BarChart)
            },
            Applicable::SizedActors => {
                matches!(
                    kind,
                    Shape(_)
                        | Image
                        | Graph
                        | PlotCurve
                        | VectorField
                        | Heatmap
                        | ContourSet
                        | NumberPlane
                        | BarChart
                        | Row
                        | Col
                        | Grid
                        | Stack
                        | Filter
                )
            },
            Applicable::ShapeKinds(kinds) => {
                matches!(kind, Shape(sk) if kinds.contains(&sk))
            },
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
    /// (e.g. `font_size` is 48 for Text, 36 for Typst, 24 for Code).
    pub default_value: fn(super::ActorKindId) -> super::property_engine::PropertyValue,
    /// How this property is read at frame time (env injection, `_animating` flags).
    pub read_source: ReadSource,
}

// ─────────────────────────────────────────────────────────────
// The registry — sorted by name for binary search
// ─────────────────────────────────────────────────────────────

/// The complete, authoritative registry of every property in Animatix.
///
/// **Must be sorted by `.name`** for `lookup_property()` binary search.
/// A `#[test]` below verifies this invariant.
use PropertyFlags as F;

use super::{ActorKindId as A, ShapeKind as S};

macro_rules! schema {
    ($name:expr, $ty:expr, $flags:expr, $field:expr, $group:expr, $applicable:expr, $default:expr) => {
        schema!(
            $name,
            $ty,
            $flags,
            $field,
            $group,
            $applicable,
            $default,
            ReadSource::Field($field)
        )
    };
    ($name:expr, $ty:expr, $flags:expr, $field:expr, $group:expr, $applicable:expr, $default:expr, $read:expr) => {
        PropertySchema {
            name: $name,
            value_type: $ty,
            flags: $flags,
            field: $field,
            group: $group,
            applicable: $applicable,
            default_value: $default,
            read_source: $read,
        }
    };
}

/// Registry of all built-in actor properties with their schemas.
pub static PROPERTY_REGISTRY: &[PropertySchema] = &[
    schema!(
        "align",
        ValueType::String,
        F::empty(),
        ActorField::ContainerLayoutGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::ContainerLayout
        }),
        Applicable::ActorKinds(&[A::Row, A::Col, A::Grid, A::Stack]),
        |_| super::property_engine::PropertyValue::String("center".to_string())
    ),
    schema!(
        "anchor",
        ValueType::SceneAnchor,
        F::ASSIGNABLE_AI,
        ActorField::PositionBindingGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PositionBinding
        }),
        Applicable::Everything,
        |_| super::property_engine::PropertyValue::String("center".to_string()),
        ReadSource::None_
    ),
    schema!(
        "ascent",
        ValueType::F32,
        F::ANIMATED,
        ActorField::Ascent,
        None,
        Applicable::Never,
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "at",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::PositionBindingGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PositionBinding
        }),
        Applicable::Everything,
        |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0]),
        ReadSource::Alias(ActorField::Position)
    ),
    schema!(
        "background_color",
        ValueType::Color,
        F::ASSIGNABLE_AI,
        ActorField::Color,
        None,
        Applicable::Never,
        |_| super::property_engine::PropertyValue::Color([0.0, 0.0, 0.0, 1.0]),
        ReadSource::None_
    ),
    schema!(
        "bar_colors",
        ValueType::String,
        F::empty(),
        ActorField::NoStorage,
        None,
        Applicable::ActorKinds(&[A::BarChart]),
        |_| super::property_engine::PropertyValue::String("auto".to_string())
    ),
    schema!(
        "bar_width",
        ValueType::F32,
        F::empty(),
        ActorField::NoStorage,
        None,
        Applicable::ActorKinds(&[A::BarChart]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "baseline",
        ValueType::F32,
        F::ANIMATED,
        ActorField::Baseline,
        None,
        Applicable::Never,
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "blur",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::FilterBlur,
        None,
        Applicable::ActorKinds(&[A::Filter]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "brightness",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::FilterBrightness,
        None,
        Applicable::ActorKinds(&[A::Filter]),
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    schema!(
        "char_progress",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::CharProgress,
        None,
        Applicable::ActorKinds(&[A::Text, A::Code, A::Typst]),
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    schema!(
        "code",
        ValueType::String,
        F::ANIMATED,
        ActorField::TextContent,
        None,
        Applicable::ActorKinds(&[A::Code]),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "color",
        ValueType::Color,
        F::ASSIGNABLE_AI,
        ActorField::Color,
        None,
        Applicable::AllDrawables,
        |_| super::property_engine::PropertyValue::Color([1.0, 1.0, 1.0, 1.0])
    ),
    schema!(
        "cols",
        ValueType::U32,
        F::empty(),
        ActorField::ContainerLayoutGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::ContainerLayout
        }),
        Applicable::ActorKinds(&[A::Grid]),
        |_| super::property_engine::PropertyValue::U32(2)
    ),
    schema!(
        "commands",
        ValueType::CommandList,
        F::ASSIGNABLE_A,
        ActorField::Commands,
        Some(GroupMembership {
            group_id: GroupHandlerId::VectorShapeState
        }),
        Applicable::ShapeKinds(&[S::Path]),
        |_| super::property_engine::PropertyValue::CommandList(String::new())
    ),
    schema!(
        "contrast",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::FilterContrast,
        None,
        Applicable::ActorKinds(&[A::Filter]),
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    schema!(
        "data",
        ValueType::String,
        F::empty(),
        ActorField::NoStorage,
        None,
        Applicable::ActorKinds(&[A::BarChart]),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "density",
        ValueType::F32,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::VectorField]),
        |_| super::property_engine::PropertyValue::F32(16.0)
    ),
    schema!(
        "descent",
        ValueType::F32,
        F::ANIMATED,
        ActorField::Descent,
        None,
        Applicable::Never,
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "direction",
        ValueType::String,
        F::empty(),
        ActorField::NoStorage,
        None,
        Applicable::ActorKinds(&[A::BarChart]),
        |_| super::property_engine::PropertyValue::String("vertical".to_string())
    ),
    schema!(
        "fill_opacity",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::FillOpacity,
        None,
        Applicable::AllShapesExceptLine,
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    schema!(
        "font_family",
        ValueType::String,
        F::ASSIGNABLE,
        ActorField::FontFamily,
        None,
        Applicable::ActorKinds(&[A::Text, A::Typst, A::Code]),
        |_| super::property_engine::PropertyValue::String(
            crate::renderer::text::DEFAULT_FONT_FAMILY.to_string()
        )
    ),
    schema!(
        "font_size",
        ValueType::F32,
        F::ASSIGNABLE_A,
        ActorField::FontSize,
        None,
        Applicable::ActorKinds(&[A::Text, A::Typst, A::Code]),
        |kind| match kind {
            A::Text => super::property_engine::PropertyValue::F32(48.0),
            A::Typst => super::property_engine::PropertyValue::F32(36.0),
            A::Code => super::property_engine::PropertyValue::F32(24.0),
            _ => super::property_engine::PropertyValue::F32(24.0),
        }
    ),
    schema!(
        "font_style",
        ValueType::String,
        F::ASSIGNABLE,
        ActorField::FontStyle,
        None,
        Applicable::ActorKinds(&[A::Text, A::Typst, A::Code]),
        |_| super::property_engine::PropertyValue::String("normal".to_string())
    ),
    schema!(
        "font_weight",
        ValueType::F32,
        F::ASSIGNABLE,
        ActorField::FontWeight,
        None,
        Applicable::ActorKinds(&[A::Text, A::Typst, A::Code]),
        |_| super::property_engine::PropertyValue::F32(400.0)
    ),
    schema!(
        "from",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::LineFrom,
        Some(GroupMembership {
            group_id: GroupHandlerId::VectorShapeState
        }),
        Applicable::ActorKinds(&[A::Shape(S::Line), A::Shape(S::Arrow), A::Callout]),
        |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])
    ),
    schema!(
        "func",
        ValueType::BuildTimeOnly,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::PlotCurve, A::VectorField, A::Heatmap, A::ContourSet]),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "gap",
        ValueType::F32,
        F::empty(),
        ActorField::ContainerLayoutGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::ContainerLayout
        }),
        Applicable::ActorKinds(&[A::Row, A::Col, A::Grid]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "grid",
        ValueType::String,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::Graph]),
        |_| super::property_engine::PropertyValue::String("auto".to_string())
    ),
    schema!(
        "head_size",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::HeadSize,
        Some(GroupMembership {
            group_id: GroupHandlerId::VectorShapeState
        }),
        Applicable::ActorKinds(&[A::Shape(S::Arrow), A::Callout]),
        |_| super::property_engine::PropertyValue::F32(10.0)
    ),
    schema!(
        "height",
        ValueType::F32,
        F::ANIMATED_I,
        ActorField::Size,
        None,
        Applicable::SizedActors,
        |_| super::property_engine::PropertyValue::F32(100.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 1,
            scale: 2.0
        }
    ),
    schema!(
        "highlight_color",
        ValueType::Color,
        F::ANIMATED,
        ActorField::HighlightColor,
        None,
        Applicable::ActorKinds(&[A::Equation, A::Fragment]),
        |_| super::property_engine::PropertyValue::Vec4([0.3, 0.5, 1.0, 1.0])
    ),
    schema!(
        "highlight_opacity",
        ValueType::F32,
        F::ANIMATED,
        ActorField::HighlightOpacity,
        None,
        Applicable::ActorKinds(&[A::Equation, A::Fragment]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "highlight_padding",
        ValueType::F32,
        F::ANIMATED,
        ActorField::HighlightPadding,
        None,
        Applicable::ActorKinds(&[A::Equation, A::Fragment]),
        |_| super::property_engine::PropertyValue::F32(4.0)
    ),
    schema!(
        "highlight_radius",
        ValueType::F32,
        F::ANIMATED,
        ActorField::HighlightRadius,
        None,
        Applicable::ActorKinds(&[A::Equation, A::Fragment]),
        |_| super::property_engine::PropertyValue::F32(3.0)
    ),
    schema!(
        "hue_rotate",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::FilterHueRotate,
        None,
        Applicable::ActorKinds(&[A::Filter]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "kind",
        ValueType::String,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::PlotCurve]),
        |_| super::property_engine::PropertyValue::String("cartesian".to_string())
    ),
    schema!(
        "label",
        ValueType::String,
        F::ASSIGNABLE_A,
        ActorField::TextContent,
        None,
        Applicable::ActorKinds(&[A::Callout]),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "label_at",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::LabelAt,
        None,
        Applicable::ActorKinds(&[A::Callout]),
        |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])
    ),
    schema!(
        "label_color",
        ValueType::Color,
        F::ASSIGNABLE_A,
        ActorField::Tagged("legend_label_color"),
        None,
        Applicable::ActorKinds(&[A::Legend]),
        |_| super::property_engine::PropertyValue::Color([1.0, 1.0, 1.0, 1.0])
    ),
    schema!(
        "latex",
        ValueType::String,
        F::ANIMATED,
        ActorField::TextContent,
        None,
        Applicable::Never,
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "legend",
        ValueType::Sum(LEGEND_SUM_VARIANTS),
        F::ASSIGNABLE_A,
        ActorField::Tagged("legend"),
        None,
        Applicable::Everything,
        |_| super::property_engine::PropertyValue::Bool(true)
    ),
    schema!(
        "letter_spacing",
        ValueType::F32,
        F::ASSIGNABLE,
        ActorField::LetterSpacing,
        None,
        Applicable::ActorKinds(&[A::Text, A::Typst, A::Code]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "levels",
        ValueType::Vec2,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::ContourSet]),
        |_| super::property_engine::PropertyValue::Vec2([0.0, 1.0])
    ),
    schema!(
        "line_cap",
        ValueType::U32,
        F::ASSIGNABLE_AI,
        ActorField::LineCap,
        None,
        Applicable::AllShapes,
        |_| super::property_engine::PropertyValue::U32(0)
    ),
    schema!(
        "line_height",
        ValueType::F32,
        F::ASSIGNABLE,
        ActorField::LineHeight,
        None,
        Applicable::ActorKinds(&[A::Text, A::Typst, A::Code]),
        |_| super::property_engine::PropertyValue::F32(1.2)
    ),
    schema!(
        "line_join",
        ValueType::U32,
        F::ASSIGNABLE_AI,
        ActorField::LineJoin,
        None,
        Applicable::AllShapes,
        |_| super::property_engine::PropertyValue::U32(0)
    ),
    schema!(
        "math",
        ValueType::String,
        F::ANIMATED,
        ActorField::TextContent,
        None,
        Applicable::ActorKinds(&[A::Typst]),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "max_depth",
        ValueType::F32,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::PlotCurve, A::ContourSet]),
        |_| super::property_engine::PropertyValue::F32(12.0)
    ),
    schema!(
        "max_height",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::MaxHeight,
        None,
        Applicable::SizedActors,
        |_| super::property_engine::PropertyValue::F32(f32::INFINITY)
    ),
    schema!(
        "max_value",
        ValueType::F32,
        F::empty(),
        ActorField::NoStorage,
        None,
        Applicable::ActorKinds(&[A::BarChart]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "max_width",
        ValueType::F32,
        F::ASSIGNABLE,
        ActorField::TextMaxWidth,
        None,
        Applicable::ActorKinds(&[A::Text, A::Typst, A::Code]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "min_height",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::MinHeight,
        None,
        Applicable::SizedActors,
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "min_width",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::MinWidth,
        None,
        Applicable::SizedActors,
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "offset",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::PositionBindingGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PositionBinding
        }),
        Applicable::Everything,
        |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0]),
        ReadSource::None_
    ),
    schema!(
        "opacity",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::Opacity,
        None,
        Applicable::Everything,
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    schema!(
        "overflow",
        ValueType::String,
        F::ASSIGNABLE,
        ActorField::Overflow,
        None,
        Applicable::ActorKinds(&[A::Text, A::Typst, A::Code]),
        |_| super::property_engine::PropertyValue::String("visible".to_string())
    ),
    schema!(
        "padding",
        ValueType::F32,
        F::empty(),
        ActorField::ContainerLayoutGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::ContainerLayout
        }),
        Applicable::ActorKinds(&[A::Graph, A::Row, A::Col, A::Grid, A::Stack]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "place",
        ValueType::Enum(&["auto", "top", "bottom", "left", "right", "above", "below"]),
        F::ASSIGNABLE,
        ActorField::Tagged("callout_place"),
        None,
        Applicable::ActorKinds(&[A::Callout]),
        |_| super::property_engine::PropertyValue::Enum("right".to_string())
    ),
    schema!(
        "points",
        ValueType::PointList,
        F::ASSIGNABLE_A,
        ActorField::Points,
        Some(GroupMembership {
            group_id: GroupHandlerId::VectorShapeState
        }),
        Applicable::ShapeKinds(&[S::Polygon]),
        |_| super::property_engine::PropertyValue::PointList(Vec::new())
    ),
    schema!(
        "position",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::Position,
        None,
        Applicable::Everything,
        |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])
    ),
    schema!(
        "radius_x",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::Size,
        Some(GroupMembership {
            group_id: GroupHandlerId::VectorShapeState
        }),
        Applicable::ShapeKinds(&[S::Ellipse]),
        |_| super::property_engine::PropertyValue::F32(50.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 0,
            scale: 1.0
        }
    ),
    schema!(
        "radius_y",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::Size,
        Some(GroupMembership {
            group_id: GroupHandlerId::VectorShapeState
        }),
        Applicable::ShapeKinds(&[S::Ellipse]),
        |_| super::property_engine::PropertyValue::F32(50.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 1,
            scale: 1.0
        }
    ),
    schema!(
        "resolution",
        ValueType::F32,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::PlotCurve, A::Heatmap, A::ContourSet]),
        |_| super::property_engine::PropertyValue::F32(48.0)
    ),
    schema!(
        "rotation",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::Rotation,
        None,
        Applicable::Everything,
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "saturate",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::FilterSaturate,
        None,
        Applicable::ActorKinds(&[A::Filter]),
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    schema!(
        "scale",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::Scale,
        None,
        Applicable::Everything,
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    schema!(
        "sepia",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::FilterSepia,
        None,
        Applicable::ActorKinds(&[A::Filter]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "shift",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::MotionOffset,
        None,
        Applicable::Everything,
        |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])
    ),
    schema!(
        "show_axis",
        ValueType::String,
        F::empty(),
        ActorField::NoStorage,
        None,
        Applicable::ActorKinds(&[A::BarChart]),
        |_| super::property_engine::PropertyValue::String("true".to_string())
    ),
    schema!(
        "show_labels",
        ValueType::String,
        F::empty(),
        ActorField::NoStorage,
        None,
        Applicable::ActorKinds(&[A::BarChart]),
        |_| super::property_engine::PropertyValue::String("true".to_string())
    ),
    schema!(
        "size",
        ValueType::Vec2,
        F::ALL,
        ActorField::Size,
        None,
        Applicable::SizedActors,
        |_| super::property_engine::PropertyValue::Vec2([50.0, 50.0])
    ),
    schema!(
        "source",
        ValueType::String,
        F::ASSIGNABLE,
        ActorField::AudioSource,
        None,
        Applicable::ActorKinds(&[A::Audio]),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "standoff",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::CalloutStandoff,
        None,
        Applicable::ActorKinds(&[A::Callout]),
        |_| super::property_engine::PropertyValue::F32(40.0)
    ),
    schema!(
        "stroke",
        ValueType::Color,
        F::ASSIGNABLE_AI,
        ActorField::StrokeColor,
        None,
        Applicable::AllStrokePaths,
        |_| super::property_engine::PropertyValue::Color([1.0, 1.0, 1.0, 1.0])
    ),
    schema!(
        "stroke_progress",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::StrokeProgress,
        None,
        Applicable::AllStrokePaths,
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    schema!(
        "stroke_width",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::StrokeWidth,
        None,
        Applicable::AllStrokePaths,
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    schema!(
        "swatch_size",
        ValueType::F32,
        F::ASSIGNABLE_A,
        ActorField::Tagged("legend_swatch_size"),
        None,
        Applicable::ActorKinds(&[A::Legend]),
        |_| super::property_engine::PropertyValue::F32(16.0)
    ),
    schema!(
        "t_domain",
        ValueType::Vec2,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::PlotCurve]),
        |_| super::property_engine::PropertyValue::Vec2([0.0, 1.0])
    ),
    schema!(
        "target",
        ValueType::String,
        F::ASSIGNABLE,
        ActorField::CalloutTarget,
        None,
        Applicable::ActorKinds(&[A::Callout]),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "text",
        ValueType::String,
        F::ASSIGNABLE_A,
        ActorField::TextContent,
        None,
        Applicable::ActorKinds(&[A::Text]),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "text_align",
        ValueType::String,
        F::ASSIGNABLE,
        ActorField::TextAlign,
        None,
        Applicable::ActorKinds(&[A::Text, A::Typst, A::Code]),
        |_| super::property_engine::PropertyValue::String("left".to_string())
    ),
    schema!(
        "text_max_width",
        ValueType::F32,
        F::ASSIGNABLE_A,
        ActorField::Tagged("legend_text_max_width"),
        None,
        Applicable::ActorKinds(&[A::Legend]),
        |_| super::property_engine::PropertyValue::F32(240.0)
    ),
    schema!(
        "tick_labels",
        ValueType::String,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::Graph]),
        |_| super::property_engine::PropertyValue::String("auto".to_string())
    ),
    schema!(
        "ticks",
        ValueType::String,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::Graph]),
        |_| super::property_engine::PropertyValue::String("auto".to_string())
    ),
    schema!(
        "title",
        ValueType::String,
        F::ASSIGNABLE_A,
        ActorField::Tagged("legend_title"),
        None,
        Applicable::ActorKinds(&[A::Legend]),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "to",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::LineTo,
        Some(GroupMembership {
            group_id: GroupHandlerId::VectorShapeState
        }),
        Applicable::ActorKinds(&[A::Shape(S::Line), A::Shape(S::Arrow), A::Callout]),
        |_| super::property_engine::PropertyValue::Vec2([100.0, 0.0])
    ),
    schema!(
        "to_offset",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::CalloutToOffset,
        None,
        Applicable::ActorKinds(&[A::Callout]),
        |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])
    ),
    schema!(
        "tolerance",
        ValueType::F32,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::PlotCurve]),
        |_| super::property_engine::PropertyValue::F32(2.0)
    ),
    schema!(
        "transform",
        ValueType::Transform,
        F::ASSIGNABLE_AI,
        ActorField::Transform,
        None,
        Applicable::Everything,
        |_| super::property_engine::PropertyValue::Transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    ),
    schema!(
        "url",
        ValueType::String,
        F::ASSIGNABLE,
        ActorField::ImageData,
        None,
        Applicable::ActorKinds(&[A::Image, A::Svg]),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    schema!(
        "vertical_align",
        ValueType::String,
        F::empty(),
        ActorField::NoStorage,
        None,
        Applicable::ActorKinds(&[A::Row, A::Col]),
        |_| super::property_engine::PropertyValue::String("center".to_string())
    ),
    schema!(
        "volume",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::AudioVolume,
        None,
        Applicable::ActorKinds(&[A::Audio]),
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    schema!(
        "width",
        ValueType::F32,
        F::ANIMATED_I,
        ActorField::Size,
        None,
        Applicable::SizedActors,
        |_| super::property_engine::PropertyValue::F32(100.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 0,
            scale: 2.0
        }
    ),
    schema!(
        "word_spacing",
        ValueType::F32,
        F::ASSIGNABLE,
        ActorField::WordSpacing,
        None,
        Applicable::ActorKinds(&[A::Text, A::Typst, A::Code]),
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    schema!(
        "x_domain",
        ValueType::Vec2,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[
            A::Graph,
            A::PlotCurve,
            A::VectorField,
            A::Heatmap,
            A::ContourSet,
            A::NumberPlane,
            A::BarChart
        ]),
        |_| super::property_engine::PropertyValue::Vec2([-5.0, 5.0])
    ),
    schema!(
        "x_range",
        ValueType::Vec2,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::NumberPlane]),
        |_| super::property_engine::PropertyValue::Vec2([-10.0, 10.0])
    ),
    schema!(
        "x_scale",
        ValueType::String,
        F::empty(),
        ActorField::NoStorage,
        None,
        Applicable::ActorKinds(&[A::Graph]),
        |_| super::property_engine::PropertyValue::String("linear".to_string())
    ),
    schema!(
        "y_domain",
        ValueType::Vec2,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[
            A::Graph,
            A::PlotCurve,
            A::VectorField,
            A::Heatmap,
            A::ContourSet,
            A::NumberPlane,
            A::BarChart
        ]),
        |_| super::property_engine::PropertyValue::Vec2([-5.0, 5.0])
    ),
    schema!(
        "y_range",
        ValueType::Vec2,
        F::empty(),
        ActorField::PlotDomainGroup,
        Some(GroupMembership {
            group_id: GroupHandlerId::PlotDomain
        }),
        Applicable::ActorKinds(&[A::NumberPlane]),
        |_| super::property_engine::PropertyValue::Vec2([-10.0, 10.0])
    ),
    schema!(
        "y_scale",
        ValueType::String,
        F::empty(),
        ActorField::NoStorage,
        None,
        Applicable::ActorKinds(&[A::Graph]),
        |_| super::property_engine::PropertyValue::String("linear".to_string())
    ),
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
            assert!(found.is_some(), "Property '{}' cannot be looked up by name", schema.name);
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
