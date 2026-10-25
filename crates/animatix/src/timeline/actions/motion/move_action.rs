use super::helpers::*;
use super::*;

/// Move the target to an absolute position (optionally along a path).
pub struct Move;

impl BuiltinAction for Move {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "move".to_string(),
            category: "Motion".to_string(),
            description:
                "Moves the target to a local translation offset on top of its existing placement."
                    .to_string(),
            params: vec![],
            modifiers: motion_timing_params(),
        }
    }

    fn execute(
        &self,
        action: &Action,
        time_ms: f64,
        timeline: &mut Timeline,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // `move target along: {…} [2s]` traces a route instead of translating:
        // the path is sampled by arc length and baked into position (and, with
        // `orient: true`, rotation) keyframes, so the motion is exact under
        // scrubbing and needs no frame-time state.
        if let Some(along) = action.modifiers.iter().find(|m| m.name.as_deref() == Some("along")) {
            run_move_along(action, along, time_ms, timeline, diagnostics);
            return;
        }
        let Some(target_offset) = parse_vec2_modifier(
            &action.modifiers,
            timeline,
            "to",
            "Move action requires a 'to' vec2 modifier such as [to: (140, -40)], \
             or an 'along' path such as [along: {move_to(100, 100), line_to(500, 300)}].",
            diagnostics,
        ) else {
            return;
        };

        let timing_modifiers = timing_modifiers_without_keys(&action.modifiers, &["to"]);
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

        for target in &action.targets {
            if !crate::timeline::actions::ensure_target_exists(
                timeline,
                target,
                &action.verb,
                diagnostics,
                None,
            ) {
                continue;
            }

            let track_id = match timeline.world.get_id(target) {
                Some(id) => id,
                None => continue,
            };
            let geometry = timeline.world.geometry_mut(track_id);
            let prior_v = geometry
                .motion_offset
                .as_ref()
                .map(|mot| mot.velocity_at(t_start_ms))
                .unwrap_or([0.0, 0.0]);
            let start_offset = geometry.motion_offset.get(t_start_ms, [0.0, 0.0]);
            let travel = [
                target_offset[0] - start_offset[0],
                target_offset[1] - start_offset[1],
            ];
            let dist_sq = travel[0] * travel[0] + travel[1] * travel[1];
            let effective_easing = match easing {
                Easing::Spring { damping, frequency } => {
                    if dist_sq > 1e-4
                        && (prior_v[0] != 0.0 || prior_v[1] != 0.0)
                        && duration_ms > 0.0
                    {
                        let dot = prior_v[0] * travel[0] + prior_v[1] * travel[1];
                        let v0_norm = (dot / dist_sq) * (duration_ms as f32);
                        Easing::SpringV0 {
                            damping,
                            frequency,
                            v0: v0_norm,
                        }
                    } else {
                        easing
                    }
                },
                other => other,
            };

            let from_offset = anticipation_start(
                geometry,
                parsed.anticipate_ms,
                t_start_ms,
                start_offset,
                travel,
            )
            .unwrap_or(start_offset);

            if let Some(mot) = geometry.motion_offset.as_mut() {
                mot.split_off_after(t_start_ms);
            }

            if duration_ms > 0.0 {
                geometry.motion_offset.ensure([0.0, 0.0]).add_keyframe(
                    t_start_ms,
                    from_offset,
                    Easing::Linear,
                );
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_offset = geometry.motion_offset.get(guard_time, [0.0, 0.0]);
                if !geometry
                    .motion_offset
                    .as_ref()
                    .map(|t| t.keyframes.contains_key(&guard_time))
                    .unwrap_or(false)
                {
                    geometry.motion_offset.ensure([0.0, 0.0]).add_keyframe(
                        guard_time,
                        prior_offset,
                        Easing::Linear,
                    );
                }
            }

            geometry.motion_offset.ensure([0.0, 0.0]).add_keyframe(
                t_end_ms,
                target_offset,
                effective_easing,
            );
        }
    }
}

/// Expand `move … [along: {…}]` into arc-length-sampled keyframes.
fn run_move_along(
    action: &Action,
    along: &crate::ast::Modifier,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let timing_modifiers = timing_modifiers_without_keys(&action.modifiers, &["along", "orient"]);
    let parsed = parse_timing_modifiers(
        &timing_modifiers,
        ModifierHost::Action,
        Some(&action.verb),
        diagnostics,
    );
    if parsed.duration_ms <= 0.0 {
        diagnostics.push(Diagnostic::warning(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "`move … [along: …]` needs a duration, e.g. [along: {…}, 2s].".to_string(),
        ));
        return;
    }
    let orient = action
        .modifiers
        .iter()
        .find(|m| m.name.as_deref() == Some("orient"))
        .is_some_and(|m| matches!(m.value, crate::ast::Expr::Bool(true)));

    let t_start_ms = (time_ms + parsed.delay_ms) as u64;
    let t_end_ms = (time_ms + parsed.delay_ms + parsed.duration_ms) as u64;
    let span = t_end_ms.saturating_sub(t_start_ms);
    let samples = (span / 40).clamp(8, 48) as usize;

    for target in &action.targets {
        if !crate::timeline::actions::ensure_target_exists(
            timeline,
            target,
            &action.verb,
            diagnostics,
            None,
        ) {
            continue;
        }

        // Resolve and sample the route before touching any track: the path
        // expression reads the build environment, and the keyframe writes below
        // need `timeline` mutably.
        let points = {
            let env = &timeline.env;
            match crate::timeline::shapes::parse_path_commands_expr(&along.value, env) {
                Some(path) => sample_path_along(&path, samples).unwrap_or_default(),
                None => {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::InvalidModifierValue,
                            DiagnosticPhase::Build,
                            format!(
                                "`{target}`: 'along' expects a list of path commands, e.g. \
                                 {{move_to(100, 100), line_to(500, 300)}}"
                            ),
                        )
                        .with_subject(target),
                    );
                    Vec::new()
                },
            }
        };
        if points.is_empty() {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidModifierValue,
                    DiagnosticPhase::Build,
                    format!("`{target}`: the 'along' path has no length to travel"),
                )
                .with_subject(target),
            );
            continue;
        }

        let track_id = match timeline.world.get_id(target) {
            Some(id) => id,
            None => continue,
        };
        let geometry = timeline.world.geometry_mut(track_id);
        // Positions are absolute scene coordinates: the route is drawn where the
        // actor should go, so the authored `at` only matters before it starts.
        // Intermediate samples are linear (equal arc-length spacing *is* the
        // constant speed); the authored ease applies to the final segment.
        for (i, (point, angle)) in points.iter().enumerate() {
            let at = t_start_ms + (span as f64 * (i as f64 / samples as f64)) as u64;
            let easing = if i == samples {
                parsed.easing
            } else {
                Easing::Linear
            };
            geometry.position.ensure([0.0, 0.0]).add_keyframe(
                at.min(t_end_ms),
                [point.x as f32, point.y as f32],
                easing,
            );
            if orient {
                geometry
                    .rotation
                    .ensure(0.0)
                    .add_keyframe(at.min(t_end_ms), *angle as f32, easing);
            }
        }
    }
}
