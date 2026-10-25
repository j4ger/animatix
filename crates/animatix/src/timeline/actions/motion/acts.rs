use super::helpers::*;
use super::*;

/// Translate the target by a relative offset.
pub struct Shift;

impl BuiltinAction for Shift {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "shift".to_string(),
            category: "Motion".to_string(),
            description:
                "Applies a relative local translation on top of the target's existing placement."
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
        let Some(shift_by) = parse_vec2_modifier(
            &action.modifiers,
            timeline,
            "by",
            "Shift action requires a 'by' vec2 modifier such as [by: (40, -24)].",
            diagnostics,
        ) else {
            return;
        };

        let timing_modifiers = timing_modifiers_without_keys(&action.modifiers, &["by"]);
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
            let end_offset = [start_offset[0] + shift_by[0], start_offset[1] + shift_by[1]];
            let dist_sq = shift_by[0] * shift_by[0] + shift_by[1] * shift_by[1];
            let effective_easing = match easing {
                Easing::Spring { damping, frequency } => {
                    if dist_sq > 1e-4
                        && (prior_v[0] != 0.0 || prior_v[1] != 0.0)
                        && duration_ms > 0.0
                    {
                        let dot = prior_v[0] * shift_by[0] + prior_v[1] * shift_by[1];
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
                shift_by,
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
                end_offset,
                effective_easing,
            );
        }
    }
}

/// Applies a relative local rotation (in radians) to the target.
pub struct Rotate;

impl BuiltinAction for Rotate {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "rotate".to_string(),
            category: "Motion".to_string(),
            description:
                "Applies a relative local rotation in radians on top of the target's existing placement."
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
        let Some(angle_by) = parse_num_modifier(
            &action.modifiers,
            timeline,
            "by",
            "Rotate action requires a numeric 'by' modifier (e.g. [by: 1.5708] for radians, or [by: deg(90)])",
            diagnostics,
        ) else {
            return;
        };

        let timing_modifiers = timing_modifiers_without_keys(&action.modifiers, &["by"]);
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
            let start_rotation = geometry.rotation.get(t_start_ms, 0.0);
            let end_rotation = start_rotation + angle_by;

            if duration_ms > 0.0 {
                geometry.rotation.ensure(0.0).add_keyframe(
                    t_start_ms,
                    start_rotation,
                    Easing::Linear,
                );
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_rotation = geometry.rotation.get(guard_time, 0.0);
                if !geometry
                    .rotation
                    .as_ref()
                    .map(|t| t.keyframes.contains_key(&guard_time))
                    .unwrap_or(false)
                {
                    geometry.rotation.ensure(0.0).add_keyframe(
                        guard_time,
                        prior_rotation,
                        Easing::Linear,
                    );
                }
            }

            geometry.rotation.ensure(0.0).add_keyframe(t_end_ms, end_rotation, easing);
        }
    }
}

/// Applies a relative uniform local scale factor to the target.
pub struct Scale;

impl BuiltinAction for Scale {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: "scale".to_string(),
            category: "Motion".to_string(),
            description:
                "Applies a relative uniform local scale on top of the target's existing placement."
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
        let Some(scale_by) = parse_positive_num_modifier(
            &action.modifiers,
            timeline,
            "by",
            "Scale action requires a positive numeric 'by' modifier such as [by: 1.5].",
            diagnostics,
        ) else {
            return;
        };

        let timing_modifiers = timing_modifiers_without_keys(&action.modifiers, &["by"]);
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
            let start_scale = geometry.scale.get(t_start_ms, 1.0);
            let end_scale = start_scale * scale_by;

            if duration_ms > 0.0 {
                geometry.scale.ensure(1.0).add_keyframe(t_start_ms, start_scale, Easing::Linear);
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_scale = geometry.scale.get(guard_time, 1.0);
                if !geometry
                    .scale
                    .as_ref()
                    .map(|t| t.keyframes.contains_key(&guard_time))
                    .unwrap_or(false)
                {
                    geometry.scale.ensure(1.0).add_keyframe(
                        guard_time,
                        prior_scale,
                        Easing::Linear,
                    );
                }
            }

            geometry.scale.ensure(1.0).add_keyframe(t_end_ms, end_scale, easing);
        }
    }
}
