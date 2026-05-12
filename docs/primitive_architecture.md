# Unified Primitive Architecture

> **Status: Implemented** (see `crates/animatix/src/primitives/`)

## Goal

Reduce adding a new primitive from **11 touch points across 7 files** to **3 touch points in 3 files**, with a single source of truth for all primitive metadata, build logic, and render logic.

## Before (Legacy)

Three parallel dispatch systems knew about the same actor types:

```
ActorKindMeta registry ──► metadata (icon, label, category)
      │
      ├─ ActorKindId enum ──► identity
      ├─ ActorKind trait ──► build-time dispatch (bypassed by shapes!)
      └─ VectorShapePrimitive trait ──► render-time shape generation
```

**Adding Triangle required:**
1. `track.rs` — `ShapeKind` enum
2. `track.rs` — `ActorKindId::from_type_name()` match arm
3. `track.rs` — `ActorKindMeta` registry entry
4. `shapes/mod.rs` — `ShapeType` enum
5. `shapes/mod.rs` — `shape_type_for_actor()` match arm
6. `shapes/primitives.rs` — `TrianglePrimitive` struct + `VectorShapePrimitive` impl
7. `shapes/primitives.rs` — `primitive_for_shape_type()` match arm
8. `scene_eval.rs` — special-case handling
9. `property_registry.rs` — applicable shape kinds
10. `icons.rs` — `phosphor_icon()` match arm
11. `actions/mod.rs` — default properties for GUI creation

## After (Current)

### Single source of truth: `PRIMITIVES` array

```rust
// crates/animatix/src/primitives/mod.rs
pub trait Primitive: Send + Sync {
    // ── Metadata ──
    fn type_name(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn category(&self) -> ActorCategory;
    fn icon_id(&self) -> &'static str;
    fn is_advanced(&self) -> bool { false }
    fn is_container(&self) -> bool { false }
    fn is_shape(&self) -> bool { false }

    // ── Build: AST → Timeline ──
    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>>;

    // ── Render (optional) ──
    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> { None }

    // ── GUI defaults ──
    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> { vec![] }
}

// ONE static array. Everything else derives from this.
pub static PRIMITIVES: &[&dyn Primitive] = &[
    &rect::RECT, &circle::CIRCLE, &square::SQUARE,
    &ellipse::ELLIPSE, &line::LINE, &arc::ARC,
    &polygon::POLYGON, &regular_polygon::REGULAR_POLYGON,
    &path::PATH, &arrow::ARROW, &dot::DOT,
    &text::TEXT, &math::MATH, &code::CODE,
    &image::IMAGE, &svg::SVG,
    &plot::GRAPH, &plot::CARTESIAN_PLOT, &plot::POLAR_PLOT,
    &plot::PARAMETRIC_PLOT, &plot::IMPLICIT_PLOT,
    &row::ROW, &col::COL, &grid::GRID,
    &stack::STACK, &group::GROUP,
];
```

### Auto-generated dispatch

The registry in `primitives/mod.rs` auto-generates from `PRIMITIVES`:

1. **`ActorKindMeta` registry** — built once via `OnceLock`, derived from metadata methods
2. **`actor_kind_registry()`** — returns `'&static [ActorKindMeta]`
3. **`actor_kind_meta(kind)`** — lookup by `ActorKindId`
4. **`actor_kind_meta_by_name(name)`** — lookup by type name
5. **`find_primitive(type_name)`** — returns `Option<&'static dyn Primitive>`

### Per-primitive file

```rust
// primitives/triangle.rs
pub struct TrianglePrimitive;
pub const TRIANGLE: TrianglePrimitive = TrianglePrimitive;

impl Primitive for TrianglePrimitive {
    fn type_name(&self) -> &'static str { "Triangle" }
    fn display_name(&self) -> &'static str { "Triangle" }
    fn category(&self) -> ActorCategory { ActorCategory::Shape }
    fn icon_id(&self) -> &'static str { "triangle" }
    fn is_shape(&self) -> bool { true }

    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Shape(ShapeKind::Triangle)
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // Build logic: register actor, process properties, etc.
        ctx.timeline.process_inline_actor_decl(
            self.type_name(), label, props, modifiers,
            ctx.time_ms, ctx.parent_label,
        );
        Ok(())
    }

    fn render(&self, ctx: &RenderCtx
    ) -> Option<Vec<VelloPath>> {
        let path = build_triangle_path(ctx.state.size);
        Some(vec![build_vello_path(path, ctx.style)])
    }

    fn default_props(
        &self, scene: &SceneDimensions
    ) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![
                Expr::Num(scene.width as f64 / 2.0),
                Expr::Num(scene.height as f64 / 2.0),
            ])),
            Property::new("size", Expr::Tuple(vec![
                Expr::Num(100.0), Expr::Num(86.6)
            ])),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }
}
```

### New primitive: 3 touch points

| Step | File | What |
|------|------|------|
| 1 | `primitives/triangle.rs` | Implement `Primitive` trait (one file) |
| 2 | `primitives/mod.rs` | Add `&triangle::TRIANGLE` to `PRIMITIVES` array (one line) |
| 3 | `timeline/track.rs` | Add variant to `ActorKindId` and `ShapeKind` enums (still required for match arms) |

**Registry, dispatch, icon mapping, and GUI defaults are auto-generated.**

## Files

### Created

| File | Responsibility |
|------|---------------|
| `primitives/mod.rs` | `Primitive` trait, `PRIMITIVES` array, `find_primitive()`, auto-generated `ActorKindMeta` registry |
| `primitives/rect.rs` | Rectangle primitive |
| `primitives/circle.rs` | Circle primitive |
| `primitives/square.rs` | Square primitive |
| `primitives/line.rs` | Line primitive |
| `primitives/ellipse.rs` | Ellipse primitive |
| `primitives/arc.rs` | Arc primitive |
| `primitives/polygon.rs` | Polygon primitive |
| `primitives/regular_polygon.rs` | Regular polygon primitive |
| `primitives/path.rs` | Path primitive |
| `primitives/arrow.rs` | Arrow primitive |
| `primitives/dot.rs` | Dot primitive |
| `primitives/text.rs` | Text primitive |
| `primitives/math.rs` | Math primitive |
| `primitives/code.rs` | Code primitive |
| `primitives/image.rs` | Image primitive |
| `primitives/svg.rs` | SVG primitive |
| `primitives/plot.rs` | Plot primitives (Graph, CartesianPlot, PolarPlot, ParametricPlot, ImplicitPlot) |
| `primitives/row.rs` | Row container |
| `primitives/col.rs` | Column container |
| `primitives/grid.rs` | Grid container |
| `primitives/stack.rs` | Stack container |
| `primitives/group.rs` | Group container |

### Modified / Simplified

| File | Action |
|------|--------|
| `timeline/track.rs` | Removed hardcoded `ACTOR_KIND_REGISTRY`; kept `ActorKindId` / `ShapeKind` enums (still needed for match arms) |
| `timeline/actor_kind.rs` | Simplified: `find_actor_kind()` now delegates to `find_primitive()` via `PrimitiveActorKind` wrapper |
| `timeline/primitive.rs` | `PrimitiveDescriptor::for_actor_type()` now derives from `PRIMITIVES` registry |
| `timeline/shapes/primitives.rs` | Kept for legacy `VectorShapePrimitive` trait (still used by render pipeline) |
| `gui/icons.rs` | Now uses `actor_kind_registry()` auto-generated from `PRIMITIVES`; no hardcoded registry |
| `gui/actions/mod.rs` | Now uses `Primitive::default_props()` for GUI actor creation |

## Design Decisions

1. **Static trait objects (`&dyn Primitive`)** — simple, no generics, compile-time registry
2. **Metadata as methods, not data** — enables custom logic per primitive
3. **Optional `render()`** — non-shapes return `None`, no separate trait hierarchy
4. **`OnceLock` for lazy registry** — `ActorKindMeta` built on first access, no proc-macro needed
5. **Enums kept explicit** — `ActorKindId` and `ShapeKind` still require manual variants (used in match arms across codebase)

## Test Guarantees

- Every `PRIMITIVES` entry has unique `type_name`
- `find_primitive(type_name).type_name == type_name` round-trips for all entries
- Registry length matches `PRIMITIVES` length; all fields match
- Every `ActorKindId` variant has corresponding metadata in registry
- GUI icon tests verify every `icon_id` maps to a valid Phosphor glyph
