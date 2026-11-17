//! Built-in action registry and execution.

use super::targets::lift_container_and_intermediate_containers;
use super::*;

/// Static identity of every built-in action.
///
/// Built-ins are unit structs implementing [`BuiltinAction`]. Keeping their
/// identities in an enum makes the registry a `match` over a `'static` table
/// instead of constructing a `Vec<Box<dyn BuiltinAction>>` for every lookup.
/// `ALL` preserves the historical registry order used by
/// [`super::get_action_signatures`].
use animatix_std::callable::TimelineBuiltin as BuiltinActionKind;

impl BuiltinAction for BuiltinActionKind {
    fn signature(&self) -> CallableDescriptor {
        self.descriptor().clone()
    }

    fn execute(
        &self,
        action: &BuiltinInvocation,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match self {
            Self::FadeIn => FadeIn.execute(action, time_ms, timeline, diagnostics),
            Self::WipeIn => WipeIn.execute(action, time_ms, timeline, diagnostics),
            Self::Move => Move.execute(action, time_ms, timeline, diagnostics),
            Self::Shift => Shift.execute(action, time_ms, timeline, diagnostics),
            Self::Rotate => Rotate.execute(action, time_ms, timeline, diagnostics),
            Self::Scale => Scale.execute(action, time_ms, timeline, diagnostics),
            Self::DrawIn => DrawIn.execute(action, time_ms, timeline, diagnostics),
            Self::RevealIn => RevealIn.execute(action, time_ms, timeline, diagnostics),
            Self::DrawOut => DrawOut.execute(action, time_ms, timeline, diagnostics),
            Self::FadeOut => FadeOut.execute(action, time_ms, timeline, diagnostics),
            Self::Persist => Persist.execute(action, time_ms, timeline, diagnostics),
            Self::Remove => Remove.execute(action, time_ms, timeline, diagnostics),
            Self::WipeOut => WipeOut.execute(action, time_ms, timeline, diagnostics),
            Self::RevealOut => RevealOut.execute(action, time_ms, timeline, diagnostics),
            Self::Shake => Shake.execute(action, time_ms, timeline, diagnostics),
            Self::Pulse => Pulse.execute(action, time_ms, timeline, diagnostics),
            Self::Bounce => Bounce.execute(action, time_ms, timeline, diagnostics),
            Self::SettleIn => SettleIn.execute(action, time_ms, timeline, diagnostics),
            Self::PopIn => PopIn.execute(action, time_ms, timeline, diagnostics),
            Self::GrowIn => GrowIn.execute(action, time_ms, timeline, diagnostics),
            Self::ShrinkOut => ShrinkOut.execute(action, time_ms, timeline, diagnostics),
            Self::Highlight => Highlight.execute(action, time_ms, timeline, diagnostics),
            Self::Unhighlight => Unhighlight.execute(action, time_ms, timeline, diagnostics),
            Self::Swap => Swap.execute(action, time_ms, timeline, diagnostics),
            Self::Reorder => Reorder.execute(action, time_ms, timeline, diagnostics),
        }
    }
}

pub(super) fn execute_action<H: BuiltinAction + ?Sized>(
    action: &BuiltinInvocation,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
    handler: &H,
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
        let timing = super::ActionTiming::parse(action, time_ms, diagnostics);
        let t_start_ms = timing.t_start_ms;
        let t_end_ms = timing.t_end_ms;
        let easing = timing.easing;

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
                .get_id(target)
                .map_or(1.0, |id| timeline.world.style(id).opacity.get(t_end_ms, 1.0));
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
    action: &BuiltinInvocation,
    signature: &CallableDescriptor,
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
        if signature.params.iter().any(|p| p.name == name) {
            continue;
        }
        let declared =
            signature.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::UnsupportedModifierKey,
                DiagnosticPhase::Build,
                format!(
                    "Function '{}' does not declare a '{}' modifier (declared: {}). The value is ignored.",
                    signature.name, name, declared
                ),
            )
            .with_subject(action.verb.as_str()),
        );
    }
}
