//! Plot primitives: graphs, curves, vector fields, heatmaps, contours, and number planes.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

/// The `Graph` plot primitive.
pub struct GraphPrimitive;

/// Singleton instance of [`GraphPrimitive`].
pub const GRAPH: GraphPrimitive = GraphPrimitive;

/// The `PlotCurve` plot primitive.
pub struct PlotCurvePrimitive;

/// Singleton instance of [`PlotCurvePrimitive`].
pub const PLOT_CURVE: PlotCurvePrimitive = PlotCurvePrimitive;

impl Primitive for GraphPrimitive {
    fn type_name(&self) -> &'static str { "Graph" }
    fn display_name(&self) -> &'static str { "Graph" }
    fn category(&self) -> ActorCategory { ActorCategory::Plot }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::CHART_BAR }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Graph }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        ]
    }
}

impl Primitive for PlotCurvePrimitive {
    fn type_name(&self) -> &'static str { "PlotCurve" }
    fn display_name(&self) -> &'static str { "Plot Curve" }
    fn category(&self) -> ActorCategory { ActorCategory::Plot }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::CHART_LINE_UP }
    fn is_advanced(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::PlotCurve }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("t_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("kind", Expr::Str("cartesian".into())),
            Property::new("func", Expr::Closure(vec!["x".into()], Box::new(Expr::Ident("x".into())))),
            Property::new("tolerance", Expr::Num(0.5)),
            Property::new("max_depth", Expr::Num(10.0)),
            Property::new("resolution", Expr::Num(96.0)),
        ]
    }
}

/// The `VectorField` plot primitive.
pub struct VectorFieldPrimitive;

/// Singleton instance of [`VectorFieldPrimitive`].
pub const VECTOR_FIELD: VectorFieldPrimitive = VectorFieldPrimitive;

impl Primitive for VectorFieldPrimitive {
    fn type_name(&self) -> &'static str { "VectorField" }
    fn display_name(&self) -> &'static str { "Vector Field" }
    fn category(&self) -> ActorCategory { ActorCategory::Plot }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::ARROWS_OUT_CARDINAL }
    fn is_advanced(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::VectorField }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("density", Expr::Num(16.0)),
            Property::new("func", Expr::Closure(
                vec!["x".into(), "y".into()],
                Box::new(Expr::Tuple(vec![Expr::Ident("x".into()), Expr::Ident("y".into())])),
            )),
        ]
    }
}

/// The `Heatmap` plot primitive.
pub struct HeatmapPrimitive;

/// Singleton instance of [`HeatmapPrimitive`].
pub const HEATMAP: HeatmapPrimitive = HeatmapPrimitive;

impl Primitive for HeatmapPrimitive {
    fn type_name(&self) -> &'static str { "Heatmap" }
    fn display_name(&self) -> &'static str { "Heatmap" }
    fn category(&self) -> ActorCategory { ActorCategory::Plot }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::GRADIENT }
    fn is_advanced(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Heatmap }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("resolution", Expr::Num(64.0)),
            Property::new("func", Expr::Closure(
                vec!["x".into(), "y".into()],
                Box::new(Expr::Num(0.0)),
            )),
        ]
    }
}

/// The `ContourSet` plot primitive.
pub struct ContourSetPrimitive;

/// Singleton instance of [`ContourSetPrimitive`].
pub const CONTOUR_SET: ContourSetPrimitive = ContourSetPrimitive;

impl Primitive for ContourSetPrimitive {
    fn type_name(&self) -> &'static str { "ContourSet" }
    fn display_name(&self) -> &'static str { "Contour Set" }
    fn category(&self) -> ActorCategory { ActorCategory::Plot }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::CHART_DONUT }
    fn is_advanced(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::ContourSet }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("resolution", Expr::Num(96.0)),
            Property::new("levels", Expr::Tuple(vec![Expr::Num(-2.0), Expr::Num(0.0), Expr::Num(2.0)])),
            Property::new("func", Expr::Closure(
                vec!["x".into(), "y".into()],
                Box::new(Expr::Num(0.0)),
            )),
        ]
    }
}

pub struct NumberPlanePrimitive;
/// Singleton instance of [`NumberPlanePrimitive`].
pub const NUMBER_PLANE: NumberPlanePrimitive = NumberPlanePrimitive;

impl Primitive for NumberPlanePrimitive {
    fn type_name(&self) -> &'static str { "NumberPlane" }
    fn display_name(&self) -> &'static str { "Number Plane" }
    fn category(&self) -> ActorCategory { ActorCategory::Plot }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::SQUARES_FOUR }
    fn kind_id(&self) -> ActorKindId { ActorKindId::NumberPlane }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_plot_actor_dispatch(
            label,
            self.type_name(),
            props,
            modifiers,
            children,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            Property::new("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            Property::new("x_range", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0), Expr::Num(2.0)])),
            Property::new("y_range", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0), Expr::Num(2.0)])),
        ]
    }
}
