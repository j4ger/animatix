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
//! 2. Match over `schema.field` / `schema.flags`  (exhaustive enum)
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
    /// Corner rounding radius for rectangles, in scene pixels.
    CornerRadius,
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

            ActorField::Transform => PropertyValue::Transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]),

            ActorField::ShapeType => PropertyValue::U32(0),
            ActorField::LineFrom => PropertyValue::Vec2([-50.0, 0.0]),
            ActorField::LineTo => PropertyValue::Vec2([50.0, 0.0]),

            ActorField::CornerRadius => PropertyValue::F32(0.0),
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
            ActorField::CalloutPlace => PropertyValue::Enum("right".to_string()),
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

// ─────────────────────────────────────────────────────────────
// Property schema
// ─────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────
// Applicability — which actor kinds a property is valid for
// ─────────────────────────────────────────────────────────────

/// Declares which actors a property applies to.
///
/// The predicate vocabulary lives in `animatix-core::caps::Applicable`;
/// re-exported here because the runtime registry, plan builder, and inspector
/// address it through this module.
pub use animatix_core::caps::Applicable;

/// The complete description of one property in the system.
///
/// This is pure data — no function pointers. All dispatch logic is driven
/// by matching over the enum fields (ValueType, ActorField).
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

    /// Which actor kinds this property is applicable to.
    pub applicable: Applicable,

    /// Default value for this property when the actor does not declare it.
    /// Computed at runtime because some defaults depend on actor kind
    /// (e.g. `font_size` is 48 for Text, 36 for Typst, 24 for Code).
    pub default_value: fn(&super::ActorCaps) -> super::property_engine::PropertyValue,
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

macro_rules! binding {
    ($name:expr, $ty:expr, $flags:expr, $field:expr, $default:expr $(, $read:expr)?) => {
        PropertyBinding {
            name: $name,
            value_type: $ty,
            flags: $flags,
            field: $field,
            default_value: $default,
            read_source: binding!(@read $field $(, $read)?),
        }
    };
    (@read $field:expr) => {
        ReadSource::Field($field)
    };
    (@read $field:expr, $read:expr) => {
        $read
    };
}

/// One property's engine-side binding.
///
/// Everything here is engine-only; the shared metadata (name, applicability,
/// finite value kind) comes from the core descriptor table, so adding a
/// property cannot leave the two sides disagreeing about what a property is.
#[derive(Clone, Copy)]
struct PropertyBinding {
    /// Canonical name — the join key with the descriptor table.
    name: &'static str,
    /// Fine-grained plan-slot type (adds shape/placement/anchor kinds).
    value_type: ValueType,
    /// Feature flags.
    flags: PropertyFlags,
    /// Storage field or side-effect handler.
    field: ActorField,
    /// Default when the actor does not declare the property.
    default_value: fn(&super::ActorCaps) -> super::property_engine::PropertyValue,
    /// How the property is read at frame time.
    read_source: ReadSource,
}

/// The composed registry: shared descriptors joined with engine bindings.
///
/// Name-sorted, so the existing binary-search lookups and the indices returned
/// by [`allowed_property_indices`] keep working. Composing (rather than
/// re-declaring) is what makes the descriptor table the single source for
/// names and applicability; the join is checked by
/// `every_binding_has_a_descriptor` and `unbound_descriptors_are_pinned`.
pub static PROPERTY_REGISTRY: std::sync::LazyLock<Vec<PropertySchema>> =
    std::sync::LazyLock::new(|| {
        BINDINGS
            .iter()
            .map(|binding| {
                let descriptor =
                    animatix_core::property::descriptor(binding.name).unwrap_or_else(|| {
                        panic!(
                            "property binding `{}` has no descriptor in \
                             animatix_core::property::PROPERTY_DESCRIPTORS",
                            binding.name
                        )
                    });
                PropertySchema {
                    name: binding.name,
                    value_type: binding.value_type,
                    flags: binding.flags,
                    field: binding.field,
                    applicable: descriptor.applicable,
                    default_value: binding.default_value,
                    read_source: binding.read_source,
                }
            })
            .collect()
    });

/// Registry of all built-in actor properties with their schemas.
/// Engine-side bindings, name-sorted (binary search depends on it).
///
/// A binding carries only what the runtime adds to the shared descriptor in
/// `animatix-core`: the finer [`ValueType`] its plan slot stores, the property
/// flags, the storage field, the default value, and the frame-time read
/// source. Names, applicability, and the finite value kind live in
/// [`animatix_core::property::PROPERTY_DESCRIPTORS`] and are joined in by
/// [`PROPERTY_REGISTRY`].
static BINDINGS: &[PropertyBinding] = &[
    binding!("align", ValueType::String, F::empty(), ActorField::ContainerLayoutGroup, |_| {
        super::property_engine::PropertyValue::String("center".to_string())
    }),
    binding!(
        "anchor",
        ValueType::SceneAnchor,
        F::ASSIGNABLE_AI,
        ActorField::PositionBindingGroup,
        |_| super::property_engine::PropertyValue::String("center".to_string()),
        ReadSource::None_
    ),
    binding!("ascent", ValueType::F32, F::ANIMATED, ActorField::Ascent, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!(
        "at",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::PositionBindingGroup,
        |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0]),
        ReadSource::Alias(ActorField::Position)
    ),
    binding!(
        "background_color",
        ValueType::Color,
        F::ASSIGNABLE_AI,
        ActorField::Color,
        |_| super::property_engine::PropertyValue::Color([0.0, 0.0, 0.0, 1.0]),
        ReadSource::None_
    ),
    binding!(
        "bar_colors",
        ValueType::BuildTimeOnly,
        F::empty(),
        ActorField::NoStorage,
        |_| super::property_engine::PropertyValue::String("auto".to_string())
    ),
    binding!("bar_width", ValueType::F32, F::empty(), ActorField::NoStorage, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!("baseline", ValueType::F32, F::ANIMATED, ActorField::Baseline, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!(
        "bounds",
        ValueType::Vec4,
        F::ASSIGNABLE_AI,
        ActorField::Tagged("filter_bounds"),
        |_| super::property_engine::PropertyValue::Vec4([0.0, 0.0, 0.0, 0.0])
    ),
    binding!(
        "char_progress",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::CharProgress,
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    binding!("code", ValueType::String, F::ANIMATED, ActorField::TextContent, |_| {
        super::property_engine::PropertyValue::String(String::new())
    }),
    binding!("color", ValueType::Color, F::ASSIGNABLE_AI, ActorField::Color, |_| {
        super::property_engine::PropertyValue::Color([1.0, 1.0, 1.0, 1.0])
    }),
    binding!("cols", ValueType::U32, F::empty(), ActorField::ContainerLayoutGroup, |_| {
        super::property_engine::PropertyValue::U32(2)
    }),
    binding!(
        "commands",
        ValueType::CommandList,
        F::ASSIGNABLE_A,
        ActorField::Commands,
        |_| super::property_engine::PropertyValue::CommandList(String::new())
    ),
    binding!(
        "corner_radius",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::CornerRadius,
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    binding!("data", ValueType::BuildTimeOnly, F::empty(), ActorField::NoStorage, |_| {
        super::property_engine::PropertyValue::String("auto".to_string())
    }),
    binding!("density", ValueType::F32, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::F32(16.0)
    }),
    binding!("descent", ValueType::F32, F::ANIMATED, ActorField::Descent, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!("direction", ValueType::String, F::empty(), ActorField::NoStorage, |_| {
        super::property_engine::PropertyValue::String("vertical".to_string())
    }),
    binding!(
        "fill_opacity",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::FillOpacity,
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    binding!("font_family", ValueType::String, F::ASSIGNABLE, ActorField::FontFamily, |_| {
        super::property_engine::PropertyValue::String(
            crate::renderer::text::DEFAULT_FONT_FAMILY.to_string(),
        )
    }),
    binding!("font_size", ValueType::F32, F::ASSIGNABLE_A, ActorField::FontSize, |caps| {
        match caps.text {
            Some(crate::timeline::actor_caps::TextKind::Text) => {
                super::property_engine::PropertyValue::F32(48.0)
            },
            Some(crate::timeline::actor_caps::TextKind::Typst) => {
                super::property_engine::PropertyValue::F32(36.0)
            },
            _ => super::property_engine::PropertyValue::F32(24.0),
        }
    }),
    binding!("font_style", ValueType::String, F::ASSIGNABLE, ActorField::FontStyle, |_| {
        super::property_engine::PropertyValue::String("normal".to_string())
    }),
    binding!("font_weight", ValueType::F32, F::ASSIGNABLE, ActorField::FontWeight, |_| {
        super::property_engine::PropertyValue::F32(400.0)
    }),
    binding!("from", ValueType::Vec2, F::ASSIGNABLE_AI, ActorField::LineFrom, |_| {
        super::property_engine::PropertyValue::Vec2([0.0, 0.0])
    }),
    binding!(
        "func",
        ValueType::BuildTimeOnly,
        F::empty(),
        ActorField::PlotDomainGroup,
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    binding!("gap", ValueType::F32, F::empty(), ActorField::ContainerLayoutGroup, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!("grid", ValueType::String, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::String("auto".to_string())
    }),
    binding!("head_size", ValueType::F32, F::ASSIGNABLE_AI, ActorField::HeadSize, |_| {
        super::property_engine::PropertyValue::F32(10.0)
    }),
    binding!(
        "height",
        ValueType::F32,
        F::ANIMATED_I,
        ActorField::Size,
        |_| super::property_engine::PropertyValue::F32(100.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 1,
            scale: 2.0
        }
    ),
    binding!(
        "highlight_color",
        ValueType::Color,
        F::ANIMATED,
        ActorField::HighlightColor,
        |_| super::property_engine::PropertyValue::Vec4([0.3, 0.5, 1.0, 1.0])
    ),
    binding!(
        "highlight_opacity",
        ValueType::F32,
        F::ANIMATED,
        ActorField::HighlightOpacity,
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    binding!(
        "highlight_padding",
        ValueType::F32,
        F::ANIMATED,
        ActorField::HighlightPadding,
        |_| super::property_engine::PropertyValue::F32(4.0)
    ),
    binding!(
        "highlight_radius",
        ValueType::F32,
        F::ANIMATED,
        ActorField::HighlightRadius,
        |_| super::property_engine::PropertyValue::F32(3.0)
    ),
    binding!("kind", ValueType::String, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::String("cartesian".to_string())
    }),
    binding!("label", ValueType::String, F::ASSIGNABLE_A, ActorField::TextContent, |_| {
        super::property_engine::PropertyValue::String(String::new())
    }),
    binding!("label_at", ValueType::Vec2, F::ASSIGNABLE_AI, ActorField::LabelAt, |_| {
        super::property_engine::PropertyValue::Vec2([0.0, 0.0])
    }),
    binding!(
        "label_color",
        ValueType::Color,
        F::ASSIGNABLE_A,
        ActorField::Tagged("legend_label_color"),
        |_| super::property_engine::PropertyValue::Color([1.0, 1.0, 1.0, 1.0])
    ),
    binding!(
        "legend",
        ValueType::Sum(LEGEND_SUM_VARIANTS),
        F::ASSIGNABLE_A,
        ActorField::Tagged("legend"),
        |_| super::property_engine::PropertyValue::Bool(true)
    ),
    binding!(
        "letter_spacing",
        ValueType::F32,
        F::ASSIGNABLE,
        ActorField::LetterSpacing,
        |_| super::property_engine::PropertyValue::F32(0.0)
    ),
    binding!("levels", ValueType::Vec2, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::Vec2([0.0, 1.0])
    }),
    binding!("line_cap", ValueType::U32, F::ASSIGNABLE_AI, ActorField::LineCap, |_| {
        super::property_engine::PropertyValue::U32(0)
    }),
    binding!("line_height", ValueType::F32, F::ASSIGNABLE, ActorField::LineHeight, |_| {
        super::property_engine::PropertyValue::F32(1.2)
    }),
    binding!("line_join", ValueType::U32, F::ASSIGNABLE_AI, ActorField::LineJoin, |_| {
        super::property_engine::PropertyValue::U32(0)
    }),
    binding!("max_depth", ValueType::F32, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::F32(12.0)
    }),
    binding!("max_height", ValueType::F32, F::ASSIGNABLE_AI, ActorField::MaxHeight, |_| {
        super::property_engine::PropertyValue::F32(f32::INFINITY)
    }),
    binding!("max_value", ValueType::F32, F::empty(), ActorField::NoStorage, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!("max_width", ValueType::F32, F::ASSIGNABLE, ActorField::TextMaxWidth, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!("min_height", ValueType::F32, F::ASSIGNABLE_AI, ActorField::MinHeight, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!("min_width", ValueType::F32, F::ASSIGNABLE_AI, ActorField::MinWidth, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!(
        "offset",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::PositionBindingGroup,
        |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0]),
        ReadSource::None_
    ),
    binding!("opacity", ValueType::F32, F::ASSIGNABLE_AI, ActorField::Opacity, |_| {
        super::property_engine::PropertyValue::F32(1.0)
    }),
    binding!("overflow", ValueType::String, F::ASSIGNABLE, ActorField::Overflow, |_| {
        super::property_engine::PropertyValue::String("visible".to_string())
    }),
    binding!("padding", ValueType::F32, F::empty(), ActorField::ContainerLayoutGroup, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!(
        "place",
        ValueType::Enum(&["auto", "top", "bottom", "left", "right", "above", "below"]),
        F::ASSIGNABLE,
        ActorField::Tagged("callout_place"),
        |_| super::property_engine::PropertyValue::Enum("right".to_string())
    ),
    binding!("points", ValueType::PointList, F::ASSIGNABLE_A, ActorField::Points, |_| {
        super::property_engine::PropertyValue::PointList(Vec::new())
    }),
    binding!("position", ValueType::Vec2, F::ASSIGNABLE_AI, ActorField::Position, |_| {
        super::property_engine::PropertyValue::Vec2([0.0, 0.0])
    }),
    binding!(
        "radius_x",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::Size,
        |_| super::property_engine::PropertyValue::F32(50.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 0,
            scale: 1.0
        }
    ),
    binding!(
        "radius_y",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::Size,
        |_| super::property_engine::PropertyValue::F32(50.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 1,
            scale: 1.0
        }
    ),
    binding!("resolution", ValueType::F32, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::F32(48.0)
    }),
    binding!("rotation", ValueType::F32, F::ASSIGNABLE_AI, ActorField::Rotation, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!("scale", ValueType::F32, F::ASSIGNABLE_AI, ActorField::Scale, |_| {
        super::property_engine::PropertyValue::F32(1.0)
    }),
    binding!("shift", ValueType::Vec2, F::ASSIGNABLE_AI, ActorField::MotionOffset, |_| {
        super::property_engine::PropertyValue::Vec2([0.0, 0.0])
    }),
    binding!("show_axis", ValueType::BuildTimeOnly, F::empty(), ActorField::NoStorage, |_| {
        super::property_engine::PropertyValue::String("true".to_string())
    }),
    binding!(
        "show_labels",
        ValueType::BuildTimeOnly,
        F::empty(),
        ActorField::NoStorage,
        |_| super::property_engine::PropertyValue::String("true".to_string())
    ),
    binding!("size", ValueType::Vec2, F::ALL, ActorField::Size, |_| {
        super::property_engine::PropertyValue::Vec2([50.0, 50.0])
    }),
    binding!("solo", ValueType::Bool, F::ASSIGNABLE, ActorField::Tagged("solo"), |_| {
        super::property_engine::PropertyValue::Bool(false)
    }),
    binding!("source", ValueType::String, F::ASSIGNABLE, ActorField::AudioSource, |_| {
        super::property_engine::PropertyValue::String(String::new())
    }),
    binding!(
        "standoff",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::CalloutStandoff,
        |_| super::property_engine::PropertyValue::F32(40.0)
    ),
    binding!("stroke", ValueType::Color, F::ASSIGNABLE_AI, ActorField::StrokeColor, |_| {
        super::property_engine::PropertyValue::Color([1.0, 1.0, 1.0, 1.0])
    }),
    binding!(
        "stroke_progress",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::StrokeProgress,
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    binding!(
        "stroke_width",
        ValueType::F32,
        F::ASSIGNABLE_AI,
        ActorField::StrokeWidth,
        |_| super::property_engine::PropertyValue::F32(1.0)
    ),
    binding!(
        "swatch_size",
        ValueType::F32,
        F::ASSIGNABLE_A,
        ActorField::Tagged("legend_swatch_size"),
        |_| super::property_engine::PropertyValue::F32(16.0)
    ),
    binding!("t_domain", ValueType::Vec2, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::Vec2([0.0, 1.0])
    }),
    binding!(
        "target",
        ValueType::BuildTimeOnly,
        F::ASSIGNABLE,
        ActorField::CalloutTarget,
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    binding!("text", ValueType::String, F::ASSIGNABLE_A, ActorField::TextContent, |_| {
        super::property_engine::PropertyValue::String(String::new())
    }),
    binding!("text_align", ValueType::String, F::ASSIGNABLE, ActorField::TextAlign, |_| {
        super::property_engine::PropertyValue::String("left".to_string())
    }),
    binding!(
        "text_max_width",
        ValueType::F32,
        F::ASSIGNABLE_A,
        ActorField::Tagged("legend_text_max_width"),
        |_| super::property_engine::PropertyValue::F32(240.0)
    ),
    binding!(
        "tick_labels",
        ValueType::String,
        F::empty(),
        ActorField::PlotDomainGroup,
        |_| super::property_engine::PropertyValue::String("auto".to_string())
    ),
    binding!("ticks", ValueType::String, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::String("auto".to_string())
    }),
    binding!(
        "title",
        ValueType::String,
        F::ASSIGNABLE_A,
        ActorField::Tagged("legend_title"),
        |_| super::property_engine::PropertyValue::String(String::new())
    ),
    binding!("to", ValueType::Vec2, F::ASSIGNABLE_AI, ActorField::LineTo, |_| {
        super::property_engine::PropertyValue::Vec2([100.0, 0.0])
    }),
    binding!(
        "to_offset",
        ValueType::Vec2,
        F::ASSIGNABLE_AI,
        ActorField::CalloutToOffset,
        |_| super::property_engine::PropertyValue::Vec2([0.0, 0.0])
    ),
    binding!("tolerance", ValueType::F32, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::F32(2.0)
    }),
    binding!(
        "transform",
        ValueType::Transform,
        F::ASSIGNABLE_AI,
        ActorField::Transform,
        |_| super::property_engine::PropertyValue::Transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    ),
    binding!("url", ValueType::String, F::ASSIGNABLE, ActorField::ImageData, |_| {
        super::property_engine::PropertyValue::String(String::new())
    }),
    binding!("vertical_align", ValueType::String, F::empty(), ActorField::NoStorage, |_| {
        super::property_engine::PropertyValue::String("center".to_string())
    }),
    binding!("volume", ValueType::F32, F::ASSIGNABLE_AI, ActorField::AudioVolume, |_| {
        super::property_engine::PropertyValue::F32(1.0)
    }),
    binding!(
        "width",
        ValueType::F32,
        F::ANIMATED_I,
        ActorField::Size,
        |_| super::property_engine::PropertyValue::F32(100.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 0,
            scale: 2.0
        }
    ),
    binding!("word_spacing", ValueType::F32, F::ASSIGNABLE, ActorField::WordSpacing, |_| {
        super::property_engine::PropertyValue::F32(0.0)
    }),
    binding!("x_domain", ValueType::Vec2, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::Vec2([-5.0, 5.0])
    }),
    binding!("x_range", ValueType::Vec2, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::Vec2([-10.0, 10.0])
    }),
    binding!("x_scale", ValueType::String, F::empty(), ActorField::NoStorage, |_| {
        super::property_engine::PropertyValue::String("linear".to_string())
    }),
    binding!("y_domain", ValueType::Vec2, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::Vec2([-5.0, 5.0])
    }),
    binding!("y_range", ValueType::Vec2, F::empty(), ActorField::PlotDomainGroup, |_| {
        super::property_engine::PropertyValue::Vec2([-10.0, 10.0])
    }),
    binding!("y_scale", ValueType::String, F::empty(), ActorField::NoStorage, |_| {
        super::property_engine::PropertyValue::String("linear".to_string())
    }),
];

// ─────────────────────────────────────────────────────────────
// Lookup
// ─────────────────────────────────────────────────────────────

/// Return the index of a property in [`PROPERTY_REGISTRY`].
///
/// Uses the name map rather than a binary search: the map is built once and
/// already carries the index, and the composed registry is behind a
/// `LazyLock`, so a hash lookup beats re-dereferencing the table for every
/// probe.
fn property_index(name: &str) -> Option<usize> {
    resolved_properties().get(name).map(|(index, _)| *index)
}

/// Look up a property schema by name.
///
/// Uses binary search over the sorted `PROPERTY_REGISTRY`.
/// Returns `None` if no property with that name exists.
pub fn lookup_property(name: &str) -> Option<&'static PropertySchema> {
    property_index(name).map(|i| &PROPERTY_REGISTRY[i])
}

/// Resolve a property name to its stable shared-schema [`animatix_syntax::schema::PropertyId`].
///
/// The id comes from the shared schema declaration order, so analyzer,
/// typechecker, and runtime plans all address the same property namespace.
pub fn property_id(name: &str) -> Option<animatix_syntax::schema::PropertyId> {
    schema_property_ids().get(name).copied()
}

/// Resolve a property name to its schema *and* the runtime plan slot id in one
/// lookup.
///
/// `effective_*` needs both on every property read — at least five per actor
/// per frame — and each used to cost its own lookup (a binary search over the
/// sorted registry, then a hash into the id map). Folding them into one
/// process-wide table removes a repeat lookup from the frame path.
pub fn resolve_property(
    name: &str,
) -> Option<(&'static PropertySchema, Option<animatix_syntax::schema::PropertyId>)> {
    let (index, slot) = *resolved_properties().get(name)?;
    Some((&PROPERTY_REGISTRY[index], slot))
}

/// A registry entry pre-resolved to its schema and plan-slot id.
pub(crate) type ResolvedPropertyRead =
    Option<(&'static PropertySchema, Option<animatix_syntax::schema::PropertyId>)>;

/// Pre-resolved registry entries for the fixed transform-path property set
/// (`rotation`, `scale`, `opacity`, `size`, `transform`).
///
/// `evaluate_node_transform` reads these five per node per frame; resolving
/// them once per process removes a string-hash lookup per property per node
/// while leaving the read itself (`read_property_value_resolved`) and the
/// modifier-override check untouched. `None` (property absent from the
/// registry) is preserved verbatim so behavior matches a failed
/// `resolve_property` exactly.
pub(crate) struct TransformPropertyReads {
    pub rotation: ResolvedPropertyRead,
    pub scale: ResolvedPropertyRead,
    pub opacity: ResolvedPropertyRead,
    pub size: ResolvedPropertyRead,
    pub transform: ResolvedPropertyRead,
}

pub(crate) fn transform_property_reads() -> &'static TransformPropertyReads {
    static CACHE: std::sync::OnceLock<TransformPropertyReads> = std::sync::OnceLock::new();
    CACHE.get_or_init(|| TransformPropertyReads {
        rotation: resolve_property("rotation"),
        scale: resolve_property("scale"),
        opacity: resolve_property("opacity"),
        size: resolve_property("size"),
        transform: resolve_property("transform"),
    })
}
/// The plan-slot id for a property's storage field, or `None` when the field is
/// not a tagged slot the runtime plan can serve.
///
/// Mirrors the guard in `dispatch::read_property_value`: `legend` and
/// `callout_place` are tagged but deliberately excluded from plan reads.
fn plan_slot_id(field: &ActorField) -> Option<animatix_syntax::schema::PropertyId> {
    let ActorField::Tagged(name) = field else {
        return None;
    };
    if *name == "legend" || *name == "callout_place" {
        return None;
    }
    property_id(name)
}

type ResolvedProperty = (usize, Option<animatix_syntax::schema::PropertyId>);

fn resolved_properties() -> &'static std::collections::HashMap<&'static str, ResolvedProperty> {
    static CACHE: std::sync::OnceLock<std::collections::HashMap<&'static str, ResolvedProperty>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        PROPERTY_REGISTRY
            .iter()
            .enumerate()
            .map(|(index, schema)| (schema.name, (index, plan_slot_id(&schema.field))))
            .collect()
    })
}

fn schema_property_ids()
-> &'static std::collections::HashMap<&'static str, animatix_syntax::schema::PropertyId> {
    static CACHE: std::sync::OnceLock<
        std::collections::HashMap<&'static str, animatix_syntax::schema::PropertyId>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        animatix_syntax::schema::property_specs()
            .into_iter()
            .map(|spec| (spec.name, spec.id))
            .collect()
    })
}

/// Look up a property schema by stable shared-schema [`animatix_syntax::schema::PropertyId`].
pub fn property_schema_by_id(
    id: animatix_syntax::schema::PropertyId,
) -> Option<&'static PropertySchema> {
    property_schema_by_id_cache().get(&id).copied()
}

fn property_schema_by_id_cache()
-> &'static std::collections::HashMap<animatix_syntax::schema::PropertyId, &'static PropertySchema>
{
    static CACHE: std::sync::OnceLock<
        std::collections::HashMap<animatix_syntax::schema::PropertyId, &'static PropertySchema>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for schema in PROPERTY_REGISTRY.iter() {
            if let Some(id) = property_id(schema.name) {
                map.insert(id, schema);
            }
        }
        map
    })
}

/// Look up the canonical name for a stable shared-schema [`animatix_syntax::schema::PropertyId`].
pub fn property_name(id: animatix_syntax::schema::PropertyId) -> Option<&'static str> {
    property_schema_by_id(id).map(|schema| schema.name)
}

// ─────────────────────────────────────────────────────────────
// Per-actor-kind allowed property indices
// ─────────────────────────────────────────────────────────────

/// Convenience: build a sorted set of allowed property indices for an actor.
/// Returns indices into PROPERTY_REGISTRY.
pub fn allowed_property_indices(caps: &super::ActorCaps, actor_type: &str) -> Vec<usize> {
    PROPERTY_REGISTRY
        .iter()
        .enumerate()
        .filter(|(_, schema)| schema.applicable.includes(caps, actor_type))
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
        for schema in PROPERTY_REGISTRY.iter() {
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

    #[test]
    fn property_ids_roundtrip_through_registry() {
        for schema in PROPERTY_REGISTRY.iter() {
            let id = property_id(schema.name).expect("runtime property must have a schema id");
            assert_eq!(property_schema_by_id(id).map(|s| s.name), Some(schema.name));
            assert_eq!(property_name(id), Some(schema.name));
        }
    }

    /// Every engine binding must have a shared descriptor: the composed
    /// registry panics otherwise, and a test failure names the offender instead
    /// of surfacing at first render.
    #[test]
    fn every_binding_has_a_descriptor() {
        for binding in BINDINGS {
            assert!(
                animatix_core::property::descriptor(binding.name).is_some(),
                "binding `{}` has no descriptor in animatix_core::property::PROPERTY_DESCRIPTORS",
                binding.name
            );
        }
    }

    /// Descriptors the runtime deliberately does not bind.
    ///
    /// Empty since 2026-09-14: every built-in property now has a binding. The
    /// list stays as the gate — a new descriptor without a binding fails
    /// `unbound_descriptors_are_pinned`, so accepting a property the runtime
    /// cannot store has to be a deliberate, reviewed act rather than an
    /// oversight. The eight names that used to be listed here were either pure
    /// second names for an existing field (`content`, `stroke_color`) or had no
    /// consumer at all (`fill`, `radius`, `start`, `end`, `function`,
    /// `language`); they were removed from the table instead, so the analyzer
    /// rejects them with a clear "unknown property" rather than accepting a
    /// drop that never reaches the screen.
    const UNBOUND_DESCRIPTORS: &[&str] = &[];

    #[test]
    fn unbound_descriptors_are_pinned() {
        let bound: std::collections::HashSet<&str> = BINDINGS.iter().map(|b| b.name).collect();
        let unbound: Vec<&str> = animatix_core::property::PROPERTY_DESCRIPTORS
            .iter()
            .map(|descriptor| descriptor.name)
            .filter(|name| !bound.contains(name))
            .collect();
        let mut unbound = unbound;
        unbound.sort_unstable();
        let mut expected = UNBOUND_DESCRIPTORS.to_vec();
        expected.sort_unstable();
        assert_eq!(
            unbound, expected,
            "the set of descriptors without an engine binding changed; add the binding \
             (or extend UNBOUND_DESCRIPTORS with a reason)"
        );
    }

    /// `ValueType` is finer than the shared `PropertyValueKind`, so the mapping
    /// is total here (no `_ => Generic` catch-all, which is exactly what let
    /// `solo`'s `Bool`/`Generic` disagreement hide) and must match the
    /// descriptor for every bound property.
    #[test]
    fn value_kinds_agree_with_descriptors() {
        use animatix_core::property::PropertyValueKind as Kind;

        fn kind_of(value_type: ValueType) -> Kind {
            match value_type {
                ValueType::F32 => Kind::F32,
                ValueType::U32 => Kind::U32,
                ValueType::Vec2 => Kind::Vec2,
                ValueType::Vec4 | ValueType::Color => Kind::Vec4,
                ValueType::String => Kind::String,
                ValueType::Bool => Kind::Bool,
                ValueType::PointList => Kind::PointList,
                ValueType::ShapeType
                | ValueType::PlacementMode
                | ValueType::SceneAnchor
                | ValueType::PositionBinding
                | ValueType::MorphOptions
                | ValueType::CalloutPlace
                | ValueType::CommandList
                | ValueType::BuildTimeOnly
                | ValueType::Enum(_)
                | ValueType::Union(_)
                | ValueType::Sum(_)
                | ValueType::Transform => Kind::Generic,
            }
        }

        for schema in PROPERTY_REGISTRY.iter() {
            let descriptor = animatix_core::property::descriptor(schema.name)
                .expect("binding without descriptor");
            assert_eq!(
                kind_of(schema.value_type),
                descriptor.value_kind,
                "`{}` stores as {:?} in the engine but {:?} in the shared descriptor",
                schema.name,
                schema.value_type,
                descriptor.value_kind
            );
        }
    }

    /// The composed registry must expose the applicability from the descriptor
    /// table (the engine no longer declares it), for every bound property.
    #[test]
    fn applicability_comes_from_the_descriptor_table() {
        for schema in PROPERTY_REGISTRY.iter() {
            let descriptor = animatix_core::property::descriptor(schema.name)
                .expect("binding without descriptor");
            assert_eq!(
                schema.applicable, descriptor.applicable,
                "`{}` applicability disagrees with the descriptor table",
                schema.name
            );
        }
    }

    #[test]
    fn unknown_property_has_no_id() {
        assert_eq!(property_id("definitely_not_a_property"), None);
        assert!(property_schema_by_id(animatix_syntax::schema::PropertyId(u32::MAX)).is_none());
    }
}
