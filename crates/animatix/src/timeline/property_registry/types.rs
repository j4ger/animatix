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
    /// List of floats (stroke dash pattern).
    F32List,
    /// A color ramp authored as a gradient paint.
    Gradient,
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
    pub const ANIMATED: Self = Self(0b0000_0001);
    /// This property can be assigned from source expressions.
    pub const ASSIGNABLE: Self = Self(0b0000_0010);
    /// This property can receive injected environment values.
    pub const INJECTABLE: Self = Self(0b0000_0100);
    /// Changes to this property affect layout resolution.
    pub const LAYOUT_AFFECTING: Self = Self(0b0000_1000);
    /// Changes to this property alter spatial placement, orientation, or dimensions.
    pub const SPATIAL: Self = Self(0b0001_0000);
    /// Changes to this property alter visibility or presence.
    pub const PRESENCE: Self = Self(0b0010_0000);
    /// Changes to this property require regenerating vector paths.
    pub const PATH_AFFECTING: Self = Self(0b0100_0000);

    // Convenience combinations for use in static PROPERTY_REGISTRY
    /// `ANIMATED | ASSIGNABLE` combined.
    pub const ASSIGNABLE_A: Self = Self(0b0000_0011); // ANIMATED | ASSIGNABLE
    /// `ANIMATED | ASSIGNABLE | INJECTABLE` combined.
    pub const ASSIGNABLE_AI: Self = Self(0b0000_0111); // ANIMATED | ASSIGNABLE | INJECTABLE
    /// `ANIMATED | INJECTABLE` combined.
    pub const ANIMATED_I: Self = Self(0b0000_0101); // ANIMATED | INJECTABLE
    /// `ANIMATED | ASSIGNABLE | INJECTABLE | SPATIAL` combined.
    pub const SPATIAL_AI: Self = Self(0b0001_0111);
    /// `ANIMATED | ASSIGNABLE | INJECTABLE | PRESENCE` combined.
    pub const PRESENCE_AI: Self = Self(0b0010_0111);
    /// `ANIMATED | ASSIGNABLE | INJECTABLE | PATH_AFFECTING` combined.
    pub const PATH_AI: Self = Self(0b0100_0111);
    /// `ANIMATED | ASSIGNABLE | INJECTABLE | SPATIAL | PATH_AFFECTING` combined.
    pub const SPATIAL_PATH_AI: Self = Self(0b0101_0111);
    /// All basic flags combined.
    pub const ALL: Self = Self(0b0000_1111); // all basic flags

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
        world: Option<&crate::timeline::World>,
        time_ms: u64,
    ) -> Option<crate::timeline::PropertyValue> {
        let read = |field| match world {
            Some(world) => world.read_property_value(track.id, field, time_ms),
            None => crate::timeline::dispatch::read_property_value(track, field, time_ms),
        };
        match self {
            ReadSource::Field(f) | ReadSource::Alias(f) => read(*f),
            ReadSource::Component {
                field,
                index,
                scale,
            } => read(*field).map(|pv| {
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
    /// Stroke dash pattern (segment/gap lengths in scene pixels).
    DashPattern,
    /// Phase offset into the dash pattern.
    DashOffset,
    /// Compositing blend mode for the node's subtree ("normal", "screen", …).
    Blend,
    /// Fill paint that ramps between colors.
    FillGradient,
    /// Stroke paint that ramps between colors.
    StrokeGradient,
    /// How a gradient behaves past its end stops.
    GradientExtend,
    /// Color space a gradient ramps through.
    GradientSpace,

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
    /// Text anchor placement relative to 'at' ("left", "center", "right").
    TextAnchor,
    /// Overflow behavior ("visible", "clip", "ellipsis").
    Overflow,
    /// Syntax-highlighting language tag for `Code` actors (Typst raw fence
    /// token, e.g. "rust"). Empty string = no highlighting.
    Language,

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
    pub fn default_value(self) -> Option<crate::timeline::property_engine::PropertyValue> {
        use crate::timeline::animation_track::{DEFAULT_LAYOUT_HALF_SIZE, DEFAULT_WHITE};
        use crate::timeline::property_engine::PropertyValue;
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
            ActorField::DashPattern => PropertyValue::F32List(Vec::new()),
            ActorField::DashOffset => PropertyValue::F32(0.0),
            ActorField::Blend => PropertyValue::String("normal".to_string()),
            ActorField::FillGradient | ActorField::StrokeGradient => {
                PropertyValue::Gradient(Default::default())
            },
            ActorField::GradientExtend => PropertyValue::String("pad".to_string()),
            ActorField::GradientSpace => PropertyValue::String("oklab".to_string()),
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
            ActorField::TextAnchor => PropertyValue::String("center".to_string()),
            ActorField::Overflow => PropertyValue::String("visible".to_string()),
            ActorField::Language => PropertyValue::String(String::new()),
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
    pub default_value:
        fn(&crate::timeline::ActorCaps) -> crate::timeline::property_engine::PropertyValue,
    /// How this property is read at frame time (env injection, `_animating` flags).
    pub read_source: ReadSource,
}

impl PropertySchema {
    /// Returns `true` if this property affects 2D spatial placement, dimensions,
    /// or transformation.
    pub fn is_spatial(&self) -> bool {
        self.flags.contains(PropertyFlags::SPATIAL)
    }

    /// Returns `true` if this property controls presence or visibility (e.g. opacity).
    pub fn is_presence(&self) -> bool {
        self.flags.contains(PropertyFlags::PRESENCE)
    }

    /// Returns `true` if modifying this property at runtime alters an actor's
    /// position, extents, or presence, and thus affects or invalidates
    /// build-time scene statistics (`scene.stats.*`).
    pub fn affects_scene_statistics(&self) -> bool {
        self.is_spatial() || self.is_presence()
    }

    /// Returns `true` if keyframing this property requires regenerating cached
    /// vector paths.
    pub fn affects_vector_paths(&self) -> bool {
        self.flags.contains(PropertyFlags::PATH_AFFECTING)
    }
}

// ─────────────────────────────────────────────────────────────
// The registry — sorted by name for binary search
// ─────────────────────────────────────────────────────────────
