//! Equation family evaluation system (Equation, Fragment).

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::primitives::{EvaluateCtx, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;
use crate::timeline::{Timeline, TrackAccessor};

/// Evaluate draw commands for an `Equation` actor.
///
/// Returns an empty command list (`Some(vec![])`) so hit regions are registered
/// for the container while child fragments are rendered via equation layout.
pub fn evaluate_equation(
    _ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    Ok(Some(vec![]))
}

/// Evaluate draw commands for a `Fragment` actor.
///
/// Returns `None` as fragments do not render independently; their content
/// is aggregated by the parent `Equation`.
pub fn evaluate_fragment(
    _ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    Ok(None)
}

/// Custom build logic for `Fragment` actors.
pub fn build_extra_fragment(
    timeline: &mut Timeline,
    label: &str,
    _ty: &'static str,
    props: &[Property],
    _modifiers: &[Modifier],
    _children: &[InlineItem],
    time_ms: f64,
    _parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    let env = timeline.env().clone();

    let track_id = timeline.world.spawn(label, "Fragment");
    let meta = timeline.world.meta_mut(track_id);
    meta.first_seen_ms = meta.first_seen_ms.min(time_ms as u64);
    timeline.world.ensure_text(track_id);
    timeline.world.ensure_highlight(track_id);

    for prop in props {
        match prop.name.as_str() {
            "text" => {
                let content_str = match &prop.value {
                    Expr::Str(s) => s.clone(),
                    other => crate::timeline::evaluate_expr_with_lookup_diagnostic(
                        other,
                        &env,
                        diagnostics,
                        &format!("{label}.text"),
                    )
                    .map(|v| v.as_str().to_string())
                    .unwrap_or_default(),
                };
                timeline
                    .world
                    .ensure_text(track_id)
                    .text_content
                    .ensure(String::new())
                    .add_keyframe(time_ms as u64, content_str, Easing::Linear);
            },
            "highlight_color" => {
                if let Expr::Ident(name) = &prop.value
                    && name == "auto"
                {
                    timeline
                        .world
                        .ensure_highlight(track_id)
                        .highlight_color
                        .ensure([0.3, 0.5, 1.0, 1.0])
                        .add_keyframe(time_ms as u64, [0.3, 0.5, 1.0, 1.0], Easing::Linear);
                    continue;
                }
                if let Some(color) = crate::timeline::parse_color_in_env_with_lookup_diagnostic(
                    label,
                    "highlight_color",
                    &prop.value,
                    &env,
                    diagnostics,
                    label,
                ) {
                    timeline
                        .world
                        .ensure_highlight(track_id)
                        .highlight_color
                        .ensure([0.3, 0.5, 1.0, 1.0])
                        .add_keyframe(time_ms as u64, color, Easing::Linear);
                }
            },
            "highlight_opacity" | "highlight_padding" | "highlight_radius" | "fragment_order"
            | "fragment_color" => {
                use crate::timeline::property_engine::parse_property_value;
                use crate::timeline::property_registry::lookup_property;
                if let Some(schema) = lookup_property(&prop.name)
                    && let Some(value) = parse_property_value(
                        schema.value_type,
                        &prop.value,
                        &env,
                        diagnostics,
                        &format!("{label}.{}", prop.name),
                    )
                {
                    let field = schema.field_for_actor(&timeline.world.caps(track_id), "Fragment");
                    timeline.world.write_property_field(
                        track_id,
                        field,
                        value,
                        time_ms as u64,
                        time_ms as u64,
                        Easing::Linear,
                        diagnostics,
                    );
                }
            },
            "highlight_blend" => {
                if let Some(crate::timeline::Value::Str(blend)) =
                    crate::timeline::evaluate_expr_with_lookup_diagnostic(
                        &prop.value,
                        &env,
                        diagnostics,
                        label,
                    )
                {
                    timeline.world.ensure_highlight(track_id).highlight_blend =
                        crate::timeline::actions::highlight::parse_blend_mode(&blend);
                }
            },
            _ => {}, // Fragment content, paint and ordering consume the properties above.
        }
    }

    Ok(())
}
