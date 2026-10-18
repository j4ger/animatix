//! Built-in property descriptors: the single declaration of the property list.
//!
//! Both sides of the language derive from this table. The parser builds its
//! `PropertySpec`s from it, adding only the type-system view of each property
//! (which lives in `animatix-syntax` alongside the rest of the type model), and
//! the engine joins it with per-property runtime bindings (storage field,
//! flags, default, read source). Adding a property is therefore one row here,
//! one type row in the parser, and one binding in the engine — and a missing or
//! mismatched step fails a test instead of drifting silently, which is how two
//! hand-sorted tables behaved before.

use crate::caps::{Applicable, ShapeKind};

/// The reserved assignment target that addresses the scene camera.
///
/// `camera` is not an actor: `camera.at` / `camera.zoom` / `camera.rotation`
/// write a scene-wide transform. It lives here because both the parser's
/// label check and the engine's assignment routing need the same name, and
/// `animatix-core` is the only crate below both of them.
pub const CAMERA_TARGET: &str = "camera";

/// Stable property identifier used by runtime plans and schema consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PropertyId(pub u32);

/// Finite value kind understood by extension property tracks.
///
/// This mirrors the runtime dynamic-track storage while staying free of
/// renderer/runtime dependencies so the parser, analyzer, and LSP can consume
/// it too. It is deliberately coarser than the engine's `ValueType` (which adds
/// the shape/placement/anchor kinds) and than the parser's `Type` (which adds
/// the expression-system types); both sides map onto it, and both mappings are
/// pinned as *total* functions by tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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
    /// String.
    String,
    /// List of 2D points.
    PointList,
    /// Any finite property value.
    Generic,
}

/// One built-in property: canonical name, where it applies, and the value kind
/// its runtime track stores.
pub struct PropertyDescriptor {
    /// Canonical source-text name.
    pub name: &'static str,
    /// Actor types (or shape kinds) the property applies to.
    pub applicable: Applicable,
    /// Finite value kind stored by dynamic property tracks.
    pub value_kind: PropertyValueKind,
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
        }
    }
}

/// Declarative macro for defining canonical built-in property descriptor tables.
///
/// Ensures descriptor name, applicability, and value kind are declared together
/// with single-point maintenance.
#[macro_export]
macro_rules! declare_property_descriptors {
    ($(
        $(#[$doc:meta])*
        $name:literal => ($applicable:expr, $val_kind:ident)
    ),* $(,)?) => {
        /// Every built-in property, in declaration order.
        ///
        /// **The order is the ABI.** [`PropertyId`] is the row index and serialized
        /// plans (`PropertyPlan` slots, `persistence.rs`) store those ids, so appending
        /// a row is safe but reordering or removing one invalidates saved files. The
        /// `property_id_order_is_pinned` test guards it.
        pub static PROPERTY_DESCRIPTORS: &[$crate::property::PropertyDescriptor] = &[
            $(
                $crate::property::PropertyDescriptor::new(
                    $name,
                    $applicable,
                    $crate::property::PropertyValueKind::$val_kind,
                ),
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
    "bar_width" => (Applicable::Actors(&["BarChart"]), F32),
    "baseline" => (Applicable::Never, F32),
    "bounds" => (Applicable::Actors(&["Filter"]), Vec4),
    "char_progress" => (Applicable::TextLike, F32),
    "code" => (Applicable::Actors(&["Code"]), String),
    // Not `Everything`: `Glass`, `Filter` and `Mask` draw no surface of their
    // own, so a scope-level `color:` had nowhere to go — the value was dropped in
    // silence, which is what the "never silently drop" rule is for. Verified
    // against the primitives: none of the three reads a style colour.
    "color" => (Applicable::AllDrawables, Vec4),
    "cols" => (Applicable::Actors(&["Grid"]), U32),
    "commands" => (Applicable::Actors(&["Path"]), Generic),
    "data" => (Applicable::Actors(&["BarChart"]), Generic),
    "density" => (Applicable::Actors(&["VectorField"]), F32),
    "descent" => (Applicable::Never, F32),
    "direction" => (Applicable::Actors(&["BarChart"]), String),
    "fill_opacity" => (Applicable::AllShapesExceptLine, F32),
    "font_family" => (Applicable::TextLike, String),
    "font_size" => (// `Legend` draws its own labels through the text engine and reads
        // `font_size` off its props (`primitives/legend.rs`), so the row has to
        // include it or the applicability lint reports a working property as
        // dropped.
        Applicable::Actors(&[
            "Captions", "Code", "Cue", "Equation", "Legend", "Math", "Text", "Typst",
        ]), F32),
    "font_style" => (Applicable::TextLike, String),
    "font_weight" => (Applicable::TextLike, F32),
    "from" => (Applicable::Actors(&["Arrow", "Callout", "Connector", "Line"]), Vec2),
    "func" => (Applicable::Actors(&["ContourSet", "Heatmap", "PlotCurve", "VectorField"]), Generic),
    "gap" => (// `BarChart` reads the same name as the bar spacing; for containers it is
        // the child gap. One row, two consumers — see `build/plot.rs`.
        Applicable::Actors(&[
            "BarChart", "Col", "Grid", "Group", "Legend", "Mask", "Row", "Stack",
        ]), F32),
    "grid" => (Applicable::PlotHost, String),
    "head_size" => (Applicable::Any(&[
            Applicable::ShapeKinds(&[ShapeKind::Arrow]),
            Applicable::Callout,
        ]), F32),
    "height" => (Applicable::SizedActors, F32),
    "highlight_color" => (Applicable::Actors(&["Equation", "Fragment"]), Vec4),
    "highlight_opacity" => (Applicable::Actors(&["Equation", "Fragment"]), F32),
    "highlight_padding" => (Applicable::Actors(&["Equation", "Fragment"]), F32),
    "highlight_radius" => (Applicable::Actors(&["Equation", "Fragment"]), F32),
    "kind" => (Applicable::Actors(&["PlotCurve"]), String),
    "label" => (Applicable::Callout, String),
    "label_at" => (Applicable::Callout, Vec2),
    "label_color" => (Applicable::LegendHost, Vec4),
    "legend" => (Applicable::Everything, Generic),
    "letter_spacing" => (Applicable::Actors(&[
            "Code", "Col", "Grid", "Math", "Row", "Stack", "Text", "Typst",
        ]), F32),
    "levels" => (Applicable::Actors(&["ContourSet"]), Vec2),
    "line_cap" => (Applicable::AllShapes, U32),
    "line_height" => (Applicable::TextLike, F32),
    "line_join" => (Applicable::AllShapes, U32),
    "max_depth" => (Applicable::Actors(&["ContourSet", "PlotCurve"]), F32),
    "max_height" => (Applicable::SizedActors, F32),
    "max_value" => (Applicable::Actors(&["BarChart"]), F32),
    "max_width" => (Applicable::Actors(&[
            "Code", "Col", "Grid", "Math", "Row", "Stack", "Text", "Typst",
        ]), F32),
    "min_height" => (Applicable::SizedActors, F32),
    "min_width" => (Applicable::SizedActors, F32),
    "offset" => (Applicable::Everything, Vec2),
    "opacity" => (Applicable::Everything, F32),
    "overflow" => (Applicable::TextLike, String),
    "padding" => (Applicable::Actors(&["Col", "Graph", "Grid", "Row", "Stack"]), F32),
    "place" => (Applicable::Callout, Generic),
    "points" => (Applicable::Actors(&["Polygon"]), PointList),
    "position" => (Applicable::Everything, Vec2),
    "radius_x" => (Applicable::Actors(&["Ellipse"]), F32),
    "radius_y" => (Applicable::Actors(&["Ellipse"]), F32),
    "resolution" => (Applicable::Actors(&["ContourSet", "Heatmap", "PlotCurve"]), F32),
    "rotation" => (Applicable::Everything, F32),
    "scale" => (Applicable::Everything, F32),
    "shift" => (Applicable::Everything, Vec2),
    "show_axis" => (Applicable::Actors(&["BarChart"]), Generic),
    "show_labels" => (Applicable::Actors(&["BarChart"]), Generic),
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
    // The uniform body property for every text-like actor (`Text`, `Typst`,
    // `Code`, `Math`); `code` remains the `Code` spelling. It used to list only
    // Text and Math, which left `Typst` with no body property the analyzer
    // recognised once the redundant `content` alias was removed.
    "text" => (Applicable::TextLike, String),
    "text_align" => (Applicable::TextLike, String),
    "text_max_width" => (// `Legend` has its own legend label wrapping, and the text engine reads
        // this as the canonical wrap width (`declarations_text.rs`: "Canonical
        // name per spec; `max_width` kept as a legacy alias"). The row used to
        // list only `Legend`, which is why 94 site scenes could set a wrap width
        // on a `Text` that the table claimed did not take one.
        Applicable::Any(&[Applicable::LegendHost, Applicable::TextLike]), F32),
    "tick_labels" => (Applicable::PlotHost, String),
    "ticks" => (Applicable::PlotHost, String),
    "title" => (Applicable::LegendHost, String),
    "to" => (Applicable::Actors(&["Arrow", "Callout", "Connector", "Line"]), Vec2),
    "to_offset" => (Applicable::Callout, Vec2),
    "tolerance" => (Applicable::Actors(&["PlotCurve"]), F32),
    "transform" => (Applicable::Everything, Generic),
    "url" => (Applicable::Actors(&["Image", "Svg"]), String),
    "vertical_align" => (Applicable::Actors(&["Col", "Row"]), String),
    "volume" => (Applicable::Actors(&["Audio"]), F32),
    "width" => (Applicable::SizedActors, F32),
    "word_spacing" => (Applicable::TextLike, F32),
    "x_domain" => (Applicable::PlotGeometry, Vec2),
    "x_range" => (Applicable::Actors(&["Graph", "NumberPlane", "PlotCurve"]), Vec2),
    "x_scale" => (Applicable::PlotHost, String),
    "y_domain" => (Applicable::PlotGeometry, Vec2),
    "y_range" => (Applicable::Actors(&["Graph", "NumberPlane", "PlotCurve"]), Vec2),
    "y_scale" => (Applicable::PlotHost, String),
    // Appended last on purpose: `PropertyId` is the row index, so new rows go
    // at the end rather than into name order.
    // `Glass` paints no surface, but its radius is not decorative: it is the
    // rounded clip the backdrop is blitted through.
    "corner_radius" => (Applicable::Actors(&["Rect", "Glass", "Connector"]), F32),
    "language" => (Applicable::Actors(&["Code"]), String),
    "dash_offset" => (Applicable::AllStrokePaths, F32),
    "dash_pattern" => (Applicable::AllStrokePaths, Generic),
    "blend" => (Applicable::Everything, String),
    "fill_gradient" => (Applicable::Any(&[Applicable::AllShapesExceptLine, Applicable::TextLike]), Generic),
    "stroke_gradient" => (Applicable::AllStrokePaths, Generic),
    "gradient_extend" => (Applicable::Any(&[Applicable::AllShapes, Applicable::AllStrokePaths]), String),
    "gradient_space" => (Applicable::Any(&[Applicable::AllShapes, Applicable::AllStrokePaths]), String),
    // Bundled stroke-icon name (`icon: "check"`). Build-time only: the engine
    // expands the name into `Path` command geometry and stores nothing on a
    // track, so it is declared `Generic` to match its `BuildTimeOnly` runtime
    // binding (mirrors how `data`/`bar_width` are handled).
    "icon" => (Applicable::Actors(&["Path"]), Generic),
    // The scene camera's per-actor opt-out. `Applicable::Everything` because the
    // camera wraps every root subtree, so any actor may need to sit outside it
    // (a HUD, a watermark, a caption pinned to the viewport). Declared `Generic`
    // to match its `BuildTimeOnly` runtime binding, exactly as `icon` does: the
    // value is read once at build into a plain track field and never keyframed.
    "camera_follow" => (Applicable::Everything, Generic),
    // Continuous parallax multiplier for camera movement on root actors (1.0 = normal, 0.0 =
    // static HUD).
    "parallax" => (Applicable::Everything, F32),
    // Connector routing algorithm ("elbow", "l-bend", "straight").
    "routing" => (Applicable::Actors(&["Connector"]), String),
    // Whether a connector draws an arrowhead at its destination.
    "arrow" => (Applicable::Actors(&["Connector"]), Bool),
    // Split-text reveal granularity: "char" (default), "word", or "line".
    "split_by" => (Applicable::TextLike, String),
    // Split-text stagger between units as a duration fraction (0.0–1.0).
    "split_stagger" => (Applicable::TextLike, F32),
    // Vertical offset in scene points from which split text units emerge.
    "split_offset_y" => (Applicable::TextLike, F32),
    // Whether split text units are clipped at the line baseline.
    "split_mask" => (Applicable::TextLike, Bool),
    // Rolling counter target numeric value.
    "value" => (Applicable::Actors(&["Counter"]), F32),
    // Optional prefix text prepended to the rolling counter.
    "prefix" => (Applicable::Actors(&["Counter"]), String),
    // Optional suffix text appended to the rolling counter.
    "suffix" => (Applicable::Actors(&["Counter"]), String),
    // Number of decimal places to display in the rolling counter.
    "decimals" => (Applicable::Actors(&["Counter"]), U32),
    // Whether to insert thousands grouping commas in the rolling counter.
    "comma" => (Applicable::Actors(&["Counter"]), Bool),
    // Declarative idle ambient motion ("float" or "breath").
    "idle" => (Applicable::Everything, String),
    // Instanced Gaussian shadow [offset_x, offset_y, blur, spread].
    "shadow" => (Applicable::AllShapesExceptLine, Vec4),
    // Shadow color in RGBA.
    "shadow_color" => (Applicable::AllShapesExceptLine, Vec4),
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
    "active_color" => (Applicable::Actors(&["Captions", "Cue"]), Vec4),
    // Color for inactive or dimmed words in captions.
    "inactive_color" => (Applicable::Actors(&["Captions"]), Vec4),
    // Whether to render a backdrop pill/box behind captions.
    "backdrop" => (Applicable::Actors(&["Captions"]), Bool),
    // Color of the backdrop box behind captions.
    "backdrop_color" => (Applicable::Actors(&["Captions"]), Vec4),
    // Corner radius of the backdrop box.
    "backdrop_radius" => (Applicable::Actors(&["Captions"]), F32),
    // Padding (horizontal, vertical) around text inside the backdrop box.
    "backdrop_padding" => (Applicable::Actors(&["Captions"]), Vec2),
    // Scale multiplier for active word spring pop in "pop" caption mode.
    "pop_scale" => (Applicable::Actors(&["Captions"]), F32),
    // Maximum number of visible lines per caption cue before line-breaking.
    "max_lines" => (Applicable::Actors(&["Captions"]), U32),
    // Fade/slide transition duration for captions in seconds.
    "transition_time" => (Applicable::Actors(&["Captions"]), F32),
    // Path to speech transcript file (.json, .vtt, .srt) for Audio or Captions actor.
    "transcript" => (Applicable::Actors(&["Audio", "Captions"]), String),
    // Start time of a caption cue in seconds.
    "start" => (Applicable::Actors(&["Cue"]), F32),
    // End time of a caption cue in seconds.
    "end" => (Applicable::Actors(&["Cue"]), F32),
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
