use super::*;
use crate::ast::{Expr, Modifier};
use crate::timeline::Environment;

impl Timeline {
    /// Handle effect-stage and scene-level assignments; returns whether handled.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assign_effect_or_scene(
        &mut self,
        target: &[TargetSegment],
        property: &str,
        value: &Expr,
        eval_env: &Environment,
        t_start_ms: u64,
        t_end_ms: u64,
        duration_ms: f64,
        instant_delayed: bool,
        easing: Easing,
        diagnostics: &mut Vec<Diagnostic>,
        assignment_subject: &str,
    ) -> bool {
        // ── Effect stage parameter: `scope.stage.param = value` ──
        //
        // Effects are not actors, so the target resolves as `[scope, stage]`
        // rather than to a track. Handle it before the generic target walk.
        if target.len() == 1 {
            // Bare stage form (`pix.size = 16`): the stage label must be
            // unique across all `Filter` scopes; otherwise it is ambiguous
            // and reported instead of guessed.
            let stage_label = target[0].label_str();
            let mut owners: Vec<String> = self
                .world
                .tracks
                .iter()
                .filter(|(_, track)| track.caps.is_effect_scope())
                .filter(|(_, track)| {
                    self.world
                        .effects(track.id)
                        .and_then(|chain| chain.stage(stage_label))
                        .is_some()
                })
                .map(|(label, _)| label.clone())
                .collect();
            if owners.len() > 1 {
                owners.sort();
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::InvalidAssignmentTarget,
                        DiagnosticPhase::Build,
                        format!(
                            "Effect stage '{stage_label}' is declared in multiple Filter scopes ({}); qualify it as `scope.{stage_label}.<param>`",
                            owners.join(", ")
                        ),
                    )
                    .with_subject(assignment_subject),
                );
                return true;
            }
            if let Some(scope_label) = owners.pop() {
                let stage_kind = self
                    .world
                    .tracks
                    .get(&scope_label)
                    .and_then(|track| self.world.effects(track.id))
                    .and_then(|chain| chain.stage(stage_label))
                    .map(|stage| stage.kind.clone());
                if let Some(kind) = stage_kind {
                    write_effect_stage_param(
                        self,
                        &scope_label,
                        stage_label,
                        kind,
                        property,
                        value,
                        eval_env,
                        t_start_ms,
                        t_end_ms,
                        duration_ms,
                        easing,
                        diagnostics,
                        assignment_subject,
                    );
                    return true;
                }
            }
        }
        if target.len() == 2 {
            let scope_label = target[0].label_str();
            let stage_label = target[1].label_str();
            let stage_kind = self
                .world
                .tracks
                .get(scope_label)
                .filter(|track| track.caps.is_effect_scope())
                .and_then(|track| self.world.effects(track.id))
                .and_then(|chain| chain.stage(stage_label))
                .map(|stage| stage.kind.clone());
            if let Some(kind) = stage_kind {
                write_effect_stage_param(
                    self,
                    scope_label,
                    stage_label,
                    kind,
                    property,
                    value,
                    eval_env,
                    t_start_ms,
                    t_end_ms,
                    duration_ms,
                    easing,
                    diagnostics,
                    assignment_subject,
                );
                return true;
            }
        }

        // ── Scene-level property (background_color) ──
        if target.len() == 1 && target[0].label_str() == "scene" {
            if property == "background_color" {
                let Some(target_color) = parse_color_in_env_with_lookup_diagnostic(
                    "scene",
                    "background_color",
                    value,
                    eval_env,
                    diagnostics,
                    assignment_subject,
                ) else {
                    return true;
                };
                if duration_ms > 0.0 {
                    let start_val = self.background_color.evaluate(t_start_ms);
                    self.background_color.add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed
                    && t_start_ms > 0
                    && !self.background_color.keyframes.contains_key(&(t_start_ms - 1))
                {
                    let prev_val = self.background_color.evaluate(t_start_ms - 1);
                    self.background_color.add_keyframe(t_start_ms - 1, prev_val, Easing::Linear);
                }
                self.background_color.add_keyframe(t_end_ms, target_color, easing);
            }
            return true;
        }
        false
    }

    /// Handle variable-field and camera assignments; returns whether handled.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn assign_variable_or_camera(
        &mut self,
        target: &[TargetSegment],
        property: &str,
        value: &Expr,
        modifiers: &[Modifier],
        eval_env: &Environment,
        time_ms: f64,
        t_start_ms: u64,
        t_end_ms: u64,
        easing: Easing,
        diagnostics: &mut Vec<Diagnostic>,
        assignment_subject: &str,
    ) -> bool {
        // ── Variable field assignment (e.g., `p.x = 30` where `p` is a variable holding an Object)
        if target.len() == 1 {
            let var_name = target[0].label_str();
            // Check if this variable exists as a variable track holding an Object value
            if let Some(current) = self
                .variable_tracks
                .get(var_name)
                .and_then(|track| track.evaluate(time_ms as u64))
            {
                if matches!(current, Value::Object(_, _)) {
                    // Evaluate the assignment value
                    let eval_val = match evaluate_expr(value, eval_env) {
                        Ok(v) => v,
                        Err(e) => {
                            diagnostics.push(Diagnostic::error(
                                DiagnosticCode::InvalidAssignmentTarget,
                                DiagnosticPhase::Build,
                                format!(
                                    "Failed to evaluate value for '{}.{}': {}",
                                    var_name, property, e
                                ),
                            ));
                            return true;
                        },
                    };
                    // Update the Object's field (immutable update via with_field)
                    let new_obj = current.with_field(property, eval_val);
                    self.variable_tracks
                        .entry(var_name.to_string())
                        .or_default()
                        .keyframes
                        .insert(time_ms as u64, new_obj);
                    return true;
                }
            }
        }

        // The scene camera is addressed by a reserved name, not by a track, so
        // it is matched before target resolution — there is no actor called
        // `camera` to look up. See `timeline::camera`.
        if target
            .first()
            .is_some_and(|segment| segment.label_str() == crate::timeline::camera::CAMERA_TARGET)
        {
            if crate::timeline::camera::is_focus(property) {
                let target_label = match value {
                    crate::ast::Expr::Ident(s) | crate::ast::Expr::Str(s) => Some(s.clone()),
                    _ => match evaluate_expr_with_lookup_diagnostic(
                        value,
                        eval_env,
                        diagnostics,
                        assignment_subject,
                    ) {
                        Some(Value::Str(s)) => Some(s),
                        _ => None,
                    },
                };
                let Some(label) = target_label else {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            format!("`camera.{property}` expects an actor name, got {value:?}"),
                        )
                        .with_subject(assignment_subject),
                    );
                    return true;
                };

                let padding = modifiers
                    .iter()
                    .find_map(|m| {
                        if m.name.as_deref() == Some("padding") {
                            match &m.value {
                                crate::ast::Expr::Num(n) => Some(*n as f32),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    })
                    .unwrap_or(40.0);

                if let Some(track) = self.world.tracks.get(&label) {
                    let pos = track.geometry.position.get(t_start_ms, [0.0, 0.0]);
                    let half_size = track.geometry.size.get(t_start_ms, [50.0, 50.0]);
                    let w = (half_size[0] * 2.0).max(1.0);
                    let h = (half_size[1] * 2.0).max(1.0);
                    let view_w = 1920.0_f32;
                    let view_h = 1080.0_f32;
                    let target_zoom = ((view_w - 2.0 * padding) / w)
                        .min((view_h - 2.0 * padding) / h)
                        .clamp(0.1, 10.0);
                    let scene_center = [view_w / 2.0, view_h / 2.0];
                    let target_pan = [
                        -target_zoom * (pos[0] - scene_center[0]),
                        -target_zoom * (pos[1] - scene_center[1]),
                    ];

                    self.camera.assign(
                        "pan",
                        &Value::Vec2([target_pan[0] as f64, target_pan[1] as f64]),
                        t_start_ms,
                        t_end_ms,
                        easing,
                    );
                    self.camera.assign(
                        "zoom",
                        &Value::Num(target_zoom as f64),
                        t_start_ms,
                        t_end_ms,
                        easing,
                    );
                    self.camera_used.set(true);
                } else {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::UnknownTargetPath,
                            DiagnosticPhase::Build,
                            format!("`camera.{property}`: target actor '{label}' not found"),
                        )
                        .with_subject(assignment_subject),
                    );
                }
                return true;
            }

            let Some(evaluated) = evaluate_expr_with_lookup_diagnostic(
                value,
                eval_env,
                diagnostics,
                assignment_subject,
            ) else {
                // The expression itself failed; that diagnostic is already filed.
                return true;
            };
            if !crate::timeline::camera::is_camera_property(property) {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!(
                            "`camera` has no property '{property}'; it takes {}",
                            crate::timeline::camera::CAMERA_PROPERTIES
                        ),
                    )
                    .with_subject(assignment_subject),
                );
            } else if !self.camera.assign(property, &evaluated, t_start_ms, t_end_ms, easing) {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!(
                            "`camera.{property}` expects {}, got {evaluated:?}",
                            if crate::timeline::camera::is_pan(property) {
                                "an (x, y) pan in pixels"
                            } else {
                                "a number"
                            }
                        ),
                    )
                    .with_subject(assignment_subject),
                );
            }
            return true;
        }

        false
    }
}
