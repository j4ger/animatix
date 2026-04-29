use super::registry::{ActionParam, ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::{Action, Modifier};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::timeline::track::TrackAccessor;
use crate::timeline::{ModifierHost, Timeline, Value, evaluate_expr, parse_timing_modifiers};

fn motion_timing_params() -> Vec<ActionParam> {
    let mut params = vec![
        ActionParam {
            name: "to".to_string(),
            description:
                "Target local translation offset for the move action (e.g. [to: (140, -40)])."
                    .to_string(),
            type_info: "vec2".to_string(),
        },
        ActionParam {
            name: "angle".to_string(),
            description: "Target rotation in radians for the rotate action (e.g. [angle: 1.5708])."
                .to_string(),
            type_info: "number (radians)".to_string(),
        },
        ActionParam {
            name: "factor".to_string(),
            description: "Target scale factor for the scale action (e.g. [factor: 1.5])."
                .to_string(),
            type_info: "positive number".to_string(),
        },
        ActionParam {
            name: "by".to_string(),
            description: "Relative translation delta for the shift action (e.g. [by: (40, -24)])."
                .to_string(),
            type_info: "vec2".to_string(),
        },
    ];
    params.extend(base_timing_params());
    params
}

fn push_shift_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    code: DiagnosticCode,
    message: impl Into<String>,
    subject: &str,
) {
    diagnostics.push(
        Diagnostic::warning(code, DiagnosticPhase::Build, message.into()).with_subject(subject),
    );
}

fn parse_vec2_modifier(
    modifiers: &[Modifier],
    timeline: &Timeline,
    key: &str,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<[f32; 2]> {
    let mut value = None;
    let mut saw_key = false;

    for modifier in modifiers {
        if modifier.name.as_deref() != Some(key) {
            continue;
        }

        if saw_key {
            push_shift_diagnostic(
                diagnostics,
                DiagnosticCode::ConflictingModifierKey,
                format!("Conflicting '{key}' modifiers on action; using the last value provided."),
                key,
            );
        }

        match evaluate_expr(&modifier.value, &timeline.env) {
            Ok(Value::Vec2([x, y])) => {
                value = Some([x as f32, y as f32]);
                saw_key = true;
            }
            Ok(_) | Err(_) => {
                push_shift_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    message,
                    key,
                );
            }
        }
    }

    if value.is_none() {
        push_shift_diagnostic(
            diagnostics,
            DiagnosticCode::InvalidModifierValue,
            message,
            key,
        );
    }

    value
}

fn parse_num_modifier(
    modifiers: &[Modifier],
    timeline: &Timeline,
    key: &str,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<f32> {
    let mut value = None;
    let mut saw_key = false;

    for modifier in modifiers {
        if modifier.name.as_deref() != Some(key) {
            continue;
        }

        if saw_key {
            push_shift_diagnostic(
                diagnostics,
                DiagnosticCode::ConflictingModifierKey,
                format!("Conflicting '{key}' modifiers on action; using the last value provided."),
                key,
            );
        }

        match evaluate_expr(&modifier.value, &timeline.env) {
            Ok(Value::Num(n)) => {
                value = Some(n as f32);
                saw_key = true;
            }
            Ok(_) | Err(_) => {
                push_shift_diagnostic(
                    diagnostics,
                    DiagnosticCode::InvalidModifierValue,
                    message,
                    key,
                );
            }
        }
    }

    if value.is_none() {
        push_shift_diagnostic(
            diagnostics,
            DiagnosticCode::InvalidModifierValue,
            message,
            key,
        );
    }

    value
}

fn parse_positive_num_modifier(
    modifiers: &[Modifier],
    timeline: &Timeline,
    key: &str,
    message: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<f32> {
    let value = parse_num_modifier(modifiers, timeline, key, message, diagnostics)?;
    if value <= 0.0 {
        push_shift_diagnostic(
            diagnostics,
            DiagnosticCode::InvalidModifierValue,
            message,
            key,
        );
        return None;
    }
    Some(value)
}

fn timing_modifiers_without_keys(modifiers: &[Modifier], excluded_keys: &[&str]) -> Vec<Modifier> {
    modifiers
        .iter()
        .filter(|modifier| {
            modifier
                .name
                .as_deref()
                .is_none_or(|name| !excluded_keys.contains(&name))
        })
        .cloned()
        .collect()
}

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
        let Some(target_offset) = parse_vec2_modifier(
            &action.modifiers,
            timeline,
            "to",
            "Move action requires a 'to' vec2 modifier such as [to: (140, -40)].",
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
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics) {
                continue;
            }

            let track = timeline
                .tracks
                .get_mut(target)
                .expect("validated target track");
            let start_offset = track.motion_offset.get(t_start_ms, [0.0, 0.0]);

            if duration_ms > 0.0 {
                track
                    .motion_offset
                    .ensure([0.0, 0.0])
                    .add_keyframe(t_start_ms, start_offset, Easing::Linear);
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_offset = track.motion_offset.get(guard_time, [0.0, 0.0]);
                if !track.motion_offset.as_ref().map(|t| t.keyframes.contains_key(&guard_time)).unwrap_or(false) {
                    track
                        .motion_offset
                        .ensure([0.0, 0.0])
                        .add_keyframe(guard_time, prior_offset, Easing::Linear);
                }
            }

            track
                .motion_offset
                .ensure([0.0, 0.0])
                .add_keyframe(t_end_ms, target_offset, easing);
        }
    }
}

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
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics) {
                continue;
            }

            let track = timeline
                .tracks
                .get_mut(target)
                .expect("validated target track");
            let start_offset = track.motion_offset.get(t_start_ms, [0.0, 0.0]);
            let end_offset = [start_offset[0] + shift_by[0], start_offset[1] + shift_by[1]];

            if duration_ms > 0.0 {
                track
                    .motion_offset
                    .ensure([0.0, 0.0])
                    .add_keyframe(t_start_ms, start_offset, Easing::Linear);
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_offset = track.motion_offset.get(guard_time, [0.0, 0.0]);
                if !track.motion_offset.as_ref().map(|t| t.keyframes.contains_key(&guard_time)).unwrap_or(false) {
                    track
                        .motion_offset
                        .ensure([0.0, 0.0])
                        .add_keyframe(guard_time, prior_offset, Easing::Linear);
                }
            }

            track
                .motion_offset
                .ensure([0.0, 0.0])
                .add_keyframe(t_end_ms, end_offset, easing);
        }
    }
}

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
            "Rotate action requires a numeric 'by' modifier in radians such as [by: 1.5708].",
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
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics) {
                continue;
            }

            let track = timeline
                .tracks
                .get_mut(target)
                .expect("validated target track");
            let start_rotation = track.rotation.get(t_start_ms, 0.0);
            let end_rotation = start_rotation + angle_by;

            if duration_ms > 0.0 {
                track
                    .rotation
                    .ensure(0.0)
                    .add_keyframe(t_start_ms, start_rotation, Easing::Linear);
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_rotation = track.rotation.get(guard_time, 0.0);
                if !track.rotation.as_ref().map(|t| t.keyframes.contains_key(&guard_time)).unwrap_or(false) {
                    track
                        .rotation
                        .ensure(0.0)
                        .add_keyframe(guard_time, prior_rotation, Easing::Linear);
                }
            }

            track.rotation.ensure(0.0).add_keyframe(t_end_ms, end_rotation, easing);
        }
    }
}

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
            if !super::ensure_target_exists(timeline, target, &action.verb, diagnostics) {
                continue;
            }

            let track = timeline
                .tracks
                .get_mut(target)
                .expect("validated target track");
            let start_scale = track.scale.get(t_start_ms, 1.0);
            let end_scale = start_scale * scale_by;

            if duration_ms > 0.0 {
                track
                    .scale
                    .ensure(1.0)
                    .add_keyframe(t_start_ms, start_scale, Easing::Linear);
            } else if delay_ms > 0.0 && t_start_ms > 0 {
                let guard_time = t_start_ms.saturating_sub(1);
                let prior_scale = track.scale.get(guard_time, 1.0);
                if !track.scale.as_ref().map(|t| t.keyframes.contains_key(&guard_time)).unwrap_or(false) {
                    track
                        .scale
                        .ensure(1.0)
                        .add_keyframe(guard_time, prior_scale, Easing::Linear);
                }
            }

            track.scale.ensure(1.0).add_keyframe(t_end_ms, end_scale, easing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Property, Stmt, Time};
    use crate::timeline::PlacementMode;

    fn circle_decl(label: &str) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            label: label.to_string(),
            ty: "Circle".to_string(),
            props: vec![Property {
                name: "radius".to_string(),
                value: Expr::Num(20.0),
                value_span: None,
            }],
            modifiers: vec![],
            children: vec![],
        }
    }

    fn shift_action(target: &str, by: Expr) -> Stmt {
        Stmt::Action(Action {
            verb: "shift".to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("by".to_string()),
                    value: by,
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
            ],
        })
    }

    fn move_action(target: &str, to: Expr) -> Stmt {
        Stmt::Action(Action {
            verb: "move".to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("to".to_string()),
                    value: to,
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
            ],
        })
    }

    fn rotate_action(target: &str, by: Expr) -> Stmt {
        Stmt::Action(Action {
            verb: "rotate".to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("by".to_string()),
                    value: by,
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
            ],
        })
    }

    fn scale_action(target: &str, by: Expr) -> Stmt {
        Stmt::Action(Action {
            verb: "scale".to_string(),
            targets: vec![target.to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("by".to_string()),
                    value: by,
                },
                Modifier {
                    name: None,
                    value: Expr::Ident("1s".to_string()),
                },
            ],
        })
    }

    #[test]
    fn move_animates_motion_offset_to_target_value() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                circle_decl("badge"),
                move_action(
                    "badge",
                    Expr::Tuple(vec![Expr::Num(120.0), Expr::Num(-30.0)]),
                ),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("badge").expect("badge track");

        assert_eq!(track.motion_offset.get(0, [0.0, 0.0]), [0.0, 0.0]);
        assert_eq!(track.motion_offset.get(1000, [0.0, 0.0]), [120.0, -30.0]);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn move_requires_to_vec2_modifier() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![circle_decl("badge"), move_action("badge", Expr::Num(10.0))],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidModifierValue)
        );
    }

    #[test]
    fn rotate_animates_rotation_track_over_duration() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                circle_decl("badge"),
                rotate_action("badge", Expr::Num(1.5708)),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("badge").expect("badge track");

        assert!((track.rotation.get(0, 0.0) - 0.0).abs() < f32::EPSILON);
        assert!((track.rotation.get(1000, 0.0) - 1.5708).abs() < 0.0001);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn rotate_requires_numeric_by_modifier() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                circle_decl("badge"),
                rotate_action("badge", Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0)])),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidModifierValue)
        );
    }

    #[test]
    fn scale_animates_scale_track_over_duration() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![circle_decl("badge"), scale_action("badge", Expr::Num(1.5))],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("badge").expect("badge track");

        assert!((track.scale.get(0, 1.0) - 1.0).abs() < f32::EPSILON);
        assert!((track.scale.get(1000, 1.0) - 1.5).abs() < 0.0001);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn scale_requires_positive_numeric_by_modifier() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![circle_decl("badge"), scale_action("badge", Expr::Num(0.0))],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidModifierValue)
        );
    }

    #[test]
    fn shift_animates_motion_offset_over_duration() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                circle_decl("badge"),
                shift_action(
                    "badge",
                    Expr::Tuple(vec![Expr::Num(40.0), Expr::Num(-24.0)]),
                ),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("badge").expect("badge track");

        assert_eq!(track.motion_offset.get(0, [0.0, 0.0]), [0.0, 0.0]);
        assert_eq!(track.motion_offset.get(1000, [0.0, 0.0]), [40.0, -24.0]);
        assert!(report.diagnostics.is_empty());
    }

    #[test]
    fn shift_requires_by_vec2_modifier() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![circle_decl("badge"), shift_action("badge", Expr::Num(10.0))],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InvalidModifierValue)
        );
    }

    #[test]
    fn shift_preserves_layout_managed_targets() {
        let ast = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    label: "row".to_string(),
                    ty: "Row".to_string(),
                    props: vec![Property {
                        name: "gap".to_string(),
                        value: Expr::Num(20.0),
                        value_span: None,
                    }],
                    modifiers: vec![],
                    children: vec![crate::ast::InlineItem::Labeled {
                        label: "child".to_string(),
                        ty: "Circle".to_string(),
                        props: vec![Property {
                            name: "radius".to_string(),
                            value: Expr::Num(20.0),
                            value_span: None,
                        }],
                        modifiers: vec![],
                        children: vec![],
                    }],
                },
                shift_action("child", Expr::Tuple(vec![Expr::Num(25.0), Expr::Num(0.0)])),
            ],
            span: None,
        }];

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        let track = report.output.tracks.get("child").expect("child track");

        assert_eq!(
            track.placement_mode.get(0, PlacementMode::LayoutManaged),
            PlacementMode::LayoutManaged
        );
        assert_eq!(track.motion_offset.get(1000, [0.0, 0.0]), [25.0, 0.0]);
    }
}
