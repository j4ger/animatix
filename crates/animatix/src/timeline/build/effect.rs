//! Effect-chain lowering for compositing scopes.
//!
//! Effect children of a `Filter` scope are lowered into the scope's
//! [`EffectChainTrack`](crate::timeline::effects::EffectChainTrack) instead of
//! becoming scene-graph actors: they own no `AnimationTrack`, no layout, and no
//! hit region, and are not rendered as content.

use super::*;

use crate::timeline::effects::{Effect, EffectStage, identity_to_property, value_to_property};
use crate::timeline::property_engine::PropertyValue;
use crate::timeline::timing::{ModifierHost, ParsedTimingModifiers, parse_timing_modifiers};

impl Timeline {
    /// Lower one effect declaration into its parent scope's chain.
    ///
    /// Emits a diagnostic and does nothing when the parent is not a `Filter`
    /// scope or a parameter is not declared by the effect.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_effect_stage(
        &mut self,
        parent_label: &str,
        label: &str,
        effect: &dyn Effect,
        props: &[crate::ast::Property],
        modifiers: &[Modifier],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let is_scope =
            self.tracks.get(parent_label).is_some_and(|track| track.caps.is_effect_scope());
        if !is_scope {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::UnknownActorType,
                    DiagnosticPhase::Build,
                    format!(
                        "Effect '{}' must be declared inside a Filter scope",
                        effect.type_name()
                    ),
                )
                .with_subject(label),
            );
            return;
        }

        let ParsedTimingModifiers {
            duration_ms,
            delay_ms,
            easing,
            ..
        } = parse_timing_modifiers(
            modifiers,
            ModifierHost::ActorDeclaration,
            Some(label),
            diagnostics,
        );
        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;
        let eval_env = self.build_eval_env(time_ms as u64);

        let mut stage = EffectStage::new(
            label.to_string(),
            crate::timeline::effects::EffectId::new(effect.type_name()),
        );
        for prop in props {
            let subject = format!("{}.{}", label, prop.name);

            if prop.name == "enabled" {
                match crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    &eval_env,
                    diagnostics,
                    &subject,
                ) {
                    Some(Value::Bool(enabled)) => {
                        if duration_ms > 0.0 {
                            let start = stage
                                .enabled
                                .sample(t_start_ms)
                                .unwrap_or(PropertyValue::Bool(true));
                            stage.enabled.add_keyframe_eased(t_start_ms, start, Easing::Linear);
                        }
                        stage.enabled.add_keyframe_eased(
                            t_end_ms,
                            PropertyValue::Bool(enabled),
                            easing,
                        );
                    },
                    Some(other) => tracing::warn!(
                        "{subject}: 'enabled' expects a bool, got {:?}; ignoring",
                        other
                    ),
                    None => {}, // Evaluation error already reported as a diagnostic.
                }
                continue;
            }

            let Some(spec) = effect.params().iter().find(|param| param.name == prop.name) else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!("Effect '{}' has no parameter '{}'", effect.type_name(), prop.name),
                    )
                    .with_subject(&subject),
                );
                continue;
            };

            let Some(value) = crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic(
                &prop.value,
                &eval_env,
                diagnostics,
                &subject,
            ) else {
                continue;
            };
            let Some(parsed) = value_to_property(spec.kind, value, &subject) else {
                continue;
            };

            let track = stage.param_track_mut(spec.name.as_ref(), spec.kind);
            if duration_ms > 0.0 {
                let start =
                    track.sample(t_start_ms).unwrap_or_else(|| identity_to_property(spec.identity));
                track.add_keyframe_eased(t_start_ms, start, Easing::Linear);
            }
            track.add_keyframe_eased(t_end_ms, parsed, easing);
        }

        // Re-borrow the parent mutably only now: `lower_effect_stage` read the
        // parent above, and the stage may need environment lookups in between.
        if let Some(parent) = self.tracks.get_mut(parent_label) {
            parent.effects.stages.push(stage);
        }
    }
}
