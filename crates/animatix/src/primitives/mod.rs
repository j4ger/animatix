//! Unified primitive system for Animatix.
//!
//! Every actor type (shape, text, media, plot, container) is a `Primitive`.
//! `BUILT_INS` is the bootstrap list — each row pairs an `animatix-std`
//! identity card with the behaviour implementing it and the family function
//! pointers that drive evaluation/building; `PrimitiveRegistry` seeds
//! built-ins through the same registration path used by extensions.
//!
//! ## Architecture
//!
//! ```text
//! BUILT_INS array (bootstrap: card + behaviour + family behaviour)
//!        │
//!        ├──► PrimitiveRegistry (single storage for built-ins + extensions)
//!        ├──► BehaviorRegistry (one BehaviorDef per row, ID = row index)
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
//! 3. Append a `BuiltIn::new(&catalog::<NAME>, &<NAME>::CONST, <family>::evaluate_<name>, <build_extra>)`
//!    row to `BUILT_INS`. The row carries the catalog card by symbol, the compiled-in
//!    behaviour, and the evaluate/build_extra function pointers in one place, so metadata
//!    and behaviour cannot drift positionally; a primitive added to the catalog without a
//!    row fails the pairing test instead of panicking when `BehaviorRegistry` is
//!    constructed.
//! 4. If the primitive has properties, add a core descriptor (with a non-default
//!    `AuthoredTypeHint` only when needed) and an engine binding. The syntax type view
//!    derives from the descriptor; `timeline::property_registry` documents the join and
//!    the tests that fail until the tables agree. Append descriptor rows — the row index
//!    is the serialized `PropertyId`.
//! 5. Document it (docs/primitives.md, docs/spec.md) and add render/hit-region coverage if it
//!    draws.
//!
//! The `animatix-std` catalog is the authoritative list of built-ins.

use crate::timeline::behavior::FamilyBehavior;
use crate::timeline::behavior::builtin;

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
mod container_props;
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

mod impls;

pub(crate) use impls::actor_category_to_primitive_category;
pub use impls::{primitive_catalog, primitive_info_by_name, primitive_specs};
// The re-export keeps the historical `primitives::find_primitive` path; the
// deprecation notice stays on the item, so callers still see it.
#[allow(deprecated)]
pub use impls::find_primitive;

/// A built-in primitive: its `animatix-std` identity card paired by symbol
/// with the engine behaviour that implements it, plus the family function
/// pointers used by evaluation and custom declaration build logic.
pub struct BuiltIn {
    /// The identity card from the `animatix-std` catalog (metadata source).
    pub info: &'static PrimitiveInfo,
    /// The compiled-in behaviour implementation.
    pub behavior: &'static dyn Primitive,
    /// Evaluation and custom build function pointers for this primitive.
    pub family: FamilyBehavior,
}

/// Bootstrap list of all built-in primitives.
///
/// `PrimitiveRegistry::new()` registers these through the same `register`
/// path used by extension primitives. The row index is the stable
/// [`BehaviorId`](crate::timeline::BehaviorId) for a built-in, so new rows are
/// appended; the order is not presentation-only.
pub static BUILT_INS: &[BuiltIn] = &[
    // Shapes
    BuiltIn::new(&animatix_std::catalog::RECT, &RECT, builtin::shape::evaluate_rect, None),
    BuiltIn::new(
        &animatix_std::catalog::ELLIPSE,
        &ELLIPSE,
        builtin::shape::evaluate_ellipse,
        None,
    ),
    BuiltIn::new(&animatix_std::catalog::LINE, &LINE, builtin::shape::evaluate_line, None),
    BuiltIn::new(&animatix_std::catalog::ARROW, &ARROW, builtin::shape::evaluate_arrow, None),
    BuiltIn::new(
        &animatix_std::catalog::POLYGON,
        &POLYGON,
        builtin::shape::evaluate_polygon,
        None,
    ),
    BuiltIn::new(&animatix_std::catalog::PATH, &PATH, builtin::shape::evaluate_path, None),
    // Text
    BuiltIn::new(
        &animatix_std::catalog::TEXT,
        &TEXT,
        builtin::text::evaluate_text,
        Some(builtin::text::build_extra_text_named),
    ),
    BuiltIn::new(
        &animatix_std::catalog::CODE,
        &CODE,
        builtin::text::evaluate_code,
        Some(builtin::text::build_extra_text_named),
    ),
    BuiltIn::new(
        &animatix_std::catalog::MATH,
        &MATH,
        builtin::text::evaluate_math,
        Some(builtin::text::build_extra_text_named),
    ),
    BuiltIn::new(
        &animatix_std::catalog::TYPST,
        &TYPST,
        builtin::text::evaluate_typst,
        Some(builtin::text::build_extra_text_named),
    ),
    // Media
    BuiltIn::new(
        &animatix_std::catalog::IMAGE,
        &IMAGE,
        builtin::media::evaluate_image,
        Some(builtin::media::build_extra_media_named),
    ),
    #[cfg(feature = "svg")]
    BuiltIn::new(
        &animatix_std::catalog::SVG,
        &SVG,
        builtin::media::evaluate_svg,
        Some(builtin::media::build_extra_media_named),
    ),
    BuiltIn::new(
        &animatix_std::catalog::AUDIO,
        &AUDIO,
        builtin::media::evaluate_audio,
        Some(builtin::media::build_extra_media_named),
    ),
    // Plots
    BuiltIn::new(
        &animatix_std::catalog::GRAPH,
        &GRAPH,
        builtin::plot::evaluate_plot,
        Some(builtin::plot::build_extra_plot_named),
    ),
    BuiltIn::new(
        &animatix_std::catalog::PLOT_CURVE,
        &PLOT_CURVE,
        builtin::plot::evaluate_plot,
        Some(builtin::plot::build_extra_plot_named),
    ),
    BuiltIn::new(
        &animatix_std::catalog::VECTOR_FIELD,
        &VECTOR_FIELD,
        builtin::plot::evaluate_plot,
        Some(builtin::plot::build_extra_plot_named),
    ),
    BuiltIn::new(
        &animatix_std::catalog::HEATMAP,
        &HEATMAP,
        builtin::plot::evaluate_plot,
        Some(builtin::plot::build_extra_plot_named),
    ),
    BuiltIn::new(
        &animatix_std::catalog::CONTOUR_SET,
        &CONTOUR_SET,
        builtin::plot::evaluate_plot,
        Some(builtin::plot::build_extra_plot_named),
    ),
    BuiltIn::new(
        &animatix_std::catalog::NUMBER_PLANE,
        &NUMBER_PLANE,
        builtin::plot::evaluate_plot,
        Some(builtin::plot::build_extra_plot_named),
    ),
    BuiltIn::new(
        &animatix_std::catalog::BAR_CHART,
        &BAR_CHART,
        builtin::plot::evaluate_plot,
        Some(builtin::plot::build_extra_plot_named),
    ),
    // Containers
    BuiltIn::new(&animatix_std::catalog::ROW, &ROW, builtin::container::evaluate_container, None),
    BuiltIn::new(&animatix_std::catalog::COL, &COL, builtin::container::evaluate_container, None),
    BuiltIn::new(
        &animatix_std::catalog::GRID,
        &GRID,
        builtin::container::evaluate_container,
        None,
    ),
    BuiltIn::new(
        &animatix_std::catalog::STACK,
        &STACK,
        builtin::container::evaluate_container,
        None,
    ),
    BuiltIn::new(
        &animatix_std::catalog::GROUP,
        &GROUP,
        builtin::container::evaluate_container,
        None,
    ),
    BuiltIn::new(&animatix_std::catalog::MASK, &MASK, builtin::effect::evaluate_effect, None),
    BuiltIn::new(&animatix_std::catalog::FILTER, &FILTER, builtin::effect::evaluate_effect, None),
    BuiltIn::new(&animatix_std::catalog::GLASS, &GLASS, builtin::effect::evaluate_effect, None),
    BuiltIn::new(
        &animatix_std::catalog::EQUATION,
        &EQUATION,
        builtin::equation::evaluate_equation,
        None,
    ),
    // Equation / Fragment
    BuiltIn::new(
        &animatix_std::catalog::FRAGMENT,
        &FRAGMENT,
        builtin::equation::evaluate_fragment,
        Some(builtin::equation::build_extra_fragment),
    ),
    // Captions container & Cue sub-item
    BuiltIn::new(
        &animatix_std::catalog::CAPTIONS,
        &CAPTIONS,
        builtin::captions::evaluate_captions,
        None,
    ),
    BuiltIn::new(
        &animatix_std::catalog::CUE,
        &CUE,
        builtin::captions::evaluate_cue,
        Some(builtin::captions::build_extra_cue),
    ),
    // Annotations
    BuiltIn::new(
        &animatix_std::catalog::CALLOUT,
        &CALLOUT,
        builtin::annotation::evaluate_callout,
        None,
    ),
    BuiltIn::new(
        &animatix_std::catalog::LEGEND,
        &LEGEND,
        builtin::annotation::evaluate_legend,
        Some(builtin::annotation::build_extra_legend),
    ),
    BuiltIn::new(
        &animatix_std::catalog::CONNECTOR,
        &CONNECTOR,
        builtin::annotation::evaluate_connector,
        None,
    ),
    BuiltIn::new(
        &animatix_std::catalog::COUNTER,
        &COUNTER,
        builtin::text::evaluate_counter,
        Some(builtin::text::build_extra_text_named),
    ),
];

pub use animatix_std::PrimitiveInfo;

pub(crate) use impls::{media_default_props, shape_default_props, text_default_props};

#[cfg(test)]
mod tests;
