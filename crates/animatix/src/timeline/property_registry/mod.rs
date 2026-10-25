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
use PropertyFlags as F;
use ValueType as V;

macro_rules! val {
    (f32 $v:expr) => {
        |_| super::property_engine::PropertyValue::F32($v)
    };
    (u32 $v:expr) => {
        |_| super::property_engine::PropertyValue::U32($v)
    };
    (bool $v:expr) => {
        |_| super::property_engine::PropertyValue::Bool($v)
    };
    (str $v:expr) => {
        |_| super::property_engine::PropertyValue::String($v.to_string())
    };
    (vec2 $x:expr, $y:expr) => {
        |_| super::property_engine::PropertyValue::Vec2([$x, $y])
    };
    (vec4 $r:expr, $g:expr, $b:expr, $a:expr) => {
        |_| super::property_engine::PropertyValue::Vec4([$r, $g, $b, $a])
    };
    (color $r:expr, $g:expr, $b:expr, $a:expr) => {
        |_| super::property_engine::PropertyValue::Color([$r, $g, $b, $a])
    };
    (gradient) => {
        |_| super::property_engine::PropertyValue::Gradient(Default::default())
    };
    (point_list) => {
        |_| super::property_engine::PropertyValue::PointList(Vec::new())
    };
    (f32_list) => {
        |_| super::property_engine::PropertyValue::F32List(Vec::new())
    };
    (commands) => {
        |_| super::property_engine::PropertyValue::CommandList(String::new())
    };
    (transform) => {
        |_| super::property_engine::PropertyValue::Transform([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])
    };
    (enum $v:expr) => {
        |_| super::property_engine::PropertyValue::Enum($v.to_string())
    };
    (caps $c:expr) => {
        $c
    };
}

macro_rules! b {
    ($name:literal, $ty:expr, $flags:expr, Tagged, $val:expr $(, $read:expr)?) => {
        PropertyBinding {
            name: $name,
            value_type: $ty,
            flags: $flags,
            field: ActorField::Tagged($name),
            default_value: $val,
            read_source: b!(@read ActorField::Tagged($name) $(, $read)?),
        }
    };
    ($name:literal, $ty:expr, $flags:expr, Tagged($tagged_name:literal), $val:expr $(, $read:expr)?) => {
        PropertyBinding {
            name: $name,
            value_type: $ty,
            flags: $flags,
            field: ActorField::Tagged($tagged_name),
            default_value: $val,
            read_source: b!(@read ActorField::Tagged($tagged_name) $(, $read)?),
        }
    };
    ($name:literal, $ty:expr, $flags:expr, $field:ident, $val:expr $(, $read:expr)?) => {
        PropertyBinding {
            name: $name,
            value_type: $ty,
            flags: $flags,
            field: ActorField::$field,
            default_value: $val,
            read_source: b!(@read ActorField::$field $(, $read)?),
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
/// The complete, authoritative registry of every property in Animatix.
static BINDINGS: &[PropertyBinding] = &[
    b!(
        "active_color",
        V::Color,
        F::ASSIGNABLE_AI,
        Tagged,
        val!(color 1.0, 0.8, 0.0, 1.0)
    ),
    b!("align", V::String, F::empty(), ContainerLayoutGroup, val!(str "center")),
    b!(
        "anchor",
        V::SceneAnchor,
        F::ASSIGNABLE_AI,
        PositionBindingGroup,
        val!(str "center"),
        ReadSource::None_
    ),
    b!("arrow", V::Bool, F::ASSIGNABLE_AI, Tagged, val!(bool true)),
    b!("ascent", V::F32, F::ANIMATED, Ascent, val!(f32 0.0)),
    b!(
        "at",
        V::Vec2,
        F::SPATIAL_AI,
        PositionBindingGroup,
        val!(vec2 0.0, 0.0),
        ReadSource::Alias(ActorField::Position)
    ),
    b!("backdrop", V::Bool, F::ASSIGNABLE_AI, Tagged, val!(bool false)),
    b!(
        "backdrop_color",
        V::Color,
        F::ASSIGNABLE_AI,
        Tagged,
        val!(color 0.0, 0.0, 0.0, 0.65)
    ),
    b!("backdrop_padding", V::Vec2, F::ASSIGNABLE_AI, Tagged, val!(vec2 16.0, 8.0)),
    b!("backdrop_radius", V::F32, F::ASSIGNABLE_AI, Tagged, val!(f32 8.0)),
    b!(
        "background_color",
        V::Color,
        F::ASSIGNABLE_AI,
        Color,
        val!(color 0.0, 0.0, 0.0, 1.0),
        ReadSource::None_
    ),
    b!("bar_colors", V::BuildTimeOnly, F::empty(), NoStorage, val!(str "auto")),
    b!("bar_width", V::F32, F::empty(), NoStorage, val!(f32 0.0)),
    b!("baseline", V::F32, F::ANIMATED, Baseline, val!(f32 0.0)),
    // `blend` and `dash_pattern` are deliberately not INJECTABLE: the frame
    // environment injects every INJECTABLE row of every actor every frame, and a
    // blend name / dash list is written and stamped, never read back. See the note
    // on `fill_gradient`.
    b!("blend", V::String, F::ASSIGNABLE_A, Blend, val!(str "normal")),
    b!(
        "bounds",
        V::Vec4,
        F::ASSIGNABLE_AI,
        Tagged("filter_bounds"),
        val!(vec4 0.0, 0.0, 0.0, 0.0)
    ),
    b!("camera_follow", V::BuildTimeOnly, F::empty(), NoStorage, val!(bool true)),
    b!("caption_mode", V::String, F::ASSIGNABLE_AI, Tagged, val!(str "block")),
    b!("char_progress", V::F32, F::ASSIGNABLE_AI, CharProgress, val!(f32 1.0)),
    b!("code", V::String, F::ANIMATED, TextContent, val!(str "")),
    b!("color", V::Color, F::ASSIGNABLE_AI, Color, val!(color 1.0, 1.0, 1.0, 1.0)),
    b!("cols", V::U32, F::empty(), ContainerLayoutGroup, val!(u32 2)),
    b!("comma", V::Bool, F::ASSIGNABLE_AI, Tagged, val!(bool false)),
    b!(
        "commands",
        V::CommandList,
        F::ASSIGNABLE_A.union(F::SPATIAL).union(F::PATH_AFFECTING),
        Commands,
        val!(commands)
    ),
    b!("corner_radius", V::F32, F::PATH_AI, CornerRadius, val!(f32 0.0)),
    b!("dash_offset", V::F32, F::ASSIGNABLE_AI, DashOffset, val!(f32 0.0)),
    b!("dash_pattern", V::F32List, F::ASSIGNABLE_A, DashPattern, val!(f32_list)),
    b!("data", V::BuildTimeOnly, F::empty(), NoStorage, val!(str "auto")),
    b!("decimals", V::U32, F::ASSIGNABLE_AI, Tagged, val!(u32 0)),
    b!("density", V::F32, F::empty(), PlotDomainGroup, val!(f32 16.0)),
    b!("descent", V::F32, F::ANIMATED, Descent, val!(f32 0.0)),
    b!("direction", V::String, F::empty(), NoStorage, val!(str "vertical")),
    b!("end", V::F32, F::ASSIGNABLE_AI, Tagged, val!(f32 0.0)),
    b!("fill_gradient", V::Gradient, F::ASSIGNABLE_A, FillGradient, val!(gradient)),
    b!("fill_opacity", V::F32, F::ASSIGNABLE_AI, FillOpacity, val!(f32 1.0)),
    b!(
        "font_family",
        V::String,
        F::ASSIGNABLE_A,
        FontFamily,
        val!(str crate::renderer::text::DEFAULT_FONT_FAMILY)
    ),
    b!(
        "font_size",
        V::F32,
        F::ASSIGNABLE_A,
        FontSize,
        val!(
            caps | caps | {
                match caps.text {
                    Some(crate::timeline::actor_caps::TextKind::Text)
                    | Some(crate::timeline::actor_caps::TextKind::Counter) => {
                        super::property_engine::PropertyValue::F32(48.0)
                    },
                    Some(crate::timeline::actor_caps::TextKind::Typst) => {
                        super::property_engine::PropertyValue::F32(36.0)
                    },
                    _ => super::property_engine::PropertyValue::F32(24.0),
                }
            }
        )
    ),
    b!("font_style", V::String, F::ASSIGNABLE_A, FontStyle, val!(str "normal")),
    b!("font_weight", V::F32, F::ASSIGNABLE_A, FontWeight, val!(f32 400.0)),
    b!("from", V::Vec2, F::SPATIAL_PATH_AI, LineFrom, val!(vec2 0.0, 0.0)),
    b!("func", V::BuildTimeOnly, F::empty(), PlotDomainGroup, val!(str "")),
    b!("gap", V::F32, F::empty(), ContainerLayoutGroup, val!(f32 0.0)),
    b!("gradient_extend", V::String, F::ASSIGNABLE_A, GradientExtend, val!(str "pad")),
    b!("gradient_space", V::String, F::ASSIGNABLE_A, GradientSpace, val!(str "oklab")),
    b!("grid", V::String, F::empty(), PlotDomainGroup, val!(str "auto")),
    b!("head_size", V::F32, F::PATH_AI, HeadSize, val!(f32 10.0)),
    b!(
        "height",
        V::F32,
        F::ANIMATED_I.union(F::SPATIAL).union(F::PATH_AFFECTING),
        Size,
        val!(f32 100.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 1,
            scale: 2.0
        }
    ),
    b!(
        "highlight_color",
        V::Color,
        F::ANIMATED,
        HighlightColor,
        val!(vec4 0.3, 0.5, 1.0, 1.0)
    ),
    b!("highlight_opacity", V::F32, F::ANIMATED, HighlightOpacity, val!(f32 0.0)),
    b!("highlight_padding", V::F32, F::ANIMATED, HighlightPadding, val!(f32 4.0)),
    b!("highlight_radius", V::F32, F::ANIMATED, HighlightRadius, val!(f32 3.0)),
    b!("icon", V::BuildTimeOnly, F::empty(), NoStorage, val!(str "")),
    b!("idle", V::String, F::ASSIGNABLE_AI, Tagged, val!(str "")),
    b!(
        "inactive_color",
        V::Color,
        F::ASSIGNABLE_AI,
        Tagged,
        val!(color 0.6, 0.6, 0.6, 0.8)
    ),
    b!("kind", V::String, F::empty(), PlotDomainGroup, val!(str "cartesian")),
    b!("label", V::String, F::ASSIGNABLE_A, TextContent, val!(str "")),
    b!("label_at", V::Vec2, F::ASSIGNABLE_AI, LabelAt, val!(vec2 0.0, 0.0)),
    b!(
        "label_color",
        V::Color,
        F::ASSIGNABLE_A,
        Tagged("legend_label_color"),
        val!(color 1.0, 1.0, 1.0, 1.0)
    ),
    b!("language", V::String, F::ASSIGNABLE_A, Language, val!(str "")),
    b!("legend", V::Sum(LEGEND_SUM_VARIANTS), F::ASSIGNABLE_A, Tagged, val!(bool true)),
    b!("letter_spacing", V::F32, F::ASSIGNABLE_A, LetterSpacing, val!(f32 0.0)),
    b!("levels", V::Vec2, F::empty(), PlotDomainGroup, val!(vec2 0.0, 1.0)),
    b!("line_cap", V::U32, F::ASSIGNABLE_AI, LineCap, val!(u32 0)),
    b!("line_height", V::F32, F::ASSIGNABLE_A, LineHeight, val!(f32 1.2)),
    b!("line_join", V::U32, F::ASSIGNABLE_AI, LineJoin, val!(u32 0)),
    b!("max_depth", V::F32, F::empty(), PlotDomainGroup, val!(f32 12.0)),
    b!("max_height", V::F32, F::SPATIAL_AI, MaxHeight, val!(f32 f32::INFINITY)),
    b!("max_lines", V::U32, F::ASSIGNABLE_AI, Tagged, val!(u32 2)),
    b!("max_value", V::F32, F::empty(), NoStorage, val!(f32 0.0)),
    b!("max_width", V::F32, F::ASSIGNABLE_A, TextMaxWidth, val!(f32 0.0)),
    b!("min_height", V::F32, F::SPATIAL_AI, MinHeight, val!(f32 0.0)),
    b!("min_width", V::F32, F::SPATIAL_AI, MinWidth, val!(f32 0.0)),
    b!(
        "offset",
        V::Vec2,
        F::SPATIAL_AI,
        PositionBindingGroup,
        val!(vec2 0.0, 0.0),
        ReadSource::None_
    ),
    b!("opacity", V::F32, F::PRESENCE_AI, Opacity, val!(f32 1.0)),
    b!("overflow", V::String, F::ASSIGNABLE_A, Overflow, val!(str "visible")),
    b!("padding", V::F32, F::empty(), ContainerLayoutGroup, val!(f32 0.0)),
    b!("parallax", V::F32, F::empty(), NoStorage, val!(f32 1.0)),
    b!(
        "perspective",
        V::F32,
        F::ASSIGNABLE_AI.union(F::SPATIAL),
        Tagged,
        val!(f32 800.0)
    ),
    b!(
        "place",
        V::Enum(&["auto", "top", "bottom", "left", "right", "above", "below"]),
        F::ASSIGNABLE,
        Tagged("callout_place"),
        val!(enum "right")
    ),
    b!(
        "points",
        V::PointList,
        F::ASSIGNABLE_A.union(F::SPATIAL).union(F::PATH_AFFECTING),
        Points,
        val!(point_list)
    ),
    b!("pop_scale", V::F32, F::ASSIGNABLE_AI, Tagged, val!(f32 1.15)),
    b!("position", V::Vec2, F::SPATIAL_AI, Position, val!(vec2 0.0, 0.0)),
    b!("prefix", V::String, F::ASSIGNABLE_AI, Tagged, val!(str "")),
    b!(
        "radius_x",
        V::F32,
        F::SPATIAL_PATH_AI,
        Size,
        val!(f32 50.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 0,
            scale: 1.0
        }
    ),
    b!(
        "radius_y",
        V::F32,
        F::SPATIAL_PATH_AI,
        Size,
        val!(f32 50.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 1,
            scale: 1.0
        }
    ),
    b!("resolution", V::F32, F::empty(), PlotDomainGroup, val!(f32 48.0)),
    b!("rotate_x", V::F32, F::ASSIGNABLE_AI.union(F::SPATIAL), Tagged, val!(f32 0.0)),
    b!("rotate_y", V::F32, F::ASSIGNABLE_AI.union(F::SPATIAL), Tagged, val!(f32 0.0)),
    b!("rotation", V::F32, F::SPATIAL_AI, Rotation, val!(f32 0.0)),
    b!("routing", V::String, F::ASSIGNABLE_AI, Tagged, val!(str "elbow")),
    b!("scale", V::F32, F::SPATIAL_AI, Scale, val!(f32 1.0)),
    b!("shadow", V::Vec4, F::ASSIGNABLE_AI, Tagged, val!(vec4 0.0, 4.0, 12.0, 0.0)),
    b!(
        "shadow_color",
        V::Color,
        F::ASSIGNABLE_AI,
        Tagged,
        val!(color 0.0, 0.0, 0.0, 0.25)
    ),
    b!("shared_id", V::String, F::ASSIGNABLE, Tagged, val!(str "")),
    b!("shift", V::Vec2, F::SPATIAL_AI, MotionOffset, val!(vec2 0.0, 0.0)),
    b!("show_axis", V::BuildTimeOnly, F::empty(), NoStorage, val!(str "true")),
    b!("show_labels", V::BuildTimeOnly, F::empty(), NoStorage, val!(str "true")),
    b!(
        "size",
        V::Vec2,
        F::ALL.union(F::SPATIAL).union(F::PATH_AFFECTING),
        Size,
        val!(vec2 50.0, 50.0)
    ),
    b!("solo", V::Bool, F::ASSIGNABLE, Tagged, val!(bool false)),
    b!("source", V::String, F::ASSIGNABLE, AudioSource, val!(str "")),
    b!("split_by", V::String, F::ASSIGNABLE_A, Tagged, val!(str "char")),
    b!("split_mask", V::Bool, F::ASSIGNABLE_A, Tagged, val!(bool false)),
    b!("split_offset_y", V::F32, F::ASSIGNABLE_A, Tagged, val!(f32 0.0)),
    b!("split_stagger", V::F32, F::ASSIGNABLE_A, Tagged, val!(f32 0.0)),
    b!("standoff", V::F32, F::ASSIGNABLE_AI, CalloutStandoff, val!(f32 40.0)),
    b!("start", V::F32, F::ASSIGNABLE_AI, Tagged, val!(f32 0.0)),
    b!(
        "stroke",
        V::Color,
        F::ASSIGNABLE_AI,
        StrokeColor,
        val!(color 1.0, 1.0, 1.0, 1.0)
    ),
    b!("stroke_gradient", V::Gradient, F::ASSIGNABLE_A, StrokeGradient, val!(gradient)),
    b!("stroke_progress", V::F32, F::ASSIGNABLE_AI, StrokeProgress, val!(f32 1.0)),
    b!("stroke_width", V::F32, F::ASSIGNABLE_AI, StrokeWidth, val!(f32 1.0)),
    b!("suffix", V::String, F::ASSIGNABLE_AI, Tagged, val!(str "")),
    b!(
        "swatch_size",
        V::F32,
        F::ASSIGNABLE_A,
        Tagged("legend_swatch_size"),
        val!(f32 16.0)
    ),
    b!("t_domain", V::Vec2, F::empty(), PlotDomainGroup, val!(vec2 0.0, 1.0)),
    b!("target", V::BuildTimeOnly, F::ASSIGNABLE, CalloutTarget, val!(str "")),
    b!("text", V::String, F::ASSIGNABLE_A, TextContent, val!(str "")),
    b!("text_align", V::String, F::ASSIGNABLE_A, TextAlign, val!(str "left")),
    b!("text_anchor", V::String, F::ASSIGNABLE_A, TextAnchor, val!(str "center")),
    b!(
        "text_max_width",
        V::F32,
        F::ASSIGNABLE_A,
        Tagged("legend_text_max_width"),
        val!(f32 240.0)
    ),
    b!("tick_labels", V::String, F::empty(), PlotDomainGroup, val!(str "auto")),
    b!("ticks", V::String, F::empty(), PlotDomainGroup, val!(str "auto")),
    b!("title", V::String, F::ASSIGNABLE_A, Tagged("legend_title"), val!(str "")),
    b!("to", V::Vec2, F::SPATIAL_PATH_AI, LineTo, val!(vec2 100.0, 0.0)),
    b!("to_offset", V::Vec2, F::ASSIGNABLE_AI, CalloutToOffset, val!(vec2 0.0, 0.0)),
    b!("tolerance", V::F32, F::empty(), PlotDomainGroup, val!(f32 2.0)),
    b!("transcript", V::String, F::ASSIGNABLE, Tagged, val!(str "")),
    b!("transform", V::Transform, F::SPATIAL_AI, Transform, val!(transform)),
    b!("transition_time", V::F32, F::ASSIGNABLE_AI, Tagged, val!(f32 0.0)),
    b!("url", V::String, F::ASSIGNABLE, ImageData, val!(str "")),
    b!("value", V::F32, F::ASSIGNABLE_AI, Tagged, val!(f32 0.0)),
    b!("vertical_align", V::String, F::empty(), NoStorage, val!(str "center")),
    b!("volume", V::F32, F::ASSIGNABLE_AI, AudioVolume, val!(f32 1.0)),
    b!(
        "width",
        V::F32,
        F::ANIMATED_I.union(F::SPATIAL).union(F::PATH_AFFECTING),
        Size,
        val!(f32 100.0),
        ReadSource::Component {
            field: ActorField::Size,
            index: 0,
            scale: 2.0
        }
    ),
    b!("word_spacing", V::F32, F::ASSIGNABLE_A, WordSpacing, val!(f32 0.0)),
    b!("x_domain", V::Vec2, F::empty(), PlotDomainGroup, val!(vec2 - 5.0, 5.0)),
    b!("x_range", V::Vec2, F::empty(), PlotDomainGroup, val!(vec2 - 10.0, 10.0)),
    b!("x_scale", V::String, F::empty(), NoStorage, val!(str "linear")),
    b!("y_domain", V::Vec2, F::empty(), PlotDomainGroup, val!(vec2 - 5.0, 5.0)),
    b!("y_range", V::Vec2, F::empty(), PlotDomainGroup, val!(vec2 - 10.0, 10.0)),
    b!("y_scale", V::String, F::empty(), NoStorage, val!(str "linear")),
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
mod lookup;
mod types;

pub use lookup::*;
pub use types::*;
