use super::registry::{ActionParam, ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::primitives::ChildProcessing;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{ModifierHost, Timeline, parse_timing_modifiers};

fn highlight_timing_params() -> Vec<ActionParam> {
    let mut params = vec![
        ActionParam {
            name: "color".to_string(),
            description: "Highlight rectangle color (e.g. [color: white], [color: accent.danger])"
                .to_string(),
            type_info: "color".to_string(),
        },
        ActionParam {
            name: "blend".to_string(),
            description:
                "Blend mode name (e.g. [blend: difference], [blend: exclusion], [blend: normal])"
                    .to_string(),
            type_info: "string".to_string(),
        },
        ActionParam {
            name: "padding".to_string(),
            description: "Highlight rectangle padding in logical pixels (e.g. [padding: 6.0])"
                .to_string(),
            type_info: "number".to_string(),
        },
        ActionParam {
            name: "radius".to_string(),
            description: "Highlight rectangle corner radius (e.g. [radius: 4.0])".to_string(),
            type_info: "number".to_string(),
        },
    ];
    params.extend(base_timing_params());
    params
}

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
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "highlight".to_string(),
            category: "Effects".to_string(),
            description: "Fades in a colored highlight rectangle behind equation fragments."
                .to_string(),
            params: vec![],
            modifiers: highlight_timing_params(),
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
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

        // Parse optional color modifier (default: white [1,1,1,1])
        let color =
            action
                .modifiers
                .iter()
                .find(|m| m.name.as_deref() == Some("color"))
                .and_then(|m| {
                    crate::timeline::evaluate_expr(&m.value, &timeline.env).ok().map(|v| {
                        let c = v.as_color();
                        [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32]
                    })
                });

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
            if let Some(c) = color {
                timeline
                    .world
                    .highlight
                    .get_mut(id)
                    .highlight_color
                    .ensure([1.0, 1.0, 1.0, 1.0])
                    .add_keyframe(t_start_ms, c, Easing::Linear);
            }

            // Set highlight padding keyframe if specified
            if let Some(p) = padding {
                timeline.world.highlight.get_mut(id).highlight_padding.ensure(4.0).add_keyframe(
                    t_start_ms,
                    p,
                    Easing::Linear,
                );
            }

            // Set highlight radius keyframe if specified
            if let Some(r) = radius {
                timeline.world.highlight.get_mut(id).highlight_radius.ensure(2.0).add_keyframe(
                    t_start_ms,
                    r,
                    Easing::Linear,
                );
            }

            // Set blend mode (non-animated configuration value)
            timeline.world.highlight.get_mut(id).highlight_blend = blend;

            // Animate highlight opacity: 0 → 1
            let start_opacity =
                timeline.world.highlight.get_mut(id).highlight_opacity.get(t_start_ms, 0.0);
            timeline.world.highlight.get_mut(id).highlight_opacity.ensure(0.0).add_keyframe(
                t_start_ms,
                start_opacity,
                Easing::Linear,
            );
            timeline
                .world
                .highlight
                .get_mut(id)
                .highlight_opacity
                .ensure(0.0)
                .add_keyframe(t_end_ms, 1.0, easing);
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
                if let Some(parent_label) = find_equation_parent(timeline, target) {
                    if let Some(parent_id) = timeline.get_entity_id(&parent_label) {
                        for child in timeline.world.child_labels(parent_id) {
                            if !target_set.contains(child) {
                                siblings.push((child.to_string(), t_start_ms, t_end_ms));
                            }
                        }
                    }
                }
            }
            siblings
        };
        for (sibling, t_start, t_end) in siblings_to_unhighlight {
            if let Some(id) = timeline.world.get_id(&sibling) {
                let start_opacity =
                    timeline.world.highlight.get_mut(id).highlight_opacity.get(t_start, 1.0);
                timeline.world.highlight.get_mut(id).highlight_opacity.ensure(0.0).add_keyframe(
                    t_start,
                    start_opacity,
                    Easing::Linear,
                );
                timeline.world.highlight.get_mut(id).highlight_opacity.ensure(0.0).add_keyframe(
                    t_end,
                    0.0,
                    Easing::Linear,
                );
            }
        }
    }
}

/// Unhighlight action fades out the highlight overlay on equation fragments.
pub struct Unhighlight;

impl BuiltinAction for Unhighlight {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "unhighlight".to_string(),
            category: "Effects".to_string(),
            description: "Fades out the highlight rectangle behind equation fragments.".to_string(),
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

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }

            let Some(id) = timeline.world.get_id(target) else {
                continue;
            };

            // Animate highlight opacity: current → 0
            let start_opacity =
                timeline.world.highlight.get_mut(id).highlight_opacity.get(t_start_ms, 1.0);
            timeline.world.highlight.get_mut(id).highlight_opacity.ensure(0.0).add_keyframe(
                t_start_ms,
                start_opacity,
                Easing::Linear,
            );
            timeline
                .world
                .highlight
                .get_mut(id)
                .highlight_opacity
                .ensure(0.0)
                .add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}

#[cfg(test)]
#[path = "highlight_tests.rs"]
mod tests;
