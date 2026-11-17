//! Shared parameter tables and modifier parsing for the motion actions.

use crate::ast::Modifier;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::{Timeline, Value, evaluate_expr};

pub(super) fn push_shift_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    code: DiagnosticCode,
    message: impl Into<String>,
    subject: &str,
) {
    diagnostics.push(
        Diagnostic::warning(code, DiagnosticPhase::Build, message.into()).with_subject(subject),
    );
}

pub(super) fn parse_vec2_modifier(
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

pub(super) fn parse_num_modifier(
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

pub(super) fn parse_positive_num_modifier(
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
