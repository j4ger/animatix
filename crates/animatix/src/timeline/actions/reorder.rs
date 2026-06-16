use super::registry::{ActionParam, ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::{Action, Expr, Modifier};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::{Easing, ModifierHost, Timeline, parse_timing_modifiers};

/// Swaps the layout positions of two children in their parent container.
pub struct Swap;
/// Reorders all children of a container to a specified sequence.
pub struct Reorder;

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
                            "Swap action target '{}' is not declared.",
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

        // Set keyframes: hold current order at start time, then transition to new order
        let track = timeline
            .child_orders
            .entry(parent)
            .or_insert_with(|| crate::timeline::PropertyTrack::new(current_order.clone()));
        track.add_keyframe(t_start_ms, current_order, Easing::Linear);
        track.add_keyframe(t_end_ms, new_order, easing);

        // Invalidate frame cache so next evaluation picks up the new order
        timeline.invalidate_frame_cache();
    }
}

impl BuiltinAction for Reorder {
    fn signature(&self) -> ActionSignature {
        let mut modifiers = base_timing_params();
        modifiers.push(ActionParam {
            name: "order".to_string(),
            description: "New child order as a list of labels (e.g. [order: (c, b, a)]).".to_string(),
            type_info: "list of identifiers".to_string(),
        });
        ActionSignature {
            name: "reorder".to_string(),
            category: "Reorder".to_string(),
            description: "Reorders all children of a container to a specified order.".to_string(),
            params: vec![],
            modifiers,
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Need exactly 1 target (the container)
        if action.targets.len() != 1 {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidModifierValue,
                    DiagnosticPhase::Build,
                    format!(
                        "Reorder action requires exactly 1 target (the container), got {}.",
                        action.targets.len()
                    ),
                )
                .with_subject(format!("{} {}", action.verb, action.targets.join(", "))),
            );
            return;
        }

        let container = &action.targets[0];

        // Verify container exists
        if !timeline.tracks.contains_key(container) {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnsupportedActionTarget,
                    DiagnosticPhase::Build,
                    format!(
                        "Reorder action target '{}' is not declared.",
                        container
                    ),
                )
                .with_subject(container),
            );
            return;
        }

        // Parse the order modifier
        let mut order_expr: Option<&Expr> = None;
        for modifier in &action.modifiers {
            if modifier.name.as_deref() == Some("order") {
                order_expr = Some(&modifier.value);
                break;
            }
        }

        let new_order = match order_expr {
            Some(Expr::List(items)) => {
                let mut labels = Vec::new();
                for item in items {
                    match item {
                        Expr::Ident(label) | Expr::Str(label) => labels.push(label.clone()),
                        _ => {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::InvalidModifierValue,
                                    DiagnosticPhase::Build,
                                    "Reorder 'order' modifier must be a list of identifier labels (e.g. (c, b, a))."
                                        .to_string(),
                                )
                                .with_subject(format!("{} {}", action.verb, container)),
                            );
                            return;
                        }
                    }
                }
                labels
            }
            _ => {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidModifierValue,
                        DiagnosticPhase::Build,
                        "Reorder action requires an 'order' modifier with a list of labels (e.g. [order: (c, b, a)])."
                            .to_string(),
                    )
                    .with_subject(format!("{} {}", action.verb, container)),
                );
                return;
            }
        };

        // Get current order to validate against
        let current_order = timeline.get_child_order(container, time_ms as u64);

        // Validate: the new order must contain exactly the same children
        let current_set: std::collections::HashSet<_> = current_order.iter().collect();
        let new_set: std::collections::HashSet<_> = new_order.iter().collect();

        if current_set != new_set {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidModifierValue,
                    DiagnosticPhase::Build,
                    format!(
                        "Reorder 'order' must contain exactly the same children as the container '{}'. Expected {:?}, got {:?}.",
                        container, current_order, new_order
                    ),
                )
                .with_subject(format!("{} {}", action.verb, container)),
            );
            return;
        }

        // Parse timing modifiers (excluding 'order')
        let timing_modifiers: Vec<Modifier> = action
            .modifiers
            .iter()
            .filter(|m| m.name.as_deref() != Some("order"))
            .cloned()
            .collect();

        let parsed = parse_timing_modifiers(
            &timing_modifiers,
            ModifierHost::Action,
            Some(&action.verb),
            diagnostics,
        );
        let duration_ms = parsed.duration_ms;
        let delay_ms = parsed.delay_ms;
        let easing = parsed.easing;

        let t_start_ms = (time_ms + delay_ms) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        // Check for overlapping reorder on same container
        if let Some(track) = timeline.child_orders.get(container) {
            if let Some((&pending_time, _)) = track.keyframes.range(t_start_ms..).next() {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::ConflictingModifierKey,
                        DiagnosticPhase::Build,
                        format!(
                            "Reorder action on '{}' overlaps with a pending reorder that completes at {}ms. Reorders on the same container must not overlap in time.",
                            container, pending_time
                        ),
                    )
                    .with_subject(format!("{} {}", action.verb, container)),
                );
                return;
            }
        }

        // Set keyframes: hold current order at start time, then transition to new order
        let track = timeline
            .child_orders
            .entry(container.clone())
            .or_insert_with(|| crate::timeline::PropertyTrack::new(current_order.clone()));
        track.add_keyframe(t_start_ms, current_order, Easing::Linear);
        track.add_keyframe(t_end_ms, new_order, easing);

        // Invalidate frame cache so next evaluation picks up the new order
        timeline.invalidate_frame_cache();
    }
}
