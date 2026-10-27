//! Assignment handling for plot primitives: func closure transitions and procedural parameters.

use crate::ast::Expr;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::plot::FuncSource;

pub(crate) fn handle_plot_assignment(
    property: &str,
    value: &Expr,
    ctx: &mut crate::primitives::AssignmentCtx,
    env: &crate::timeline::Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> bool {
    if property == "func" {
        // Evaluate RHS to a closure.
        let closure_val = match crate::timeline::evaluate_expr(value, env) {
            Ok(v) => v,
            Err(e) => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidPlotFunc,
                        DiagnosticPhase::Build,
                        format!("Failed to evaluate func assignment on '{}': {}", ctx.label, e),
                    )
                    .with_subject(subject),
                );
                return true;
            },
        };
        let (to_args, to_body, to_captures) = match closure_val {
            crate::timeline::Value::Closure(args, body, captures) => (args, *body, captures),
            _ => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidPlotFunc,
                        DiagnosticPhase::Build,
                        format!(
                            "func assignment on '{}' must be a closure (e.g. `(x) => expr`)",
                            ctx.label
                        ),
                    )
                    .with_subject(subject),
                );
                return true;
            },
        };

        // Determine "from" source:
        //   - If there's an active transition, record-and-chain: freeze current blend
        //   - Else use last completed transition's `to`, or the declaration func
        let plot = ctx.payload.as_mut().and_then(|p| p.as_plot_mut());
        let from_source = if let Some(active) =
            plot.as_ref().and_then(|p| p.func_transitions.last())
        {
            if ctx.time_ms >= active.start_ms && ctx.time_ms <= active.end_ms {
                // Record-and-chain: freeze current blend state
                let progress = if active.end_ms > active.start_ms {
                    ((ctx.time_ms - active.start_ms) as f64
                        / (active.end_ms - active.start_ms) as f64)
                        .clamp(0.0, 1.0)
                } else {
                    1.0
                };
                FuncSource::Blend {
                    from: Box::new(active.from.clone()),
                    to: Box::new(active.to.clone()),
                    frozen_progress: progress,
                }
            } else {
                // Last transition completed, use its `to`
                active.to.clone()
            }
        } else if let Some(procedural_plot) = plot.as_ref().and_then(|p| p.procedural_plot.as_ref())
        {
            // No prior transitions, use declaration func
            FuncSource::Compiled(
                procedural_plot.func_args.clone(),
                Box::new(procedural_plot.func_body.clone()),
                procedural_plot.extra_captures.clone(),
            )
        } else {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPlotFunc,
                    DiagnosticPhase::Build,
                    format!(
                        "Cannot assign func on '{}': no func declared on this plot actor",
                        ctx.label
                    ),
                )
                .with_subject(subject),
            );
            return true;
        };

        // Validate same arity
        if from_source.arity() != to_args.len() {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPlotFunc,
                    DiagnosticPhase::Build,
                    format!("func transition on '{}' must keep the same arity ", ctx.label),
                )
                .with_subject(subject),
            );
            return true;
        }

        // Push FuncTransition
        if let Some(plot) = ctx.payload.as_mut().and_then(|p| p.as_plot_mut()) {
            plot.func_transitions.push(crate::timeline::plot::FuncTransition {
                start_ms: ctx.t_start_ms,
                end_ms: ctx.t_end_ms,
                easing: ctx.easing,
                from: from_source,
                to: FuncSource::Compiled(to_args, Box::new(to_body), to_captures),
                blend_mode: ctx.func_blend_mode.unwrap_or_default(),
            });
        }
        return true;
    }

    // Check if this is a plot parameter assignment
    let is_plot_param = ctx
        .payload
        .as_ref()
        .and_then(|p| p.as_plot())
        .and_then(|p| p.procedural_plot.as_ref())
        .map(|p| p.param_names.iter().any(|n| n == property))
        .unwrap_or(false);

    if is_plot_param {
        let target_val = match crate::timeline::evaluate_expr(value, env) {
            Ok(crate::timeline::Value::Num(n)) => n,
            Ok(_) => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!("Plot parameter '{}' on '{}' must be numeric", property, ctx.label),
                    )
                    .with_subject(subject),
                );
                return true;
            },
            Err(e) => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!(
                            "Failed to evaluate plot parameter '{}.{}': {}",
                            ctx.label, property, e
                        ),
                    )
                    .with_subject(subject),
                );
                return true;
            },
        };

        let has_duration = ctx.t_end_ms > ctx.t_start_ms;
        let prop_name = property.to_string();
        if let Some(plot) = ctx.payload.as_mut().and_then(|p| p.as_plot_mut()) {
            let param_track = plot
                .plot_param_tracks
                .entry(prop_name)
                .or_insert_with(|| crate::timeline::PropertyTrack::new(target_val));

            if has_duration {
                let start_val = param_track.evaluate(ctx.t_start_ms);
                param_track.add_keyframe(ctx.t_start_ms, start_val, crate::easing::Easing::Linear);
            } else if ctx.instant_delayed && ctx.t_start_ms > 0 {
                let prev_val = param_track.evaluate(ctx.t_start_ms - 1);
                param_track.add_keyframe(
                    ctx.t_start_ms - 1,
                    prev_val,
                    crate::easing::Easing::Linear,
                );
            }
            param_track.add_keyframe(ctx.t_end_ms, target_val, ctx.easing);
        }
        return true;
    }

    false
}
