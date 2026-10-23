use super::registry::{ActionParam, ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::{Action, Modifier};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{ModifierHost, Timeline, Value, evaluate_expr, parse_timing_modifiers};

fn motion_timing_params() -> Vec<ActionParam> {
    let mut params = vec![
        ActionParam {
            name: "to".to_string(),
            description:
                "Target local translation offset for the move action (e.g. [to: (140, -40)])."
                    .to_string(),
            type_info: "vec2".to_string(),
        },
        ActionParam {
            name: "angle".to_string(),
            description: "Target rotation in radians for the rotate action. Use deg() to convert from degrees (e.g. [angle: deg(90)])."
                .to_string(),
            type_info: "number (radians)".to_string(),
        },
        ActionParam {
            name: "factor".to_string(),
            description: "Target scale factor for the scale action (e.g. [factor: 1.5])."
                .to_string(),
            type_info: "positive number".to_string(),
        },
        ActionParam {
            name: "along".to_string(),
            description: "Route to travel instead of a translation, as a list of path \
                          commands sampled by arc length (e.g. [along: {move_to(100, 100), \
                          line_to(500, 300)}]). Move verbs only."
                .to_string(),
            type_info: "path command list".to_string(),
        },
        ActionParam {
            name: "orient".to_string(),
            description: "With 'along', turn the actor to face its direction of travel."
                .to_string(),
            type_info: "bool".to_string(),
        },
        ActionParam {
            name: "anticipate".to_string(),
            description: "Counter-move window before the travel starts, e.g. [anticipate: 80ms]. \
                          The actor leans back by a fraction of its travel first."
                .to_string(),
            type_info: "time literal".to_string(),
        },
        ActionParam {
            name: "by".to_string(),
            description: "Relative translation delta for the shift action (e.g. [by: (40, -24)])."
                .to_string(),
            type_info: "vec2".to_string(),
        },
    ];
    params.extend(base_timing_params());
    params
}

fn push_shift_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    code: DiagnosticCode,
    message: impl Into<String>,
    subject: &str,
) {
    diagnostics.push(
        Diagnostic::warning(code, DiagnosticPhase::Build, message.into()).with_subject(subject),
    );
}

fn parse_vec2_modifier(
    modifiers: &[Modifier],
    timeline: &Timeline,
    key: &str,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<[f32; 2]> {
    let mut value = None;
    let mut saw_key = false;

    for modifier in modifiers {
        if modifier.name.as_deref() != Some(key) {
            continue;
        }

        if saw_key {
            push_shift_diagnostic(
                diagnostics,
                DiagnosticCode::ConflictingModifierKey,
                format!("Conflicting '{key}' modifiers on action; using the last value provided."),
                key,
            );
        }

        match evaluate_expr(&modifier.value, &timeline.env) {
            Ok(Value::Vec2([x, y])) => {
                value = Some([x as f32, y as f32]);
                saw_key = true;
            },
            Ok(_) | Err(_) => {
                push_shift_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    message,
                    key,
                );
            },
        }
    }

    if value.is_none() {
        push_shift_diagnostic(diagnostics, DiagnosticCode::InvalidModifierValue, message, key);
    }

    value
}

fn parse_num_modifier(
    modifiers: &[Modifier],
    timeline: &Timeline,
    key: &str,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<f32> {
    let mut value = None;
    let mut saw_key = false;

    for modifier in modifiers {
        if modifier.name.as_deref() != Some(key) {
            continue;
        }

        if saw_key {
            push_shift_diagnostic(
                diagnostics,
                DiagnosticCode::ConflictingModifierKey,
                format!("Conflicting '{key}' modifiers on action; using the last value provided."),
                key,
            );
        }

        match evaluate_expr(&modifier.value, &timeline.env) {
            Ok(Value::Num(n)) => {
                value = Some(n as f32);
                saw_key = true;
            },
            Ok(_) | Err(_) => {
                push_shift_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    message,
                    key,
                );
            },
        }
    }

    if value.is_none() {
        push_shift_diagnostic(diagnostics, DiagnosticCode::InvalidModifierValue, message, key);
    }

    value
}

fn parse_positive_num_modifier(
    modifiers: &[Modifier],
    timeline: &Timeline,
    key: &str,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<f32> {
    let value = parse_num_modifier(modifiers, timeline, key, message, diagnostics)?;
    if value <= 0.0 {
        push_shift_diagnostic(diagnostics, DiagnosticCode::InvalidModifierValue, message, key);
        return None;
    }
    Some(value)
}

pub(crate) fn timing_modifiers_without_keys(
    modifiers: &[Modifier],
    excluded_keys: &[&str],
) -> Vec<Modifier> {
    modifiers
        .iter()
        .filter(|modifier| {
            modifier.name.as_deref().is_none_or(|name| !excluded_keys.contains(&name))
        })
        .cloned()
        .collect()
}

/// Moves the target to an absolute local translation offset.
mod acts;
mod helpers;
mod move_action;
#[cfg(test)]
mod tests;

pub use acts::{Rotate, Scale, Shift};
pub use move_action::Move;
