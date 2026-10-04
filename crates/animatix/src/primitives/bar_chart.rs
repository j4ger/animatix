//! Bar chart / column chart primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::primitives::{
    AssignmentCtx, BuildCtx, EvaluateCtx, Primitive, RenderCommand, TextCompileCtx,
};
use crate::renderer::error::RenderError;
use crate::timeline::AnimationTrack;
use crate::timeline::{BarDataTransition, parse_bar_chart_data_expr};
use crate::timeline::{Environment, SceneDimensions};

/// The `BarChart` primitive.
pub struct BarChartPrimitive;

/// Singleton instance of `BarChartPrimitive`.
pub const BAR_CHART: BarChartPrimitive = BarChartPrimitive;

impl Primitive for BarChartPrimitive {
    fn type_name(&self) -> &str {
        "BarChart"
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
        track: &mut AnimationTrack,
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        _env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
    ) -> bool {
        if property != "data" {
            return false;
        }
        let to = parse_bar_chart_data_expr(value, diagnostics, &track.label);
        // Chain like `func` does: the value in effect is the previous
        // transition's target, or the declaration when there is none.
        let from = track
            .bar_data_transitions
            .last()
            .map_or_else(|| track.bar_data.clone(), |previous| previous.to.clone());
        // Bar *captions* are compiled once at build, into the slots the
        // declaration's dataset laid out, so the two things a transition can get
        // wrong about them are both worth saying out loud. Note the actor's own
        // name in the message: `subject` is the assignment target (`chart.data`),
        // which reads as though `data` were the chart's label.
        let same_labels =
            from.len() == to.len() && from.iter().zip(&to).all(|(left, right)| left.0 == right.0);
        // Both guards are about captions, so a chart that asks for none
        // (`show_labels: false`) is free to re-order or start empty.
        let captions_expected = track.bar_layout.as_ref().is_some_and(|l| l.has_captions());
        if captions_expected && from.is_empty() {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "BarChart '{}' declares no `data:`, so no bar captions were compiled at \
                         build time; the assigned bars will draw without labels. Declare the \
                         first dataset with `data:` and assign the later ones.",
                        track.label
                    ),
                )
                .with_subject(subject),
            );
        } else if captions_expected && !same_labels {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "BarChart '{}' assigns a different label set or order; bar captions are \
                         compiled at build time into the declaration's slots, so the values will \
                         interpolate but the captions will not follow. Keep the same labels in \
                         the same order to animate a chart's values.",
                        track.label
                    ),
                )
                .with_subject(subject),
            );
        }
        track.bar_data_transitions.push(BarDataTransition {
            start_ms: ctx.t_start_ms,
            end_ms: ctx.t_end_ms,
            easing: ctx.easing,
            from,
            to,
        });
        true
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(960.0), Expr::Num(540.0)])),
            Property::new("size", Expr::Tuple(vec![Expr::Num(600.0), Expr::Num(300.0)])),
        ]
    }
}
