//! Built-in action registry and execution.

use super::targets::lift_container_and_intermediate_containers;
use super::*;

pub(super) fn get_builtin_actions() -> Vec<Box<dyn BuiltinAction>> {
    vec![
        Box::new(FadeIn),
        Box::new(WipeIn),
        Box::new(Move),
        Box::new(Shift),
        Box::new(Rotate),
        Box::new(Scale),
        Box::new(DrawIn),
        Box::new(RevealIn),
        Box::new(DrawOut),
        Box::new(FadeOut),
        Box::new(Persist),
        Box::new(Remove),
        Box::new(WipeOut),
        Box::new(RevealOut),
        Box::new(Shake),
        Box::new(Pulse),
        Box::new(Bounce),
        Box::new(SettleIn),
        Box::new(PopIn),
        Box::new(Highlight),
        Box::new(Unhighlight),
        Box::new(Swap),
        Box::new(Reorder),
    ]
}

pub(super) fn execute_action(
    action: &Action,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
    handler: &dyn BuiltinAction,
) {
    let expanded_targets = expand_group_targets(timeline, &action.targets, &action.verb);
    if expanded_targets != action.targets {
        debug!(
            "Expanded group targets for '{}': {:?} -> {:?}",
            action.verb, action.targets, expanded_targets
        );
    }
    let mut expanded_action = action.clone();
    expanded_action.targets = expanded_targets;
    let signature = handler.signature();
    validate_action_modifiers(&expanded_action, &signature, diagnostics);
    handler.execute(&expanded_action, time_ms, timeline, diagnostics);

    if signature.category == "Entrance" {
        use crate::timeline::property_track::TrackAccessor;
        let parsed = crate::timeline::parse_timing_modifiers(
            &action.modifiers,
            crate::timeline::ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let t_start_ms = (time_ms + parsed.delay_ms) as u64;
        let t_end_ms = (time_ms + parsed.delay_ms + parsed.duration_ms) as u64;
        let easing = parsed.easing;

        for orig_target in &action.targets {
            lift_container_and_intermediate_containers(
                timeline,
                orig_target,
                t_start_ms,
                t_end_ms,
                easing,
            );
        }

        // Warn when an entrance targets an actor whose opacity remains 0.0
        // after entrance (e.g. an unlifted opacity track or an ancestor holding it invisible).
        for target in &expanded_action.targets {
            let target_opacity = timeline
                .world
                .tracks
                .get(target)
                .map_or(1.0, |t| t.style.opacity.get(t_end_ms, 1.0));
            if target_opacity <= 0.0 {
                diagnostics.push(
                    Diagnostic::warning(
                        crate::diagnostics::DiagnosticCode::NeverRevealed,
                        crate::diagnostics::DiagnosticPhase::Build,
                        format!(
                            "Entrance action '{}' targets '{}', but its opacity remains 0.0 after entrance.",
                            action.verb, target
                        ),
                    )
                    .with_subject(target)
                    .with_byte_span(action.byte_span),
                );
            }
        }
    }
}

/// Named modifier keys must be declared by the action's signature (or be
/// universal timing vocabulary). `parse_timing_modifiers` whitelists a set of
/// effect-key names (`intensity`, `color`, ...) so actions that declare them
/// don't warn — which also let undeclared keys pass silently
/// (`highlight ball [intensity: 1.3]` on the shipped gradient_descent example
/// was a no-op). This check turns those into diagnostics.
fn validate_action_modifiers(
    action: &Action,
    signature: &ActionSignature,
    diagnostics: &mut Vec<Diagnostic>,
) {
    const TIMING_KEYS: [&str; 2] = ["delay", "ease"];
    for modifier in &action.modifiers {
        let Some(name) = modifier.name.as_deref() else {
            continue; // positional duration shorthand
        };
        if TIMING_KEYS.contains(&name) {
            continue;
        }
        if signature.modifiers.iter().any(|p| p.name == name) {
            continue;
        }
        let declared = signature
            .modifiers
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::UnsupportedModifierKey,
                DiagnosticPhase::Build,
                format!(
                    "Action '{}' does not declare a '{}' modifier (declared: {}). The value is ignored.",
                    signature.name, name, declared
                ),
            )
            .with_subject(action.verb.as_str()),
        );
    }
}
