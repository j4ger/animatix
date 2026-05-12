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

pub struct CartesianPlotPrimitive;
pub const CARTESIAN_PLOT: CartesianPlotPrimitive = CartesianPlotPrimitive;

pub struct PolarPlotPrimitive;
pub const POLAR_PLOT: PolarPlotPrimitive = PolarPlotPrimitive;

pub struct ParametricPlotPrimitive;
pub const PARAMETRIC_PLOT: ParametricPlotPrimitive = ParametricPlotPrimitive;

pub struct ImplicitPlotPrimitive;
pub const IMPLICIT_PLOT: ImplicitPlotPrimitive = ImplicitPlotPrimitive;

macro_rules! impl_plot_primitive {
    ($ty:ident, $type_name:literal, $display_name:literal, $kind:expr, $icon:literal, $advanced:expr, $defaults:expr) => {
        impl Primitive for $ty {
            fn type_name(&self) -> &'static str { $type_name }
            fn display_name(&self) -> &'static str { $display_name }
            fn category(&self) -> ActorCategory { ActorCategory::Plot }
            fn icon_id(&self) -> &'static str { $icon }
            fn is_advanced(&self) -> bool { $advanced }
            fn kind_id(&self) -> ActorKindId { $kind }

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
                $defaults
            }
        }
    };
}

impl_plot_primitive!(
    GraphPrimitive,
    "Graph",
    "Graph",
    ActorKindId::Graph,
    "chart-bar",
    false,
    vec![
        property("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
        property("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
        property("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        property("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
    ]
);

impl_plot_primitive!(
    CartesianPlotPrimitive,
    "CartesianPlot",
    "Cartesian Plot",
    ActorKindId::CartesianPlot,
    "chart-line-up",
    true,
    vec![
        property("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
        property("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
        property("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        property("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        property("t_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        property("func", Expr::Closure(vec!["x".into()], Box::new(Expr::Ident("x".into())))),
        property("tolerance", Expr::Num(0.5)),
        property("max_depth", Expr::Num(10.0)),
        property("resolution", Expr::Num(96.0)),
    ]
);

impl_plot_primitive!(
    PolarPlotPrimitive,
    "PolarPlot",
    "Polar Plot",
    ActorKindId::PolarPlot,
    "chart-polar",
    true,
    vec![
        property("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
        property("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
        property("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        property("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        property("t_domain", Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(std::f64::consts::TAU)])),
        property("func", Expr::Closure(vec!["t".into()], Box::new(Expr::Num(1.0)))),
        property("tolerance", Expr::Num(0.5)),
        property("max_depth", Expr::Num(10.0)),
        property("resolution", Expr::Num(96.0)),
    ]
);

impl_plot_primitive!(
    ParametricPlotPrimitive,
    "ParametricPlot",
    "Parametric Plot",
    ActorKindId::ParametricPlot,
    "chart-scatter",
    true,
    vec![
        property("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
        property("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
        property("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        property("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        property("t_domain", Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(std::f64::consts::TAU)])),
        property(
            "func",
            Expr::Closure(
                vec!["t".into()],
                Box::new(Expr::Tuple(vec![Expr::Ident("t".into()), Expr::Num(0.0)])),
            ),
        ),
        property("tolerance", Expr::Num(0.5)),
        property("max_depth", Expr::Num(10.0)),
        property("resolution", Expr::Num(96.0)),
    ]
);

impl_plot_primitive!(
    ImplicitPlotPrimitive,
    "ImplicitPlot",
    "Implicit Plot",
    ActorKindId::ImplicitPlot,
    "chart-donut",
    true,
    vec![
        property("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
        property("size", Expr::Tuple(vec![Expr::Num(500.0), Expr::Num(500.0)])),
        property("x_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        property("y_domain", Expr::Tuple(vec![Expr::Num(-10.0), Expr::Num(10.0)])),
        property("func", Expr::Closure(vec!["x".into(), "y".into()], Box::new(Expr::Num(0.0)))),
        property("tolerance", Expr::Num(0.5)),
        property("max_depth", Expr::Num(10.0)),
        property("resolution", Expr::Num(96.0)),
    ]
);
