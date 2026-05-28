//! Unified primitive system for Animatix.
//!
//! Every actor type (shape, text, media, plot, container) is a `Primitive`.
//! The single `PRIMITIVES` array is the source of truth for metadata,
//! build logic, and render logic.
//!
//! ## Architecture
//!
//! ```text
//! PRIMITIVES array (static)
//!        │
//!        ├──► ActorKindMeta registry (auto-generated via OnceLock)
//!        ├──► find_primitive() — by type name
//!        ├──► PrimitiveDescriptor::for_actor_type()
//!        └──► ActorKind dispatch (via PrimitiveActorKind wrapper)
//! ```
//!
//! ## Adding a new primitive
//!
//! 1. Create `primitives/<name>.rs` implementing `Primitive`
//! 2. Add `&<name>::CONST` to the `PRIMITIVES` array below
//! 3. Add variant to `ActorKindId` in `timeline/track.rs`
//! 4. If it's a shape, add variant to `ShapeKind` in `timeline/track.rs`
//!
//! Steps 3-4 are required because enums are used in match arms across
//! the codebase and cannot be auto-generated from a static array.
//! However, the metadata registry (`ActorKindMeta`) IS auto-generated
//! from `PRIMITIVES`, so you never need to touch the registry manually.
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
//!
use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::{
    ActorCategory, ActorKindId, AnimationTrack, Environment, SceneDimensions, Timeline,
    VectorShapeState, VectorShapeStyle, VelloPath,
};

// ── Re-export all primitive modules ──────────────────────────────────────

mod rect;       pub use rect::RECT;
mod ellipse;    pub use ellipse::ELLIPSE;
mod line;       pub use line::LINE;
mod polygon;    pub use polygon::POLYGON;
mod path;       pub use path::PATH;
mod text;       pub use text::TEXT;
mod math;       pub use math::MATH;
mod code;       pub use code::CODE;
mod image;      pub use image::IMAGE;
mod svg;        pub use svg::SVG;
mod plot;       pub use plot::{GRAPH, PLOT_CURVE, VECTOR_FIELD, HEATMAP, CONTOUR_SET, NUMBER_PLANE};
mod row;        pub use row::ROW;
mod col;        pub use col::COL;
mod grid;       pub use grid::GRID;
mod stack;      pub use stack::STACK;
mod group;      pub use group::GROUP;
mod mask;       pub use mask::MASK;

mod viewport;   pub use viewport::VIEWPORT;

mod typst;      pub use typst::TYPST;

mod audio;      pub use audio::AUDIO;

// ── Primitive trait ─────────────────────────────────────────────────────

/// Context passed to `Primitive::build()`.
pub struct BuildCtx<'a> {
    /// The timeline being built.
    pub timeline: &'a mut Timeline,
    /// Current time in milliseconds.
    pub time_ms: f64,
    /// Optional parent actor label.
    pub parent_label: Option<&'a str>,
    /// Build diagnostics collector.
    pub diagnostics: &'a mut Vec<Diagnostic>,
}

/// Timing and resource context for `Primitive::handle_assignment()`.
pub struct AssignmentCtx<'a> {
    /// Animation start time in milliseconds.
    pub t_start_ms: u64,
    /// Animation end time in milliseconds.
    pub t_end_ms: u64,
    /// Easing function for the animation.
    pub easing: Easing,
    /// Whether the animation is instant but delayed.
    pub instant_delayed: bool,
    /// Animation duration in milliseconds.
    pub duration_ms: f64,
    /// Font rendering context.
    pub font_context: &'a crate::renderer::text::FontContext,
    /// Text compiler for recompilation.
    pub text_compiler: &'a mut crate::renderer::text::TextCompiler,
}

/// Context passed to `Primitive::render()`.
pub struct RenderCtx<'a> {
    /// Current vector shape state.
    pub state: &'a VectorShapeState,
    /// Shape style (color, stroke, fill).
    pub style: VectorShapeStyle,
    /// Current time in milliseconds.
    pub time_ms: u64,
}

/// Every actor type in Animatix implements this trait.
///
/// Metadata, build logic, and (optionally) render logic live in one place.
pub trait Primitive: Send + Sync {
    // ── Metadata ──

    /// Source-text type name, e.g. "Rect", "Text", "Row".
    fn type_name(&self) -> &'static str;

    /// Human-readable label for UI palettes and tooltips.
    fn display_name(&self) -> &'static str;

    /// UI category (Shapes, Text, Media, Plots, Containers).
    fn category(&self) -> ActorCategory;

    /// Opaque icon identifier. The GUI maps this to a concrete icon.
    fn icon_id(&self) -> &'static str;

    /// When true, shown in a "More..." submenu instead of top-level.
    fn is_advanced(&self) -> bool { false }

    /// Returns true if this primitive is a layout container.
    fn is_container(&self) -> bool { false }

    /// Returns true if this primitive renders as a vector shape.
    fn is_shape(&self) -> bool { false }

    /// Returns the corresponding `ActorKindId` variant.
    fn kind_id(&self) -> ActorKindId;

    // ── Build: AST → Timeline ──

    /// Build the actor into the timeline.
    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>>;

    // ── Render (optional, for shapes) ──

    /// Render the primitive into Vello paths.
    /// Returns `None` for non-visual primitives.
    fn render(&self, _ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        None
    }

    // ── Build-time shape state (for vector shapes) ──

    /// Apply primitive-specific defaults to the shape state.
    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    /// Apply a single property to the shape state.
    /// Returns `true` if the property was handled.
    fn apply_property(
        &self,
        _name: &str,
        _value: &Expr,
        _env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
        _state: &mut VectorShapeState,
    ) -> bool {
        false
    }

    /// Finalize the shape state after all properties have been applied.
    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    /// Returns true if this shape uses a custom path (Polygon, Path).
    fn uses_custom_path(&self) -> bool { false }

    /// Returns true if this shape exposes tip size properties (Line with arrows).
    fn exposes_tip_size(&self) -> bool { false }

    /// Returns true if this shape supports fill.
    fn supports_fill(&self) -> bool { true }

    /// Returns the colorscheme key for default color lookup.
    /// For example, "Text" returns "text.primary", shapes return "accent.primary".
    fn default_color_key(&self, property: &str) -> Option<&'static str> {
        match property {
            "color" => match self.category() {
                ActorCategory::Text => Some("text.primary"),
                ActorCategory::Shape | ActorCategory::Plot => Some("surface.primary"),
                ActorCategory::Media => Some("text.primary"),
                ActorCategory::Container => None,
            },
            "stroke" | "stroke_color" => match self.category() {
                ActorCategory::Shape => Some("stroke.default"),
                _ => None,
            },
            _ => None,
        }
    }

    /// How the GUI should resize this actor.
    fn resize_mode(&self) -> crate::timeline::ResizeMode {
        match self.category() {
            ActorCategory::Text | ActorCategory::Media | ActorCategory::Plot => {
                crate::timeline::ResizeMode::Scale
            }
            _ => crate::timeline::ResizeMode::Size,
        }
    }

    // ── GUI defaults ──

    /// Default properties used when creating this actor from the GUI.
    fn default_props(&self, _scene_dimensions: &SceneDimensions) -> Vec<Property> {
        vec![]
    }

    // ── Assignment-phase handling ──

    /// Handle a property assignment at the assignment phase.
    /// Return `true` if the primitive handled it (bypassing generic engine).
    /// Default implementation returns `false` (delegate to generic engine).
    fn handle_assignment(
        &self,
        _track: &mut AnimationTrack,
        _property: &str,
        _value: &Expr,
        _ctx: &mut AssignmentCtx,
        _env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
    ) -> bool {
        false
    }
}

// ── The one static array ────────────────────────────────────────────────

/// Canonical registry of all primitives.
///
/// **This is the only place you add a new primitive.**
pub static PRIMITIVES: &[&dyn Primitive] = &[
    // Shapes
    &RECT, &ELLIPSE, &LINE, &POLYGON, &PATH,
    // Text
    &TEXT, &MATH, &CODE, &TYPST,
    // Media
    &IMAGE, &SVG, &AUDIO,
    // Plots
    &GRAPH, &PLOT_CURVE, &VECTOR_FIELD, &HEATMAP, &CONTOUR_SET, &NUMBER_PLANE,
    // Containers
    &ROW, &COL, &GRID, &STACK, &GROUP, &MASK, &VIEWPORT,
];

// ── Auto-generated registry ─────────────────────────────────────────────

/// Static metadata generated from `PRIMITIVES`.
/// Built once at first access via `OnceLock`.
pub struct ActorKindMeta {
    /// Actor kind identifier.
    pub kind: ActorKindId,
    /// Source-text type name.
    pub type_name: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// UI category.
    pub category: ActorCategory,
    /// Icon identifier.
    pub icon_id: &'static str,
    /// Whether shown in advanced submenu.
    pub advanced: bool,
}

use std::sync::OnceLock;

static REGISTRY_LOCK: OnceLock<Vec<ActorKindMeta>> = OnceLock::new();

fn build_registry() -> Vec<ActorKindMeta> {
    PRIMITIVES
        .iter()
        .map(|p| ActorKindMeta {
            kind: p.kind_id(),
            type_name: p.type_name(),
            display_name: p.display_name(),
            category: p.category(),
            icon_id: p.icon_id(),
            advanced: p.is_advanced(),
        })
        .collect()
}

/// Get the auto-generated metadata registry.
pub fn actor_kind_registry() -> &'static [ActorKindMeta] {
    REGISTRY_LOCK.get_or_init(build_registry)
}

/// Look up metadata by `ActorKindId`.
pub fn actor_kind_meta(kind: ActorKindId) -> &'static ActorKindMeta {
    actor_kind_registry()
        .iter()
        .find(|m| m.kind == kind)
        .expect("actor_kind_meta: ActorKindId variant not found in PRIMITIVES array — add it to PRIMITIVES and ActorKindId enum")
}

/// Look up metadata by type name.
pub fn actor_kind_meta_by_name(name: &str) -> Option<&'static ActorKindMeta> {
    actor_kind_registry().iter().find(|m| m.type_name == name)
}

// ── Dispatch helpers ────────────────────────────────────────────────────

/// Look up a primitive by its type name.
pub fn find_primitive(ty: &str) -> Option<&'static dyn Primitive> {
    PRIMITIVES.iter().find(|p| p.type_name() == ty).copied()
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_primitives_have_unique_type_names() {
        let mut seen = std::collections::HashSet::new();
        for p in PRIMITIVES.iter() {
            let name = p.type_name();
            assert!(
                seen.insert(name),
                "Duplicate type_name: {:?}",
                name
            );
        }
    }

    #[test]
    fn find_primitive_roundtrips() {
        for p in PRIMITIVES.iter() {
            let found = find_primitive(p.type_name());
            assert!(
                found.is_some(),
                "find_primitive({:?}) returned None",
                p.type_name()
            );
            assert_eq!(found.unwrap().type_name(), p.type_name());
        }
    }

    #[test]
    fn registry_matches_primitives() {
        let registry = actor_kind_registry();
        assert_eq!(registry.len(), PRIMITIVES.len());
        for (meta, prim) in registry.iter().zip(PRIMITIVES.iter()) {
            assert_eq!(meta.kind, prim.kind_id());
            assert_eq!(meta.type_name, prim.type_name());
            assert_eq!(meta.display_name, prim.display_name());
            assert_eq!(meta.category, prim.category());
            assert_eq!(meta.icon_id, prim.icon_id());
            assert_eq!(meta.advanced, prim.is_advanced());
        }
    }

    #[test]
    fn every_kind_id_has_meta() {
        // This enumerates all variants and verifies they're in the registry
        use crate::timeline::ShapeKind;
        let registry = actor_kind_registry();
        let kinds: std::collections::HashSet<_> =
            registry.iter().map(|m| m.kind).collect();

        let shape_kinds = [
            ShapeKind::Rect, ShapeKind::Ellipse,
            ShapeKind::Line, ShapeKind::Polygon, ShapeKind::Path,
        ];
        for sk in &shape_kinds {
            let id = ActorKindId::Shape(*sk);
            assert!(kinds.contains(&id), "Missing ActorKindMeta for ShapeKind::{:?}", sk);
        }

        for id in [
            ActorKindId::Text, ActorKindId::Math, ActorKindId::Code, ActorKindId::Typst,
            ActorKindId::Image, ActorKindId::Svg,
            ActorKindId::Graph, ActorKindId::PlotCurve,
            ActorKindId::VectorField, ActorKindId::Heatmap, ActorKindId::ContourSet,
            ActorKindId::NumberPlane,
            ActorKindId::Row, ActorKindId::Col, ActorKindId::Grid,
            ActorKindId::Stack, ActorKindId::Group, ActorKindId::Mask,
            ActorKindId::Viewport, ActorKindId::Audio,
        ] {
            assert!(kinds.contains(&id), "Missing ActorKindMeta for {:?}", id);
        }
    }
}
