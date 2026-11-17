use super::registry::{BuiltinAction, CallableDescriptor};
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::primitives::ChildProcessing;
use crate::timeline::Timeline;
use crate::timeline::actions::BuiltinInvocation;
use crate::timeline::property_track::TrackAccessor;

/// Parse a blend mode string into a `vello::peniko::Mix` variant.
fn parse_blend_mode(s: &str) -> vello::peniko::Mix {
    match s {
        "difference" => vello::peniko::Mix::Difference,
        "exclusion" => vello::peniko::Mix::Exclusion,
        "normal" => vello::peniko::Mix::Normal,
        "multiply" => vello::peniko::Mix::Multiply,
        "screen" => vello::peniko::Mix::Screen,
        "overlay" => vello::peniko::Mix::Overlay,
        _ => vello::peniko::Mix::Difference,
    }
}

fn is_equation_parent(timeline: &Timeline, id: crate::timeline::EntityId) -> bool {
    timeline
        .primitive_registry
        .info_of(timeline.world.actor_type(id))
        .is_some_and(|info| info.child_processing == ChildProcessing::Equation)
}

/// Find the parent Equation track label for a given Fragment label, if any.
///
/// This enables exclusive group behavior: when a Fragment inside an Equation
/// is highlighted, sibling Fragments are automatically unhighlighted so that
/// only one Fragment (or one group via multi-target) is highlighted at a time.
fn find_equation_parent(timeline: &Timeline, fragment: &str) -> Option<String> {
    let fragment_id = timeline.get_entity_id(fragment)?;
    timeline
        .world
        .tracks
        .iter()
        .find(|(_, t)| {
            is_equation_parent(timeline, *t)
                && timeline.world.hierarchy(*t).children.contains(&fragment_id)
        })
        .map(|(k, _)| k.clone())
}

/// Highlight action fades in a colored overlay behind equation fragments.
pub struct Highlight;

impl BuiltinAction for Highlight {
    fn signature(&self) -> CallableDescriptor {
        animatix_std::callable::TimelineBuiltin::Highlight.descriptor().clone()
    }

    fn execute(
        &self,
        action: &BuiltinInvocation,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let timing = super::ActionTiming::parse(action, time_ms, diagnostics);
        let easing = timing.easing;

        let t_start_ms = timing.t_start_ms;
        let t_end_ms = timing.t_end_ms;

        // Parse optional color modifier (default: white [1,1,1,1])
        let color = if let Some(modifier) =
            action.modifiers.iter().find(|m| m.name.as_deref() == Some("color"))
        {
            match crate::timeline::utils::resolve_color_in_env(&modifier.value, &timeline.env) {
                Ok(Some(color)) => Some(color),
                result => {
                    tracing::warn!(?result, "highlight color could not be resolved");
                    diagnostics.push(crate::diagnostics::Diagnostic::error(
                        crate::diagnostics::DiagnosticCode::InvalidModifierValue,
                        crate::diagnostics::DiagnosticPhase::Build,
                        "highlight color must be a valid color value or color string",
                    ));
                    return;
                },
            }
        } else {
            None
        };

        // Parse optional blend modifier (default: "difference")
        let blend = action
            .modifiers
            .iter()
            .find(|m| m.name.as_deref() == Some("blend"))
            .and_then(|m| {
                crate::timeline::evaluate_expr(&m.value, &timeline.env)
                    .ok()
                    .map(|v| parse_blend_mode(&v.as_str()))
            })
            .unwrap_or(vello::peniko::Mix::Difference);

        // Parse optional padding modifier
        let padding = action
            .modifiers
            .iter()
            .find(|m| m.name.as_deref() == Some("padding"))
            .and_then(|m| {
                crate::timeline::evaluate_expr(&m.value, &timeline.env)
                    .ok()
                    .map(|v| v.as_num() as f32)
            });

        // Parse optional radius modifier
        let radius = action
            .modifiers
            .iter()
            .find(|m| m.name.as_deref() == Some("radius"))
            .and_then(|m| {
                crate::timeline::evaluate_expr(&m.value, &timeline.env)
                    .ok()
                    .map(|v| v.as_num() as f32)
            });

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }

            let Some(id) = timeline.world.get_id(target) else {
                continue;
            };

            // Set highlight color keyframe if specified
            let hl = timeline.world.ensure_highlight(id);
            if let Some(c) = color {
                hl.highlight_color.ensure([1.0, 1.0, 1.0, 1.0]).add_keyframe(
                    t_start_ms,
                    c,
                    Easing::Linear,
                );
            }

            // Set highlight padding keyframe if specified
            if let Some(p) = padding {
                hl.highlight_padding.ensure(4.0).add_keyframe(t_start_ms, p, Easing::Linear);
            }

            // Set highlight radius keyframe if specified
            if let Some(r) = radius {
                hl.highlight_radius.ensure(2.0).add_keyframe(t_start_ms, r, Easing::Linear);
            }

            // Set blend mode (non-animated configuration value)
            hl.highlight_blend = blend;

            // Animate highlight opacity: 0 → 1
            let start_opacity = hl.highlight_opacity.get(t_start_ms, 0.0);
            hl.highlight_opacity.ensure(0.0).add_keyframe(
                t_start_ms,
                start_opacity,
                Easing::Linear,
            );
            hl.highlight_opacity.ensure(0.0).add_keyframe(t_end_ms, 1.0, easing);
        }

        // ── Exclusive group logic ──────────────────────────────────
        // When highlighting Fragments inside an Equation, auto-unhighlight
        // sibling Fragments that are NOT in the current action targets.
        // This ensures only one Fragment (or one group via multi-target)
        // is highlighted at a time.  Different Equations are independent.
        //
        // Two-pass pattern to avoid borrow checker issues:
        //   Pass 1: read from timeline, collect sibling info (owned strings)
        //   Pass 2: mutate sibling tracks
        let siblings_to_unhighlight: Vec<(String, u64, u64)> = {
            let target_set: std::collections::HashSet<String> =
                action.targets.iter().cloned().collect();
            let mut siblings = Vec::new();
            for target in &action.targets {
                if let Some(parent_label) = find_equation_parent(timeline, target)
                    && let Some(parent_id) = timeline.get_entity_id(&parent_label)
                {
                    for child in timeline.world.child_labels(parent_id) {
                        if !target_set.contains(child) {
                            siblings.push((child.to_string(), t_start_ms, t_end_ms));
                        }
                    }
                }
            }
            siblings
        };
        for (sibling, t_start, t_end) in siblings_to_unhighlight {
            if let Some(id) = timeline.world.get_id(&sibling) {
                let hl = timeline.world.ensure_highlight(id);
                let start_opacity = hl.highlight_opacity.get(t_start, 1.0);
                hl.highlight_opacity.ensure(0.0).add_keyframe(
                    t_start,
                    start_opacity,
                    Easing::Linear,
                );
                hl.highlight_opacity.ensure(0.0).add_keyframe(t_end, 0.0, Easing::Linear);
            }
        }
    }
}

/// Unhighlight action fades out the highlight overlay on equation fragments.
pub struct Unhighlight;

impl BuiltinAction for Unhighlight {
    fn signature(&self) -> CallableDescriptor {
        animatix_std::callable::TimelineBuiltin::Unhighlight.descriptor().clone()
    }

    fn execute(
        &self,
        action: &BuiltinInvocation,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let timing = super::ActionTiming::parse(action, time_ms, diagnostics);
        let easing = timing.easing;

        let t_start_ms = timing.t_start_ms;
        let t_end_ms = timing.t_end_ms;

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }

            let Some(id) = timeline.world.get_id(target) else {
                continue;
            };

            // Animate highlight opacity: current → 0
            let hl = timeline.world.ensure_highlight(id);
            let start_opacity = hl.highlight_opacity.get(t_start_ms, 1.0);
            hl.highlight_opacity.ensure(0.0).add_keyframe(
                t_start_ms,
                start_opacity,
                Easing::Linear,
            );
            hl.highlight_opacity.ensure(0.0).add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}

#[cfg(test)]
#[path = "highlight_tests.rs"]
mod tests;
