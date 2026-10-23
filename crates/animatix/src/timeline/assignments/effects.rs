//! Effect-stage parameter assignment helpers.

use super::super::{Diagnostic, Easing, Environment, Timeline, Value};
use crate::ast::Expr;
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};

/// Write a keyframed assignment for an effect stage parameter
/// (`scope.stage.param = value`).
///
/// Reports a diagnostic (never silently drops) when the property is not a
/// declared parameter of the stage's effect.
#[allow(clippy::too_many_arguments)]
pub(super) fn write_effect_stage_param(
    timeline: &mut Timeline,
    scope_label: &str,
    stage_label: &str,
    kind: crate::timeline::effects::EffectId,
    property: &str,
    value: &Expr,
    eval_env: &Environment,
    t_start_ms: u64,
    t_end_ms: u64,
    duration_ms: f64,
    easing: Easing,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) {
    use crate::timeline::effects::{EffectParamKind, effect};
    use crate::timeline::property_engine::PropertyValue;

    let Some(effect) = effect(&kind) else {
        tracing::warn!("effect stage '{scope_label}.{stage_label}' has no registered descriptor");
        return;
    };
    let (is_enabled, param_kind, identity) = if property == "enabled" {
        (true, EffectParamKind::Bool, PropertyValue::Bool(true))
    } else if let Some(spec) = effect.params().iter().find(|param| param.name == property) {
        (false, spec.kind, crate::timeline::effects::identity_to_property(spec.identity))
    } else {
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::InvalidPropertyValue,
                DiagnosticPhase::Build,
                format!("Effect '{}' has no parameter '{}'", effect.type_name(), property),
            )
            .with_subject(subject),
        );
        return;
    };

    let Some(evaluated) = crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic(
        value,
        eval_env,
        diagnostics,
        subject,
    ) else {
        return;
    };
    let parsed = if is_enabled {
        match evaluated {
            Value::Bool(enabled) => PropertyValue::Bool(enabled),
            other => {
                tracing::warn!("{subject}: 'enabled' expects a bool, got {other:?}; ignoring");
                return;
            },
        }
    } else {
        match crate::timeline::effects::value_to_property(param_kind, evaluated, subject) {
            Some(v) => v,
            None => return,
        }
    };

    let Some(scope) = timeline.world.tracks.get_mut(scope_label) else {
        return;
    };
    let Some(stage) = scope.effects.stage_mut(stage_label) else {
        return;
    };
    let track = if is_enabled {
        &mut stage.enabled
    } else {
        stage.param_track_mut(property, param_kind)
    };
    if duration_ms > 0.0 {
        let start = track.sample(t_start_ms).unwrap_or(identity);
        track.add_keyframe_eased(t_start_ms, start, Easing::Linear);
    }
    track.add_keyframe_eased(t_end_ms, parsed, easing);
}
