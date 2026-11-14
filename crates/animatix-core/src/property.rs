//! Built-in property descriptors: the single declaration of the property list.
//!
//! The syntax crate derives authoring types from these rows, while the engine
//! joins them with runtime bindings. `AuthoredTypeHint` records the few source
//! types that cannot be inferred from the coarser `PropertyValueKind`.

use crate::caps::{Applicable, ShapeKind};

/// Reserved assignment target addressing the scene camera. `camera.at`/`.zoom`/
/// `.rotation` write a scene-wide transform; shared by the parser and engine,
/// which both sit above `animatix-core`.
pub const CAMERA_TARGET: &str = "camera";

/// Stable property identifier used by runtime plans and schema consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PropertyId(pub u32);

/// Finite value kind understood by extension property tracks.
///
/// Deliberately coarser than the engine's `ValueType` and the parser's `Type`,
/// which both map onto it; both mappings are pinned as total functions by tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PropertyValueKind {
    /// 32-bit float.
    F32,
    /// 32-bit unsigned integer.
    U32,
    /// Boolean flag.
    Bool,
    /// 2D vector.
    Vec2,
    /// 4D vector or color.
    Vec4,
    String,
    /// List of 2D points.
    PointList,
    /// Any finite property value.
    Generic,
}

/// Authoring-time expression type for a built-in property.
///
/// `PropertyValueKind` describes the coarser value stored by dynamic property
/// tracks. This hint describes the type accepted by the syntax type checker;
/// it intentionally does not narrow `Any` to an engine storage type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuthoredTypeHint {
    /// Derive the authoring type from [`PropertyValueKind`].
    Default,
    /// String expression.
    Str,
    /// Color expression.
    Color,
    /// Number or string expression.
    NumOrStr,
    /// Boolean or string expression.
    BoolOrStr,
    /// List of 2D vectors.
    Vec2List,
    /// List of numbers.
    NumList,
    /// 2D affine transform input forms.
    Transform,
    /// Four-number tuple.
    Vec4,
    /// Unconstrained expression type.
    Any,
}

/// One built-in property: canonical name, applicability, stored value kind, and
/// syntax authoring-type hint.
///
/// Adding `authored_type` means downstream struct literals must provide a hint;
/// prefer `PropertyDescriptor::new` to inherit the default, then call
/// `with_authored_type` for a source-language exception.
pub struct PropertyDescriptor {
    /// Canonical source-text name.
    pub name: &'static str,
    /// Actor types (or shape kinds) the property applies to.
    pub applicable: Applicable,
    /// Finite value kind stored by dynamic property tracks.
    pub value_kind: PropertyValueKind,
    /// Authoring-time type hint; `Default` derives from `value_kind`.
    pub authored_type: AuthoredTypeHint,
}

impl PropertyDescriptor {
    /// Const constructor for [`PROPERTY_DESCRIPTORS`].
    pub const fn new(
        name: &'static str,
        applicable: Applicable,
        value_kind: PropertyValueKind,
    ) -> Self {
        Self {
            name,
            applicable,
            value_kind,
            authored_type: AuthoredTypeHint::Default,
        }
    }

    /// Override the authoring-time type inferred from `value_kind`.
    pub const fn with_authored_type(mut self, authored_type: AuthoredTypeHint) -> Self {
        self.authored_type = authored_type;
        self
    }
}

/// Declarative macro defining canonical built-in property descriptor tables.
#[macro_export]
macro_rules! declare_property_descriptors {
    ($(
        $(#[$doc:meta])*
        $name:literal => ($applicable:expr, $val_kind:ident $(, $authored_type:ident)?)
    ),* $(,)?) => {
        /// Every built-in property, in declaration order. **The order is the ABI**:
        /// [`PropertyId`] is the row index and serialized plans store those ids, so
        /// append but never reorder or remove. `property_id_order_is_pinned` guards it.
        pub static PROPERTY_DESCRIPTORS: &[$crate::property::PropertyDescriptor] = &[
            $(
                $crate::property::PropertyDescriptor::new(
                    $name,
                    $applicable,
                    $crate::property::PropertyValueKind::$val_kind,
                ) $(.with_authored_type($crate::property::AuthoredTypeHint::$authored_type))?,
            )*
        ];
    };
}

declare_property_descriptors! {
    "align" => (Applicable::LayoutContainers, String),
    "anchor" => (Applicable::Everything, Generic),
    "ascent" => (Applicable::Never, F32),
    "at" => (Applicable::Everything, Vec2),
    "background_color" => (Applicable::Never, Vec4),
    "bar_colors" => (Applicable::Actors(&["BarChart"]), Generic),
    "bar_width" => (Applicable::Actors(&["BarChart"]), F32, NumOrStr),
    "baseline" => (Applicable::Never, F32),
    "bounds" => (Applicable::Actors(&["Filter"]), Vec4, Vec4),
    "char_progress" => (Applicable::TextLike, F32),
    "code" => (Applicable::Actors(&["Code"]), String),
    // Not `Everything`: `Glass`, `Filter` and `Mask` draw no surface of their own,
    // so a scope-level `color:` would be silently dropped.
    "color" => (Applicable::AllDrawables, Vec4),
    "cols" => (Applicable::Actors(&["Grid"]), U32),
    "commands" => (Applicable::ShapeKinds(&[ShapeKind::Path]), Generic),
    "data" => (Applicable::Actors(&["BarChart"]), Generic),
    "density" => (Applicable::Actors(&["VectorField"]), F32),
    "descent" => (Applicable::Never, F32),
    "direction" => (Applicable::Actors(&["BarChart"]), String),
    "fill_opacity" => (Applicable::AllShapesExceptLine, F32),
    "font_family" => (Applicable::Any(&[Applicable::TextLike, Applicable::Actors(&["Captions"])]), String),
    "font_size" => (// `Legend` and `Counter` render through the text path and read
        // `font_size`, so the row must list them or the applicability lint warns.
        Applicable::Actors(&[
            "Captions", "Code", "Counter", "Cue", "Equation", "Legend", "Math", "Text", "Typst",
        ]), F32),
    "font_style" => (Applicable::Any(&[Applicable::TextLike, Applicable::Actors(&["Captions"])]), String),
    "font_weight" => (Applicable::Any(&[Applicable::TextLike, Applicable::Actors(&["Captions"])]), F32, NumOrStr),
    "from" => (Applicable::Actors(&["Arrow", "Callout", "Connector", "Line"]), Vec2),
    "func" => (Applicable::Actors(&["ContourSet", "Heatmap", "PlotCurve", "VectorField"]), Generic),
    "gap" => (// `BarChart` bar spacing and container child gap: one row, two consumers.
        Applicable::Actors(&[
            "BarChart", "Col", "Grid", "Group", "Legend", "Mask", "Row", "Stack",
        ]), F32, NumOrStr),
    "grid" => (Applicable::PlotHost, Bool),
    "head_size" => (Applicable::Any(&[
            Applicable::ShapeKinds(&[ShapeKind::Arrow]),
            Applicable::Callout,
        ]), F32),
    "height" => (Applicable::SizedActors, F32),
    "highlight_color" => (Applicable::Actors(&["Equation", "Fragment", "Typst"]), Vec4),
    "highlight_opacity" => (Applicable::Actors(&["Equation", "Fragment", "Typst"]), F32),
    "highlight_padding" => (Applicable::Actors(&["Equation", "Fragment", "Typst"]), F32),
    "highlight_radius" => (Applicable::Actors(&["Equation", "Fragment", "Typst"]), F32),
    "kind" => (Applicable::Actors(&["PlotCurve"]), String),
    "label" => (Applicable::Callout, String),
    "label_at" => (Applicable::Callout, Vec2),
    "label_color" => (Applicable::LegendHost, Vec4),
    "legend" => (Applicable::Everything, Generic, BoolOrStr),
    // Text-engine only; the layout containers do not read it.
    "letter_spacing" => (Applicable::Actors(&[
            "Captions", "Code", "Math", "Text", "Typst",
        ]), F32),
    "levels" => (Applicable::Actors(&["ContourSet"]), Vec2, Any),
    "line_cap" => (Applicable::AllShapes, U32),
    "line_height" => (Applicable::Any(&[Applicable::TextLike, Applicable::Actors(&["Captions"])]), F32),
    "line_join" => (Applicable::AllShapes, U32),
    "max_depth" => (Applicable::Actors(&["ContourSet", "PlotCurve"]), F32),
    "max_height" => (Applicable::SizedActors, F32),
    "max_value" => (Applicable::Actors(&["BarChart"]), F32, NumOrStr),
    // Text-engine only, like `letter_spacing`: no container primitive reads it.
    "max_width" => (Applicable::Actors(&[
            "Code", "Math", "Text", "Typst",
        ]), F32),
    "min_height" => (Applicable::SizedActors, F32),
    "min_width" => (Applicable::SizedActors, F32),
    "offset" => (Applicable::Everything, Vec2),
    "opacity" => (Applicable::Everything, F32),
    "overflow" => (Applicable::TextLike, String),
    "padding" => (Applicable::Actors(&["Col", "Graph", "Grid", "Row", "Stack"]), F32),
    "place" => (Applicable::Callout, Generic, Str),
    "points" => (Applicable::ShapeKinds(&[ShapeKind::Polygon]), PointList, Vec2List),
    "position" => (Applicable::Everything, Vec2),
    "radius_x" => (Applicable::ShapeKinds(&[ShapeKind::Ellipse]), F32),
    "radius_y" => (Applicable::ShapeKinds(&[ShapeKind::Ellipse]), F32),
    "resolution" => (Applicable::Actors(&["ContourSet", "Heatmap", "PlotCurve"]), F32),
    "rotation" => (Applicable::Everything, F32),
    "scale" => (Applicable::Everything, F32),
    "shift" => (Applicable::Everything, Vec2),
    "show_axis" => (Applicable::Actors(&["BarChart"]), Generic, BoolOrStr),
    "show_labels" => (Applicable::Actors(&["BarChart"]), Generic, BoolOrStr),
    "size" => (Applicable::ExceptTextLike, Vec2),
    // Authored solo flag: while any actor declares `solo: true`, every
    // non-solo subtree is hidden (recursively) in preview and export alike.
    "solo" => (Applicable::Everything, Bool),
    "source" => (Applicable::Actors(&["Audio"]), String),
    "standoff" => (Applicable::Callout, F32),
    "stroke" => (Applicable::AllStrokePaths, Vec4),
    "stroke_progress" => (Applicable::AllStrokePaths, F32),
    "stroke_width" => (Applicable::AllStrokePaths, F32),
    "swatch_size" => (Applicable::LegendHost, F32),
    "t_domain" => (Applicable::Actors(&["PlotCurve"]), Vec2),
    "target" => (Applicable::Callout, Generic),
    // Uniform body property for every text-like actor; `code` is the `Code` spelling.
    "text" => (Applicable::TextLike, String),
    "text_align" => (Applicable::TextLike, String),
    // Canonical text wrap width (`max_width` is a legacy alias); every host that
    // can set one must be listed.
    "text_max_width" => (
        Applicable::Any(&[
            Applicable::LegendHost,
            Applicable::TextLike,
            Applicable::Actors(&["Captions"]),
        ]), F32),
    "tick_labels" => (Applicable::PlotHost, String),
    "ticks" => (Applicable::PlotHost, Bool),
    "title" => (Applicable::LegendHost, String),
    "to" => (Applicable::Actors(&["Arrow", "Callout", "Connector", "Line"]), Vec2),
    "to_offset" => (Applicable::Callout, Vec2),
    "tolerance" => (Applicable::Actors(&["PlotCurve"]), F32),
    "transform" => (Applicable::Everything, Generic, Transform),
    "url" => (Applicable::Actors(&["Image", "Svg"]), String),
    "vertical_align" => (Applicable::Actors(&["Col", "Row"]), String),
    "volume" => (Applicable::Actors(&["Audio"]), F32),
    "width" => (Applicable::SizedActors, F32),
    "word_spacing" => (Applicable::Any(&[Applicable::TextLike, Applicable::Actors(&["Captions"])]), F32),
    "x_domain" => (Applicable::PlotGeometry, Vec2),
    "x_range" => (Applicable::Actors(&["Graph", "NumberPlane", "PlotCurve"]), Vec2, Any),
    "x_scale" => (Applicable::PlotHost, String),
    "y_domain" => (Applicable::PlotGeometry, Vec2),
    "y_range" => (Applicable::Actors(&["Graph", "NumberPlane", "PlotCurve"]), Vec2, Any),
    "y_scale" => (Applicable::PlotHost, String),
    // Appended last on purpose: `PropertyId` is the row index. `Glass` has no
    // surface, but its radius is the rounded clip the backdrop is blitted through.
    "corner_radius" => (Applicable::Actors(&["Rect", "Glass", "Connector"]), F32),
    "language" => (Applicable::Actors(&["Code"]), String),
    "dash_offset" => (Applicable::AllStrokePaths, F32),
    "dash_pattern" => (Applicable::AllStrokePaths, Generic, NumList),
    "blend" => (Applicable::Everything, String),
    "fill_gradient" => (Applicable::Any(&[Applicable::AllShapesExceptLine, Applicable::TextLike]), Generic),
    "stroke_gradient" => (Applicable::AllStrokePaths, Generic),
    "gradient_extend" => (Applicable::Any(&[Applicable::AllShapes, Applicable::AllStrokePaths]), String),
    "gradient_space" => (Applicable::Any(&[Applicable::AllShapes, Applicable::AllStrokePaths]), String),
    // Bundled stroke-icon name, expanded to `Path` geometry at build time; declared
    // `Generic` to match its `BuildTimeOnly` binding (like `data`/`bar_width`).
    "icon" => (Applicable::Actors(&["Path"]), Generic, Str),
    // Per-actor camera opt-out: any root actor may need to sit outside the camera,
    // so `Everything`. `Generic` matches its `BuildTimeOnly` binding, like `icon`.
    "camera_follow" => (Applicable::Everything, Generic),
    // Parallax multiplier for camera movement on root actors (1.0 normal, 0.0 static).
    "parallax" => (Applicable::Everything, F32),
    // Connector routing algorithm ("elbow", "l-bend", "straight").
    "routing" => (Applicable::Actors(&["Connector"]), String),
    "arrow" => (Applicable::Actors(&["Connector"]), Bool),
    // Split-text reveal granularity: "char" (default), "word", or "line".
    "split_by" => (Applicable::TextLike, String),
    // Split-text stagger between units as a duration fraction (0.0–1.0).
    "split_stagger" => (Applicable::TextLike, F32),
    // Vertical offset in scene points from which split text units emerge.
    "split_offset_y" => (Applicable::TextLike, F32),
    "split_mask" => (Applicable::TextLike, Bool),
    "value" => (Applicable::Actors(&["Counter"]), F32),
    "prefix" => (Applicable::Actors(&["Counter"]), String),
    "suffix" => (Applicable::Actors(&["Counter"]), String),
    "decimals" => (Applicable::Actors(&["Counter"]), U32),
    "comma" => (Applicable::Actors(&["Counter"]), Bool),
    // Declarative idle ambient motion ("float" or "breath").
    "idle" => (Applicable::Everything, String),
    // Instanced Gaussian shadow [offset_x, offset_y, blur, spread].
    "shadow" => (Applicable::AllShapesExceptLine, Vec4, Vec4),
    // Shadow color in RGBA.
    "shadow_color" => (Applicable::AllShapesExceptLine, Vec4, Color),
    // 2.5D X-axis rotation angle in radians.
    "rotate_x" => (Applicable::Everything, F32),
    // 2.5D Y-axis rotation angle in radians.
    "rotate_y" => (Applicable::Everything, F32),
    // 2.5D perspective projection camera distance in points.
    "perspective" => (Applicable::Everything, F32),
    // Cross-scene shared element transition identifier.
    "shared_id" => (Applicable::Everything, String),
    // Display mode for captions ("block", "karaoke", "pop", "roll").
    "caption_mode" => (Applicable::Actors(&["Captions"]), String),
    // Highlight color for current active word in captions.
    "active_color" => (Applicable::Actors(&["Captions", "Cue"]), Vec4, Color),
    // Color for inactive or dimmed words in captions.
    "inactive_color" => (Applicable::Actors(&["Captions"]), Vec4, Color),
    "backdrop" => (Applicable::Actors(&["Captions"]), Bool),
    "backdrop_color" => (Applicable::Actors(&["Captions"]), Vec4, Color),
    "backdrop_radius" => (Applicable::Actors(&["Captions"]), F32),
    // Padding (horizontal, vertical) around text inside the backdrop box.
    "backdrop_padding" => (Applicable::Actors(&["Captions"]), Vec2),
    // Scale multiplier for active word spring pop in "pop" caption mode.
    "pop_scale" => (Applicable::Actors(&["Captions"]), F32),
    "max_lines" => (Applicable::Actors(&["Captions"]), U32),
    // Fade/slide transition duration for captions in seconds.
    "transition_time" => (Applicable::Actors(&["Captions"]), F32),
    // Path to speech transcript file (.json, .vtt, .srt) for Audio or Captions actor.
    "transcript" => (Applicable::Actors(&["Audio", "Captions"]), String),
    // Start time of a caption cue in seconds.
    "start" => (Applicable::Actors(&["Cue"]), F32),
    // End time of a caption cue in seconds.
    "end" => (Applicable::Actors(&["Cue"]), F32),
    // Horizontal text anchor placement relative to 'at' ("left", "center", "right").
    "text_anchor" => (Applicable::TextLike, String),
    // Layout item maximum width (distinct from `max_width`, the text wrap width).
    "max_item_width" => (Applicable::SizedActors, F32),
}

/// The descriptor for `name`, when it is a built-in property.
pub fn descriptor(name: &str) -> Option<&'static PropertyDescriptor> {
    PROPERTY_DESCRIPTORS.iter().find(|d| d.name == name)
}

/// The [`PropertyId`] for `name` — its index in [`PROPERTY_DESCRIPTORS`].
pub fn descriptor_id(name: &str) -> Option<PropertyId> {
    PROPERTY_DESCRIPTORS
        .iter()
        .position(|d| d.name == name)
        .map(|index| PropertyId(index as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    mod macro_test {
        use super::*;

        declare_property_descriptors! {
            "test_prop" => (Applicable::Everything, F32),
            "test_color" => (Applicable::AllDrawables, Vec4),
        }

        #[test]
        fn test_declare_property_descriptors_macro() {
            assert_eq!(PROPERTY_DESCRIPTORS.len(), 2);
            assert_eq!(PROPERTY_DESCRIPTORS[0].name, "test_prop");
            assert_eq!(PROPERTY_DESCRIPTORS[0].value_kind, PropertyValueKind::F32);
            assert_eq!(PROPERTY_DESCRIPTORS[1].name, "test_color");
            assert_eq!(PROPERTY_DESCRIPTORS[1].value_kind, PropertyValueKind::Vec4);
        }
    }
}
