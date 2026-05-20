use super::registry::{ActionParam, ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::track::TrackAccessor;
use crate::timeline::{ModifierHost, Timeline, parse_timing_modifiers};

fn effect_timing_params() -> Vec<ActionParam> {
    let mut params = vec![
        ActionParam {
            name: "intensity".to_string(),
            description: "Intensity/strength of the effect (e.g. [intensity: 10.0] for shake amplitude)"
                .to_string(),
            type_info: "number".to_string(),
        },
        ActionParam {
            name: "frequency".to_string(),
            description: "Number of oscillations (e.g. [frequency: 5] for shake count)"
                .to_string(),
            type_info: "number".to_string(),
        },
    ];
    params.extend(base_timing_params());
    params
}

/// Shake action applies rapid oscillating position offsets to simulate shaking
pub struct Shake;

impl BuiltinAction for Shake {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "shake".to_string(),
            category: "Effects".to_string(),
            description: "Shakes the target with rapid oscillating horizontal motion.".to_string(),
            params: vec![],
            modifiers: effect_timing_params(),
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
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

        // Parse intensity (amplitude) and frequency (number of shakes)
        let intensity = action
            .modifiers
            .iter()
            .find(|m| m.name.as_deref() == Some("intensity"))
            .and_then(|m| {
                crate::timeline::evaluate_expr(&m.value, &timeline.env)
                    .ok()
                    .map(|v| v.as_num() as f32)
            })
            .unwrap_or(10.0); // Default 10px amplitude

        let frequency = action
            .modifiers
            .iter()
            .find(|m| m.name.as_deref() == Some("frequency"))
            .and_then(|m| {
                crate::timeline::evaluate_expr(&m.value, &timeline.env)
                    .ok()
                    .map(|v| v.as_num() as i32)
            })
            .unwrap_or(8); // Default 8 oscillations

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            // Get starting offset
            let start_offset = track.motion_offset.get(t_start_ms, [0.0, 0.0]);

            // Duration per shake cycle
            let _cycle_duration = if frequency > 0 {
                duration_ms / frequency as f64
            } else {
                duration_ms
            };

            // Generate alternating shake keyframes
            for i in 0..frequency {
                let cycle_progress = i as f64 / frequency as f64;
                let cycle_time = t_start_ms + (duration_ms * cycle_progress) as u64;

                // Alternate positive and negative
                let direction = if i % 2 == 0 { 1.0 } else { -1.0 };
                let shake_offset = [start_offset[0] + intensity * direction, start_offset[1]];

                // Build up shake with linear interpolation between cycles
                track
                    .motion_offset
                    .ensure([0.0, 0.0])
                    .add_keyframe(cycle_time, shake_offset, Easing::Linear);
            }

            // Return to original position at end
            track
                .motion_offset
                .ensure([0.0, 0.0])
                .add_keyframe(t_end_ms, start_offset, easing);
        }
    }
}

/// Pulse action scales the target up and down
pub struct Pulse;

impl BuiltinAction for Pulse {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "pulse".to_string(),
            category: "Effects".to_string(),
            description: "Pulses the target by scaling up and then returning to normal.".to_string(),
            params: vec![],
            modifiers: effect_timing_params(),
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
        let t_mid_ms = (time_ms + delay_ms + duration_ms / 2.0) as u64;
        let t_end_ms = (time_ms + delay_ms + duration_ms) as u64;

        let intensity = action
            .modifiers
            .iter()
            .find(|m| m.name.as_deref() == Some("intensity"))
            .and_then(|m| {
                crate::timeline::evaluate_expr(&m.value, &timeline.env)
                    .ok()
                    .map(|v| v.as_num() as f32)
            })
            .unwrap_or(0.2); // Default 20% scale increase

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            let start_scale = track.scale.get(t_start_ms, 1.0);
            let peak_scale = start_scale * (1.0 + intensity);

            // Scale up to peak
            track
                .scale
                .ensure(1.0)
                .add_keyframe(t_start_ms, start_scale, Easing::Linear);
            track.scale.ensure(1.0).add_keyframe(t_mid_ms, peak_scale, easing.clone());

            // Scale back down
            track.scale.ensure(1.0).add_keyframe(t_end_ms, start_scale, easing);
        }
    }
}

/// Bounce action applies an elastic bounce effect to position
pub struct Bounce;

impl BuiltinAction for Bounce {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "bounce".to_string(),
            category: "Effects".to_string(),
            description: "Applies elastic bounce motion to the target.".to_string(),
            params: vec![],
            modifiers: effect_timing_params(),
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

        let intensity = action
            .modifiers
            .iter()
            .find(|m| m.name.as_deref() == Some("intensity"))
            .and_then(|m| {
                crate::timeline::evaluate_expr(&m.value, &timeline.env)
                    .ok()
                    .map(|v| v.as_num() as f32)
            })
            .unwrap_or(50.0); // Default 50px bounce

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            let start_offset = track.motion_offset.get(t_start_ms, [0.0, 0.0]);

            // Bounce trajectory: down fast, up slower, settle
            // Keyframes at thirds of duration
            let t_33 = (time_ms + delay_ms + duration_ms * 0.33) as u64;
            let t_66 = (time_ms + delay_ms + duration_ms * 0.66) as u64;

            // Start
            track
                .motion_offset
                .ensure([0.0, 0.0])
                .add_keyframe(t_start_ms, start_offset, Easing::Linear);

            // Down (elastic overshoot)
            let bounce_down = [start_offset[0], start_offset[1] + intensity];
            track
                .motion_offset
                .ensure([0.0, 0.0])
                .add_keyframe(t_33, bounce_down, Easing::EaseOut);

            // Up (recovery)
            let bounce_up = [start_offset[0], start_offset[1] - intensity * 0.3];
            track
                .motion_offset
                .ensure([0.0, 0.0])
                .add_keyframe(t_66, bounce_up, Easing::EaseOut);

            // Settle back
            track
                .motion_offset
                .ensure([0.0, 0.0])
                .add_keyframe(t_end_ms, start_offset, easing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Modifier, Property, Stmt, Time};

    fn circle_decl(label: &str) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.to_string(),
            ty: "Ellipse".to_string(),
            props: vec![Property {
                name: "size".to_string(),
                value: Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(40.0)]),
                value_span: None,
            trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    }

    fn shake_action(target: &str, intensity: f64) -> Stmt {
        Stmt::Action(Action {
            verb: "shake".to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("intensity".to_string()),
                    value: Expr::Num(intensity),
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("500ms".to_string()),
                },
            ],
            byte_span: None,
        }, None)
    }

    #[test]
    fn shake_adds_motion_keyframes() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                circle_decl("badge"),
                shake_action("badge", 15.0),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("badge").expect("badge track");

        // Check that multiple motion offset keyframes were added
        assert!(track.motion_offset.as_ref().map(|t| !t.keyframes.is_empty()).unwrap_or(false));
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn pulse_adds_scale_keyframes() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                circle_decl("badge"),
                Stmt::Action(Action {
                    verb: "pulse".to_string(),
                    targets: vec!["badge".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: Some("intensity".to_string()),
                        value: Expr::Num(0.3),
                    }, Modifier {
                        name: None,
                        value: Expr::Ident("600ms".to_string()),
                    }],
                    byte_span: None,
                }, None),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("badge").expect("badge track");

        // Check that scale keyframes were added
        assert!(track.scale.as_ref().map(|t| t.keyframes.len() >= 2).unwrap_or(false));
        assert!(report.diagnostics.is_empty());
    }
}
