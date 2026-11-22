//! Bar chart / column chart primitive.

use crate::ast::{Expr, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{AssignmentCtx, EvaluateCtx, Primitive, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;
use crate::timeline::{BarDataTransition, Environment, SceneDimensions, parse_bar_chart_data_expr};

/// The `BarChart` primitive.
pub struct BarChartPrimitive;

/// Singleton instance of `BarChartPrimitive`.
pub const BAR_CHART: BarChartPrimitive = BarChartPrimitive;

impl Primitive for BarChartPrimitive {
    fn type_name(&self) -> &str {
        "BarChart"
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        crate::timeline::behavior::builtin::plot::evaluate_plot(ctx, text_ctx)
    }

    /// `data = {…}` on a `BarChart`: record the dataset pair as a transition
    /// instead of trying to interpolate a list of tuples.
    ///
    /// Bars are matched **by label** at frame time (see
    /// [`crate::timeline::plot::interpolate_bar_data`]), so a label that only the
    /// new set has enters from height 0 and one it drops leaves toward 0 — the
    /// chart reads as bars racing and falling away, not as the whole row sliding.
    /// The declaration's layout stays on the track for the same reason: rebuilding
    /// paths every frame must not re-parse properties or re-evaluate expressions.
    fn handle_assignment(
        &self,
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
    ) -> bool {
        if property != "data" {
            return false;
        }
        let Some(to) = parse_bar_chart_data_expr(value, env, diagnostics, ctx.label) else {
            return true;
        };
        // Chain like `func` does: the value in effect is the previous
        // transition's target, or the declaration when there is none.
        let from = ctx
            .bar_data_transitions()
            .last()
            .map_or_else(|| ctx.bar_data().to_vec(), |previous| previous.to.clone());
        let transition = BarDataTransition {
            start_ms: ctx.t_start_ms,
            end_ms: ctx.t_end_ms,
            easing: ctx.easing,
            from,
            to,
        };
        ctx.ensure_plot_payload().bar_data_transitions.push(transition);
        true
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(600.0), Expr::Num(300.0)])),
        ]
    }
}
