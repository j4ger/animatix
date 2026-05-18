use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, Primitive};
use crate::timeline::SceneDimensions;

fn property(name: &str, value: Expr) -> Property {
    Property {
        name: name.into(),
        value,
        value_span: None,
        trailing_comment: None,
    }
}

pub struct GraphPrimitive;
pub const GRAPH: GraphPrimitive = GraphPrimitive;

pub struct PlotCurvePrimitive;
pub const PLOT_CURVE: PlotCurvePrimitive = PlotCurvePrimitive;

impl Primitive for GraphPrimitive {
    fn type_name(&self) -> &'static str { "Graph" }
    fn display_name(&self) -> &'static str { "Graph" }
    fn category(&self) -> ActorCategory { ActorCategory::Plot }
    fn icon_id(&self) -> &'static str { "chart-bar" }
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
            property("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            property("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            property("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            property("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        ]
    }
}

impl Primitive for PlotCurvePrimitive {
    fn type_name(&self) -> &'static str { "PlotCurve" }
    fn display_name(&self) -> &'static str { "Plot Curve" }
    fn category(&self) -> ActorCategory { ActorCategory::Plot }
    fn icon_id(&self) -> &'static str { "chart-line-up" }
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
            property("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            property("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
            property("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            property("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            property("t_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
            property("kind", Expr::Str("cartesian".into())),
            property("func", Expr::Closure(vec!["x".into()], Box::new(Expr::Ident("x".into())))),
            property("tolerance", Expr::Num(0.5)),
            property("max_depth", Expr::Num(10.0)),
            property("resolution", Expr::Num(96.0)),
        ]
    }
}
