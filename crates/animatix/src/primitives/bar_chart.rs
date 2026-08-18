//! Bar chart / column chart primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{
    ActorCategory, ActorKindId, BuildCtx, EvaluateCtx, Primitive, RenderCommand, TextCompileCtx,
};
use crate::renderer::error::RenderError;
use crate::timeline::SceneDimensions;

/// The `BarChart` primitive.
pub struct BarChartPrimitive;

/// Singleton instance of `BarChartPrimitive`.
pub const BAR_CHART: BarChartPrimitive = BarChartPrimitive;

impl Primitive for BarChartPrimitive {
    fn type_name(&self) -> &str {
        "BarChart"
    }

    fn display_name(&self) -> &str {
        "Bar Chart"
    }

    fn category(&self) -> ActorCategory {
        ActorCategory::Plot
    }

    fn icon_id(&self) -> &str {
        crate::icon_glyphs::CHART_BAR
    }

    fn kind_id(&self) -> ActorKindId {
        ActorKindId::BarChart
    }

    fn is_shape(&self) -> bool {
        false
    }

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

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        _text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        if ctx.vector_paths.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vec![RenderCommand::Paths {
                paths: ctx.vector_paths.to_vec(),
            }]))
        }
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(600.0), Expr::Num(300.0)])),
        ]
    }
}
