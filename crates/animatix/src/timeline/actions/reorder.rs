use super::registry::{ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::{ModifierHost, Timeline, parse_timing_modifiers};

pub struct Swap;

impl BuiltinAction for Swap {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "swap".to_string(),
            category: "Reorder".to_string(),
            description: "Swaps the layout positions of two children in their parent container.".to_string(),
            params: vec![],
            modifiers: base_timing_params(),
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Need exactly 2 targets
        if action.targets.len() != 2 {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidModifierValue,
                    DiagnosticPhase::Build,
                    format!(
                        "Swap action requires exactly 2 targets, got {}.",
                        action.targets.len()
                    ),
                )
                .with_subject(format!("{} {}", action.verb, action.targets.join(", "))),
            );
            return;
        }

        let child_a = &action.targets[0];
        let child_b = &action.targets[1];

        // Verify both targets exist
        for target in &action.targets {
            if !timeline.tracks.contains_key(target) {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnsupportedActionTarget,
                        DiagnosticPhase::Build,
                        format!(
                            "Swap action target '{}' is not declared yet.",
                            target
                        ),
                    )
                    .with_subject(target),
                );
                return;
            }
        }

        // Find common parent
        let parent = match timeline.find_common_parent(child_a, child_b) {
            Some(p) => p,
            None => {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnsupportedActionTarget,
                        DiagnosticPhase::Build,
                        format!(
                            "Swap targets '{}' and '{}' do not share a common parent container.",
                            child_a, child_b
                        ),
                    )
                    .with_subject(format!("{} {}, {}", action.verb, child_a, child_b)),
                );
                return;
            }
        };

        // Parse timing modifiers
        let parsed = parse_timing_modifiers(
            &action.modifiers,
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let easing = parsed.easing;

        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        // Check for overlapping swap on same container
        if let Some(track) = timeline.child_orders.get(&parent) {
            if let Some((&pending_time, _)) = track.keyframes.range(t_start_ms..).next() {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::ConflictingModifierKey,
                        DiagnosticPhase::Build,
                        format!(
                            "Swap action on '{}' overlaps with a pending swap that completes at {}ms. Swaps on the same container must not overlap in time.",
                            parent, pending_time
                        ),
                    )
                    .with_subject(format!("{} {}, {}", action.verb, child_a, child_b)),
                );
                return;
            }
        }

        // Get current order
        let current_order = timeline.get_child_order(&parent, t_start_ms);

        // Verify both children are in order
        let idx_a = match current_order.iter().position(|l| l == child_a) {
            Some(i) => i,
            None => {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnsupportedActionTarget,
                        DiagnosticPhase::Build,
                        format!(
                            "Swap target '{}' is not a layout-managed child of container '{}'.",
                            child_a, parent
                        ),
                    )
                    .with_subject(child_a),
                );
                return;
            }
        };

        let idx_b = match current_order.iter().position(|l| l == child_b) {
            Some(i) => i,
            None => {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnsupportedActionTarget,
                        DiagnosticPhase::Build,
                        format!(
                            "Swap target '{}' is not a layout-managed child of container '{}'.",
                            child_b, parent
                        ),
                    )
                    .with_subject(child_b),
                );
                return;
            }
        };

        // Create new order
        let mut new_order = current_order.clone();
        new_order.swap(idx_a, idx_b);

        // Set keyframe
        timeline
            .child_orders
            .entry(parent)
            .or_insert_with(|| crate::timeline::PropertyTrack::new(current_order))
            .add_keyframe(t_end_ms, new_order, easing);

        // Invalidate frame cache so next evaluation picks up the new order
        timeline.invalidate_frame_cache();
    }
}
