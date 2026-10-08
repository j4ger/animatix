use super::registry::{ActionParam, ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{AnimationTrack, ModifierHost, Timeline, parse_timing_modifiers};

fn effect_timing_params() -> Vec<ActionParam> {
    let mut params = vec![
        ActionParam {
            name: "intensity".to_string(),
            description:
                "Intensity/strength of the effect (e.g. [intensity: 10.0] for shake amplitude)"
                    .to_string(),
            type_info: "number".to_string(),
        },
        ActionParam {
            name: "frequency".to_string(),
            description: "Number of oscillations (e.g. [frequency: 5] for shake count)".to_string(),
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
            let start_offset = track.geometry.motion_offset.get(t_start_ms, [0.0, 0.0]);

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
                track.geometry.motion_offset.ensure([0.0, 0.0]).add_keyframe(
                    cycle_time,
                    shake_offset,
                    Easing::Linear,
                );
            }

            // Return to original position at end
            track.geometry.motion_offset.ensure([0.0, 0.0]).add_keyframe(
                t_end_ms,
                start_offset,
                easing,
            );
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
            description: "Pulses the target by scaling up and then returning to normal."
                .to_string(),
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

            let start_scale = track.geometry.scale.get(t_start_ms, 1.0);
            let peak_scale = start_scale * (1.0 + intensity);

            // Scale up to peak
            track
                .geometry
                .scale
                .ensure(1.0)
                .add_keyframe(t_start_ms, start_scale, Easing::Linear);
            track.geometry.scale.ensure(1.0).add_keyframe(t_mid_ms, peak_scale, easing);

            // Scale back down
            track.geometry.scale.ensure(1.0).add_keyframe(t_end_ms, start_scale, easing);
        }
    }
}

/// Default fraction of velocity each impact keeps. 0.6 gives a first rebound at
/// 36% of the original height — visibly alive, settled inside eight hops.
const DEFAULT_RESTITUTION: f32 = 0.6;

/// A hop shorter than this many pixels is not visible, so the series stops
/// there rather than emitting a buzz of sub-pixel contacts.
const MIN_VISIBLE_HOP: f32 = 0.5;

/// Upper bound on hops, so a deliberately springy `restitution: 0.95` cannot
/// emit an unbounded keyframe chain.
const MAX_BOUNCE_HOPS: usize = 8;

/// Keyframe the additive `motion_offset` channel of a track.
///
/// Effects write here rather than to `at`, so they stack on top of whatever
/// positional choreography the author already keyed.
fn add_offset(track: &mut AnimationTrack, at_ms: u64, y: [f32; 2], easing: Easing) {
    track.geometry.motion_offset.ensure([0.0, 0.0]).add_keyframe(at_ms, y, easing);
}

/// Read a numeric modifier of an effect action, evaluated against the
/// timeline's build-time environment.
fn effect_modifier_num(action: &Action, timeline: &Timeline, name: &str) -> Option<f32> {
    action.modifiers.iter().find(|m| m.name.as_deref() == Some(name)).and_then(|m| {
        crate::timeline::evaluate_expr(&m.value, &timeline.env)
            .ok()
            .map(|v| v.as_num() as f32)
    })
}

/// Bounce action launches the actor off its resting spot and lets gravity
/// settle it back down.
pub struct Bounce;

impl BuiltinAction for Bounce {
    fn signature(&self) -> ActionSignature {
        let mut modifiers = effect_timing_params();
        modifiers.push(ActionParam {
            name: "restitution".to_string(),
            description:
                "Fraction of velocity each impact keeps (default 0.6); higher bounces longer"
                    .to_string(),
            type_info: "number".to_string(),
        });
        ActionSignature {
            name: "bounce".to_string(),
            category: "Effects".to_string(),
            description: "Hops the target under gravity: `intensity` is the first hop's height, each rebound keeps `restitution` of the last one's velocity.".to_string(),
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

        // `intensity` is the first hop's height in pixels. `restitution` is the
        // fraction of the previous rebound's *velocity* each impact keeps, so a
        // hop's airtime shrinks by it and its height by its square — which is
        // why the contacts fall closer together as the ball settles.
        let intensity = effect_modifier_num(action, timeline, "intensity").unwrap_or(40.0);
        let restitution = effect_modifier_num(action, timeline, "restitution")
            .unwrap_or(DEFAULT_RESTITUTION)
            .clamp(0.05, 0.95);

        // The series is truncated once a hop is too small to see, so the first
        // hop's airtime is scaled to make exactly those hops fill the requested
        // duration: Σ d₀·rᵏ for k in 0..hops = duration.
        let hops = (0..MAX_BOUNCE_HOPS)
            .filter(|k| intensity * restitution.powi(2 * *k as i32) >= MIN_VISIBLE_HOP)
            .count();
        let first_airtime = if hops == 0 {
            0.0
        } else {
            duration_ms * f64::from(1.0 - restitution)
                / f64::from(1.0 - restitution.powi(hops as i32))
        };

        for target in &action.targets {
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics, None) {
                continue;
            }

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            let start_offset = track.geometry.motion_offset.get(t_start_ms, [0.0, 0.0]);
            add_offset(track, t_start_ms, start_offset, Easing::Linear);

            let mut cursor = 0.0f64;
            let mut airtime = first_airtime;
            for hop in 0..hops {
                let peak = intensity * restitution.powi(2 * hop as i32);
                let base = t_start_ms as f64 + cursor;
                // Rising decelerates and falling accelerates: that pair *is*
                // the parabola, so the arc needs no per-frame expression.
                let apex_ms = (base + airtime * 0.5) as u64;
                let land_ms = (base + airtime) as u64;
                add_offset(
                    track,
                    apex_ms,
                    [start_offset[0], start_offset[1] - peak],
                    Easing::EaseOut,
                );
                add_offset(track, land_ms, start_offset, Easing::EaseIn);
                cursor += airtime;
                airtime *= f64::from(restitution);
            }

            // Pin the rest position: if the series is truncated by the hop cap
            // the actor must still land where it started.
            add_offset(track, t_end_ms, start_offset, easing);
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
            array_index: None,
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
        Stmt::Action(
            Action {
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
                target_index: vec![],
            },
            None,
        )
    }

    #[test]
    fn shake_adds_motion_keyframes() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![circle_decl("badge"), shake_action("badge", 15.0)],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("badge").expect("badge track");

        // Check that multiple motion offset keyframes were added
        assert!(
            track
                .geometry
                .motion_offset
                .as_ref()
                .map(|t| !t.keyframes.is_empty())
                .unwrap_or(false)
        );
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn pulse_adds_scale_keyframes() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                circle_decl("badge"),
                Stmt::Action(
                    Action {
                        verb: "pulse".to_string(),
                        targets: vec!["badge".to_string()],
                        args: vec![],
                        modifiers: vec![
                            Modifier {
                                name: Some("intensity".to_string()),
                                value: Expr::Num(0.3),
                            },
                            Modifier {
                                name: None,
                                value: Expr::Ident("600ms".to_string()),
                            },
                        ],
                        byte_span: None,
                        target_index: vec![],
                    },
                    None,
                ),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("badge").expect("badge track");

        // Check that scale keyframes were added
        assert!(track.geometry.scale.as_ref().map(|t| t.keyframes.len() >= 2).unwrap_or(false));
        assert!(report.diagnostics.is_empty());
    }
}
