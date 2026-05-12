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
//! | Shapes | Rect, Circle, Square, Ellipse, Line, Arc, Polygon, RegularPolygon, Path, Arrow, Dot |
//! | Text | Text, Math, Code |
//! | Media | Image, Svg |
//! | Plots | Graph, CartesianPlot, PolarPlot, ParametricPlot, ImplicitPlot |
//! | Containers | Row, Col, Grid, Stack, Group |

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::timeline::{
    ActorCategory, ActorKindId, Environment, SceneDimensions, Timeline, VectorShapeState,
    VectorShapeStyle, VelloPath,
};

// ── Re-export all primitive modules ──────────────────────────────────────

mod rect;       pub use rect::RECT;
mod circle;     pub use circle::CIRCLE;
mod square;     pub use square::SQUARE;
mod ellipse;    pub use ellipse::ELLIPSE;
mod line;       pub use line::LINE;
mod arc;        pub use arc::ARC;
mod polygon;    pub use polygon::POLYGON;
mod regular_polygon; pub use regular_polygon::REGULAR_POLYGON;
mod path;       pub use path::PATH;
mod arrow;      pub use arrow::ARROW;
mod dot;        pub use dot::DOT;
mod text;       pub use text::TEXT;
mod math;       pub use math::MATH;
mod code;       pub use code::CODE;
mod image;      pub use image::IMAGE;
mod svg;        pub use svg::SVG;
mod plot;       pub use plot::{GRAPH, CARTESIAN_PLOT, POLAR_PLOT, PARAMETRIC_PLOT, IMPLICIT_PLOT};
mod row;        pub use row::ROW;
mod col;        pub use col::COL;
mod grid;       pub use grid::GRID;
mod stack;      pub use stack::STACK;
mod group;      pub use group::GROUP;

// ── Primitive trait ─────────────────────────────────────────────────────

/// Context passed to `Primitive::build()`.
pub struct BuildCtx<'a> {
    pub timeline: &'a mut Timeline,
    pub time_ms: f64,
    pub parent_label: Option<&'a str>,
    pub diagnostics: &'a mut Vec<Diagnostic>,
}

/// Context passed to `Primitive::render()`.
pub struct RenderCtx<'a> {
    pub state: &'a VectorShapeState,
    pub style: VectorShapeStyle,
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
        _actor_type: &str,
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
    fn finalize_state(&self, _actor_type: &str, _state: &mut VectorShapeState) {}

    /// Returns true if this shape uses a custom path (Polygon, Path).
    fn uses_custom_path(&self) -> bool { false }

    /// Returns true if this shape exposes tip size properties (Arrow).
    fn exposes_tip_size(&self) -> bool { false }

    /// Returns true if this shape supports fill (most shapes; Line and Arc do not).
    fn supports_fill(&self) -> bool { true }

    // ── GUI defaults ──

    /// Default properties used when creating this actor from the GUI.
    fn default_props(&self, _scene_dimensions: &SceneDimensions) -> Vec<Property> {
        vec![]
    }
}

// ── The one static array ────────────────────────────────────────────────

/// Canonical registry of all primitives.
///
/// **This is the only place you add a new primitive.**
pub static PRIMITIVES: &[&dyn Primitive] = &[
    // Shapes (basic)
    &RECT, &CIRCLE,
    // Shapes (advanced)
    &SQUARE, &ELLIPSE, &LINE, &ARC, &POLYGON, &REGULAR_POLYGON,
    &PATH, &ARROW, &DOT,
    // Text
    &TEXT, &MATH, &CODE,
    // Media
    &IMAGE, &SVG,
    // Plots
    &GRAPH, &CARTESIAN_PLOT, &POLAR_PLOT, &PARAMETRIC_PLOT, &IMPLICIT_PLOT,
    // Containers
    &ROW, &COL, &GRID, &STACK, &GROUP,
];

// ── Auto-generated registry ─────────────────────────────────────────────

/// Static metadata generated from `PRIMITIVES`.
/// Built once at first access via `OnceLock`.
pub struct ActorKindMeta {
    pub kind: ActorKindId,
    pub type_name: &'static str,
    pub display_name: &'static str,
    pub category: ActorCategory,
    pub icon_id: &'static str,
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
        .unwrap_or_else(|| panic!("ActorKindMeta missing for {:?}", kind))
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
            ShapeKind::Rect, ShapeKind::Circle, ShapeKind::Ellipse,
            ShapeKind::Line, ShapeKind::Arc, ShapeKind::Polygon,
            ShapeKind::Path, ShapeKind::Arrow, ShapeKind::Dot,
            ShapeKind::Square, ShapeKind::RegularPolygon,
        ];
        for sk in &shape_kinds {
            let id = ActorKindId::Shape(*sk);
            assert!(kinds.contains(&id), "Missing ActorKindMeta for ShapeKind::{:?}", sk);
        }

        for id in [
            ActorKindId::Text, ActorKindId::Math, ActorKindId::Code,
            ActorKindId::Image, ActorKindId::Svg,
            ActorKindId::Graph, ActorKindId::CartesianPlot,
            ActorKindId::PolarPlot, ActorKindId::ParametricPlot,
            ActorKindId::ImplicitPlot,
            ActorKindId::Row, ActorKindId::Col, ActorKindId::Grid,
            ActorKindId::Stack, ActorKindId::Group,
        ] {
            assert!(kinds.contains(&id), "Missing ActorKindMeta for {:?}", id);
        }
    }
}
