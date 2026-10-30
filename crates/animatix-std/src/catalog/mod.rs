//! The built-in primitive catalog: one [`PrimitiveInfo`] identity card per
//! built-in actor type.
//!
//! This is the single author-visible source for primitive metadata — display
//! name, category, icon, capabilities, child processing, and the shape/text
//! projections. Each built-in is one `pub static` card below, assembled into
//! [`CATALOG`]; the engine's `BUILT_INS` rows reference these cards by symbol,
//! so metadata and behaviour cannot drift positionally. The parser crate
//! derives its contract tables from the catalog. Adding a built-in primitive
//! is one behaviour file in the engine plus one card here plus one `BUILT_INS`
//! registration row — no parallel declarations anywhere else.

use std::borrow::Cow;

use animatix_core::caps::{
    ActorCaps, ActorCategory, ChildProcessingKind, PlotKind, PrimitiveCapabilities, ShapeKind,
    TextKind,
};
use animatix_core::icon_glyphs;

/// The engine-visible identity card of one built-in primitive.
///
/// Built-ins borrow string literals (`Cow::Borrowed`, no allocation); an
/// extension primitive owns the names it received over FFI (`Cow::Owned`), so
/// registering one allocates nothing permanently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveInfo {
    /// Authored type name (`.amx`) — the registry key.
    pub type_name: Cow<'static, str>,
    /// Human-readable label for UI palettes and tooltips.
    pub display_name: Cow<'static, str>,
    /// UI category.
    pub category: ActorCategory,
    /// Opaque icon identifier (an [`animatix_core::icon_glyphs`] constant).
    pub icon_id: Cow<'static, str>,
    /// Whether shown in an "advanced" submenu instead of top-level.
    pub advanced: bool,
    /// Engine capabilities.
    pub capabilities: PrimitiveCapabilities,
    /// Child-processing strategy.
    pub child_processing: ChildProcessingKind,
    /// The concrete shape geometry, for shape primitives.
    pub shape: Option<ShapeKind>,
    /// The text engine backing this actor, for text-like primitives.
    pub text: Option<TextKind>,
    /// Which plot family this actor belongs to, for plot primitives. Carried to
    /// the engine through `PrimitiveFamilyDescriptor`, replacing the plot
    /// builder's `ty == "VectorField"`-style string compares.
    pub plot: Option<PlotKind>,
    /// Renders as a stroke-based path (shapes and `PlotCurve`).
    pub stroke_path: bool,
    /// The generic actor-declaration body writes this primitive's declaration
    /// properties into the connector geometry lanes (`from` / `to` / `routing` /
    /// `arrow`), so the primitive is deliberately *not* dispatched to
    /// [`crate::PrimitiveInfo`]'s `Primitive::build` path. Catalog data rather
    /// than a `ty == "Connector"` compare at each dispatch site.
    pub connector: bool,
    /// Author-facing documentation for completions, tooltips, and hover info.
    pub doc: &'static str,
    /// Default snippet / property stub when inserting or stubbing this primitive.
    pub default_stub: Option<&'static str>,
}

/// Build a [`PrimitiveCapabilities`] set from the fields that are `true`;
/// every omitted field stays `false` via [`PrimitiveCapabilities::NONE`].
///
/// The named form keeps each card's intent reviewable and makes an accidental
/// positional flip impossible. Evaluating in a const context keeps the catalog
/// in static data.
macro_rules! caps {
    ($($field:ident),* $(,)?) => {
        PrimitiveCapabilities {
            $($field: true,)*
            ..PrimitiveCapabilities::NONE
        }
    };
}

const SHAPE_CAPS: PrimitiveCapabilities = caps!(vector_paths, is_shape, has_visual_content);
const TEXT_CAPS: PrimitiveCapabilities = caps!(text_paths, has_visual_content);
const PLOT_CAPS: PrimitiveCapabilities = caps!(vector_paths, plot_geometry, has_visual_content);
const CONTAINER_CAPS: PrimitiveCapabilities = caps!(layout_container, is_container);
const EQUATION_CAPS: PrimitiveCapabilities =
    caps!(layout_container, is_container, has_visual_content);
const CAPTIONS_CAPS: PrimitiveCapabilities = caps!(
    text_paths,
    layout_container,
    is_container,
    has_visual_content,
    time_dependent,
    transcript
);
const CUE_CAPS: PrimitiveCapabilities = caps!(text_paths, has_visual_content, time_dependent);
/// `Glass` is a container with a rectangular *region*, not a surface. It carries
/// no `ShapeKind` — the same choice `Filter` and `Mask` make — so the shape
/// predicates (`fill_opacity`, the gradient paints) key off `caps.shape.is_some()`
/// and do not invite a scope that paints nothing; `corner_radius` stays applicable
/// through its own actor list because it is the frost's clip, and `is_shape` stays
/// set because the region and its anchors need a bounded actor.
const GLASS_CAPS: PrimitiveCapabilities = caps!(layout_container, is_container, is_shape);
const GROUP_CAPS: PrimitiveCapabilities = caps!(is_container);
/// `Audio` carries the `transcript` property the build's speech pre-pass
/// parses; it paints nothing itself.
const AUDIO_CAPS: PrimitiveCapabilities = caps!(transcript);
const IMAGE_CAPS: PrimitiveCapabilities = caps!(image_payload, has_visual_content);
const SVG_CAPS: PrimitiveCapabilities = caps!(vector_paths, has_visual_content);
const GRAPH_CAPS: PrimitiveCapabilities =
    caps!(vector_paths, plot_geometry, plot_host, has_visual_content);
const ANNOTATION_CAPS: PrimitiveCapabilities = caps!(vector_paths, has_visual_content);

mod caps_impl;
mod info_impl;
mod lookup;
#[cfg(test)]
mod tests;

pub use info_impl::*;
pub use lookup::{caps_for_type, caps_from_info, catalog_lookup};

#[cfg(test)]
mod caps_tests {
    use super::*;

    #[test]
    fn none_is_all_false() {
        assert_eq!(PrimitiveCapabilities::NONE, PrimitiveCapabilities::default());
    }

    #[test]
    fn shape_caps_use_only_their_named_fields() {
        assert_eq!(
            SHAPE_CAPS,
            PrimitiveCapabilities {
                vector_paths: true,
                is_shape: true,
                has_visual_content: true,
                ..PrimitiveCapabilities::NONE
            }
        );
    }
}
