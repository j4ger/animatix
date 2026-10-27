//! Unified primitive system for Animatix.
//!
//! Every actor type (shape, text, media, plot, container) is a `Primitive`.
//! `BUILT_INS` is the bootstrap list — each row pairs an `animatix-std`
//! identity card with the behaviour implementing it; `PrimitiveRegistry`
//! seeds built-ins through the same registration path used by extensions.
//!
//! ## Architecture
//!
//! ```text
//! BUILT_INS array (bootstrap: card + behaviour, paired by symbol)
//!        │
//!        ├──► PrimitiveRegistry (single storage for built-ins + extensions)
//!        ├──► find_primitive() — compatibility lookup for static built-ins
//!        └──► PrimitiveInfo cards (animatix_std::CATALOG, the metadata source)
//! ```
//!
//! Identity is the authored type name, stored per entity in `World`
//! (`World::actor_type`); the
//! engine dispatches on `ActorCaps`, a `Copy` projection derived from the
//! catalog row. There is no actor-kind enum to extend.
//!
//! ## Adding a new primitive
//!
//! 1. Add the identity card: a `pub const <NAME>: PrimitiveInfo` in `animatix-std/src/catalog.rs`
//!    (type/display/icon/category/advanced/ capabilities/child processing) and its name in the
//!    `CATALOG` list. Tooling, the parser's contract tables, and the inspector palette all derive
//!    from the card.
//! 2. Create `primitives/<name>.rs` implementing `Primitive` (behaviour only — no metadata
//!    methods).
//! 3. Add `BuiltIn::new(&catalog::<NAME>, &<NAME>::CONST)` to the `BUILT_INS` array below. The card
//!    is referenced by symbol, so the two lists cannot drift positionally; a name mismatch fails
//!    the pairing test.
//! 4. If the primitive has properties, add the descriptor row to
//!    `animatix-core::property::PROPERTY_DESCRIPTORS` (name + applicability + value kind), its type
//!    row to `animatix-syntax/src/schema.rs::raw_property_types()`, and the runtime binding to
//!    `timeline/property_registry.rs::BINDINGS`; the tests in those modules fail until all three
//!    line up. Append descriptor rows — the row index is the serialized `PropertyId`.
//! 5. Document it (docs/primitives.md, docs/spec.md) and add render/hit-region coverage if it
//!    draws.
//!
//! ## Current primitives
//!
//! | Category | Primitives |
//! |----------|-----------|
//! | Shapes | Rect, Ellipse, Line, Polygon, Path |
//! | Text | Text, Math, Code |
//! | Media | Image, Svg |
//! | Plots | Graph, PlotCurve |
//! | Containers | Row, Col, Grid, Stack, Group, Mask |

use std::sync::OnceLock;

use crate::timeline::ActorCategory;

pub mod commands;
mod context;
pub mod styling;
mod r#trait;

pub use commands::{RenderCommand, image_display_transform, translate_text_paths};
pub use context::{
    AssignmentCtx, BuildCtx, CaptionCue, EquationFragment, EvaluateCtx, RenderChildrenCtx,
    RenderCtx, TextCompileCtx,
};
pub(crate) use context::{RenderFrame, RenderOutputs};
pub(crate) use registry::{PrimitiveCacheEntry, PrimitiveHandle};
pub use styling::sample_shape_style;
pub(crate) use styling::{ShapeCommandMemo, evaluate_shape_render, stamp_stroke_decoration};
#[cfg(any(test, feature = "plugin-loading"))]
pub(crate) use r#trait::clip_bezpath_from_commands;
pub use r#trait::{Primitive, evaluate_text_paths};
pub(crate) use r#trait::{resolve_half_size, unreadable_layout_value};

/// Child-rendering strategy selected by a primitive.
pub use animatix_syntax::schema::ChildProcessingKind as ChildProcessing;

// ── Re-export all primitive modules ──────────────────────────────────────

mod rect;
pub use rect::RECT;
mod ellipse;
pub use ellipse::ELLIPSE;
mod line;
pub use line::LINE;
mod arrow;
pub use arrow::ARROW;
pub(crate) use arrow::build_arrow_path;
mod polygon;
pub use polygon::POLYGON;
mod path;
pub use path::PATH;
mod text;
pub use text::TEXT;
mod code;
pub use code::CODE;
mod math;
pub use math::MATH;
mod image;
pub use image::IMAGE;
#[cfg(feature = "svg")]
mod svg;
#[cfg(feature = "svg")]
pub use svg::SVG;
mod bar_chart;
pub use bar_chart::BAR_CHART;
pub(crate) mod plot;
pub(crate) mod plot_assignment;
pub use plot::{CONTOUR_SET, GRAPH, HEATMAP, NUMBER_PLANE, PLOT_CURVE, VECTOR_FIELD};
mod row;
pub use row::ROW;
mod col;
pub use col::COL;
mod grid;
pub use grid::GRID;
mod stack;
pub use stack::STACK;
mod group;
pub use group::GROUP;
mod mask;
pub use mask::MASK;
mod filter;
pub use filter::FILTER;
mod glass;
pub use glass::GLASS;
pub mod payload;
pub use payload::*;
mod registry;
pub use registry::{PrimitiveRegistrationError, PrimitiveRegistry};

mod typst;
pub use typst::TYPST;

mod audio;
pub use audio::AUDIO;

mod equation;
pub use equation::EQUATION;
mod fragment;
pub use fragment::FRAGMENT;
pub(crate) mod captions;
pub use captions::CAPTIONS;
mod cue;
pub use cue::CUE;
mod callout;
pub use callout::CALLOUT;
pub(crate) mod legend;
pub use legend::LEGEND;
pub(crate) mod connector;
pub use connector::CONNECTOR;
pub(crate) mod counter;
pub use counter::COUNTER;

/// Map the runtime UI category onto the schema category used by tooling.
pub(crate) fn actor_category_to_primitive_category(
    category: ActorCategory,
) -> animatix_syntax::schema::PrimitiveCategory {
    match category {
        ActorCategory::Shape => animatix_syntax::schema::PrimitiveCategory::Shape,
        ActorCategory::Text => animatix_syntax::schema::PrimitiveCategory::Text,
        ActorCategory::Media => animatix_syntax::schema::PrimitiveCategory::Media,
        ActorCategory::Plot => animatix_syntax::schema::PrimitiveCategory::Plot,
        ActorCategory::Container => animatix_syntax::schema::PrimitiveCategory::Container,
        ActorCategory::Annotation => animatix_syntax::schema::PrimitiveCategory::Annotation,
    }
}

// ── The one static array ────────────────────────────────────────────────

/// A built-in primitive: its `animatix-std` identity card paired by symbol
/// with the engine behaviour that implements it.
///
/// One row per primitive in [`BUILT_INS`]; referencing the card directly is
/// what removes the positional coupling between the catalog and the
/// behaviour list.
pub struct BuiltIn {
    /// The identity card from the `animatix-std` catalog (metadata source).
    pub info: &'static PrimitiveInfo,
    /// The compiled-in behaviour implementation.
    pub behavior: &'static dyn Primitive,
}

impl BuiltIn {
    /// Pair a catalog card with its behaviour implementation.
    pub const fn new(info: &'static PrimitiveInfo, behavior: &'static dyn Primitive) -> Self {
        Self { info, behavior }
    }
}

/// Bootstrap list of all built-in primitives.
///
/// `PrimitiveRegistry::new()` registers these through the same `register`
/// path used by extension primitives. Order mirrors the catalog for
/// presentation only; pairing is by symbol, not position.
pub static BUILT_INS: &[BuiltIn] = &[
    // Shapes
    BuiltIn::new(&animatix_std::catalog::RECT, &RECT),
    BuiltIn::new(&animatix_std::catalog::ELLIPSE, &ELLIPSE),
    BuiltIn::new(&animatix_std::catalog::LINE, &LINE),
    BuiltIn::new(&animatix_std::catalog::ARROW, &ARROW),
    BuiltIn::new(&animatix_std::catalog::POLYGON, &POLYGON),
    BuiltIn::new(&animatix_std::catalog::PATH, &PATH),
    // Text
    BuiltIn::new(&animatix_std::catalog::TEXT, &TEXT),
    BuiltIn::new(&animatix_std::catalog::CODE, &CODE),
    BuiltIn::new(&animatix_std::catalog::MATH, &MATH),
    BuiltIn::new(&animatix_std::catalog::TYPST, &TYPST),
    // Media
    BuiltIn::new(&animatix_std::catalog::IMAGE, &IMAGE),
    #[cfg(feature = "svg")]
    BuiltIn::new(&animatix_std::catalog::SVG, &SVG),
    BuiltIn::new(&animatix_std::catalog::AUDIO, &AUDIO),
    // Plots
    BuiltIn::new(&animatix_std::catalog::GRAPH, &GRAPH),
    BuiltIn::new(&animatix_std::catalog::PLOT_CURVE, &PLOT_CURVE),
    BuiltIn::new(&animatix_std::catalog::VECTOR_FIELD, &VECTOR_FIELD),
    BuiltIn::new(&animatix_std::catalog::HEATMAP, &HEATMAP),
    BuiltIn::new(&animatix_std::catalog::CONTOUR_SET, &CONTOUR_SET),
    BuiltIn::new(&animatix_std::catalog::NUMBER_PLANE, &NUMBER_PLANE),
    BuiltIn::new(&animatix_std::catalog::BAR_CHART, &BAR_CHART),
    // Containers
    BuiltIn::new(&animatix_std::catalog::ROW, &ROW),
    BuiltIn::new(&animatix_std::catalog::COL, &COL),
    BuiltIn::new(&animatix_std::catalog::GRID, &GRID),
    BuiltIn::new(&animatix_std::catalog::STACK, &STACK),
    BuiltIn::new(&animatix_std::catalog::GROUP, &GROUP),
    BuiltIn::new(&animatix_std::catalog::MASK, &MASK),
    BuiltIn::new(&animatix_std::catalog::FILTER, &FILTER),
    BuiltIn::new(&animatix_std::catalog::GLASS, &GLASS),
    BuiltIn::new(&animatix_std::catalog::EQUATION, &EQUATION),
    // Equation / Fragment
    BuiltIn::new(&animatix_std::catalog::FRAGMENT, &FRAGMENT),
    // Captions container & Cue sub-item
    BuiltIn::new(&animatix_std::catalog::CAPTIONS, &CAPTIONS),
    BuiltIn::new(&animatix_std::catalog::CUE, &CUE),
    // Annotations
    BuiltIn::new(&animatix_std::catalog::CALLOUT, &CALLOUT),
    BuiltIn::new(&animatix_std::catalog::LEGEND, &LEGEND),
    BuiltIn::new(&animatix_std::catalog::CONNECTOR, &CONNECTOR),
    BuiltIn::new(&animatix_std::catalog::COUNTER, &COUNTER),
];
// ── Built-in metadata ───────────────────────────────────────────────────

pub use animatix_std::PrimitiveInfo;

/// The built-in primitive catalog (the `animatix-std` identity cards).
pub fn primitive_catalog() -> &'static [&'static PrimitiveInfo] {
    animatix_std::CATALOG
}

/// Look up built-in metadata by the actor's authored type name.
pub fn primitive_info_by_name(name: &str) -> Option<&'static PrimitiveInfo> {
    animatix_std::catalog_lookup(name)
}

/// Expose built-in primitive metadata through the shared schema model.
///
/// Single conversion implementation lives on [`PrimitiveRegistry::specs`];
/// this free function is a thin wrapper over the built-in registry so the
/// spec-building logic is not duplicated.
pub fn primitive_specs() -> Vec<animatix_syntax::schema::PrimitiveSpec> {
    builtin_primitive_registry().specs()
}

// ── Dispatch helpers ────────────────────────────────────────────────────

/// Look up a built-in primitive by type name.
///
/// This compatibility helper is backed by the same [`PrimitiveRegistry`] used
/// by timeline builds, not by a separate static lookup.
pub fn find_primitive(ty: &str) -> Option<&'static dyn Primitive> {
    builtin_primitive_registry().find(ty)
}

fn builtin_primitive_registry() -> &'static PrimitiveRegistry {
    static REGISTRY: OnceLock<PrimitiveRegistry> = OnceLock::new();
    REGISTRY.get_or_init(PrimitiveRegistry::new)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests;
