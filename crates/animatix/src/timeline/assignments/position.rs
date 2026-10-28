//! Compound `position` / `offset` assignment steps.
//!
//! Split out of the assignment entry point so it stays a phase dispatcher;
//! the execution order is unchanged.

use super::*;

impl Timeline {
    /// `position` / `at` — resolve the compound position binding and key both
    /// the position track and the binding track.
    pub(super) fn assign_position(
        &mut self,
        plan: &AssignmentPlan,
        value: &crate::timeline::Expr,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let default_pos = [0.0, 0.0];
        let default_binding = PositionBinding::Absolute;
        let geometry = self.world.geometry.get_mut(plan.track_id);
        let target_pos = if let Some((binding, position)) =
            resolve_position_binding_with_lookup_diagnostic(
                Some(value),
                None,
                None,
                &plan.eval_env,
                diagnostics,
                &plan.subject,
            ) {
            preserve_discrete_position_state_before(geometry, plan.t_start_ms);
            if plan.instant_delayed {
                preserve_instant_delayed_value(&mut geometry.position, plan.t_start_ms);
            }
            mark_track_manual_position(geometry, plan.t_start_ms);
            if plan.duration_ms > 0.0 {
                let start_binding = geometry.position_binding.get(plan.t_start_ms, default_binding);
                geometry.position_binding.ensure(default_binding).add_keyframe(
                    plan.t_start_ms,
                    start_binding,
                    Easing::Linear,
                );
                geometry.position_binding.ensure(default_binding).add_keyframe(
                    plan.t_end_ms,
                    binding,
                    plan.easing,
                );
            } else {
                set_track_position_binding(geometry, plan.t_start_ms, binding);
            }
            position.unwrap_or_else(|| geometry.position.last(default_pos))
        } else {
            geometry.position.last(default_pos)
        };
        if plan.duration_ms > 0.0 {
            let start_val = geometry.position.get(plan.t_start_ms, default_pos);
            geometry.position.ensure(default_pos).add_keyframe(
                plan.t_start_ms,
                start_val,
                Easing::Linear,
            );
        } else if plan.instant_delayed {
            preserve_instant_delayed_value(&mut geometry.position, plan.t_start_ms);
        }
        geometry
            .position
            .ensure(default_pos)
            .add_keyframe(plan.t_end_ms, target_pos, plan.easing);
    }

    /// `offset` — rewrite the offset component of the *current* position
    /// binding, keeping the anchor or percentage base it hangs off. An offset
    /// only exists relative to an anchor/percent base, so on an absolute or
    /// container-default binding it is ignored with the same `IgnoredOffset`
    /// warning the declaration path uses.
    pub(super) fn assign_offset(
        &mut self,
        plan: &AssignmentPlan,
        value: &crate::timeline::Expr,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(new_offset) = parse_numeric_vec2_with_lookup_diagnostic(
            value,
            &plan.eval_env,
            diagnostics,
            &plan.subject,
        ) else {
            return;
        };
        let default_binding = PositionBinding::Absolute;
        let geometry = self.world.geometry.get_mut(plan.track_id);
        let current = geometry.position_binding.get(plan.t_start_ms, default_binding);
        let rebased = match current {
            PositionBinding::SceneAnchor { anchor, .. } => PositionBinding::SceneAnchor {
                anchor,
                offset: new_offset,
            },
            PositionBinding::ScenePercent { x, y, .. } => PositionBinding::ScenePercent {
                x,
                y,
                offset: new_offset,
            },
            PositionBinding::Absolute
            | PositionBinding::ContainerDefault { .. }
            | PositionBinding::ContainerPercent { .. } => {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::IgnoredOffset,
                        DiagnosticPhase::Build,
                        format!(
                            "`offset` has no effect on '{}' without `at`/`anchor`; the value is ignored.",
                            plan.subject
                        ),
                    )
                    .with_subject(&plan.subject),
                );
                return;
            },
        };
        preserve_discrete_position_state_before(geometry, plan.t_start_ms);
        mark_track_manual_position(geometry, plan.t_start_ms);
        if plan.duration_ms > 0.0 {
            let start_binding = geometry.position_binding.get(plan.t_start_ms, default_binding);
            geometry.position_binding.ensure(default_binding).add_keyframe(
                plan.t_start_ms,
                start_binding,
                Easing::Linear,
            );
            geometry.position_binding.ensure(default_binding).add_keyframe(
                plan.t_end_ms,
                rebased,
                plan.easing,
            );
        } else {
            set_track_position_binding(geometry, plan.t_start_ms, rebased);
        }
    }
}
