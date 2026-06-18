use super::registry::{ActionParam, ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
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
        let color = action
            .modifiers
            .iter()
            .find(|m| m.name.as_deref() == Some("color"))
            .and_then(|m| {
                crate::timeline::evaluate_expr(&m.value, &timeline.env)
                    .ok()
                    .map(|v| {
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

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            // Set highlight color keyframe if specified
            if let Some(c) = color {
                track
                    .highlight
                    .highlight_color
                    .ensure([1.0, 1.0, 1.0, 1.0])
                    .add_keyframe(t_start_ms, c, Easing::Linear);
            }

            // Set highlight padding keyframe if specified
            if let Some(p) = padding {
                track
                    .highlight
                    .highlight_padding
                    .ensure(4.0)
                    .add_keyframe(t_start_ms, p, Easing::Linear);
            }

            // Set highlight radius keyframe if specified
            if let Some(r) = radius {
                track
                    .highlight
                    .highlight_radius
                    .ensure(2.0)
                    .add_keyframe(t_start_ms, r, Easing::Linear);
            }

            // Set blend mode (non-animated configuration value)
            track.highlight.highlight_blend = blend;

            // Animate highlight opacity: 0 → 1
            let start_opacity = track.highlight.highlight_opacity.get(t_start_ms, 0.0);
            track
                .highlight
                .highlight_opacity
                .ensure(0.0)
                .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
            track
                .highlight
                .highlight_opacity
                .ensure(0.0)
                .add_keyframe(t_end_ms, 1.0, easing);
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

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            // Animate highlight opacity: current → 0
            let start_opacity = track.highlight.highlight_opacity.get(t_start_ms, 1.0);
            track
                .highlight
                .highlight_opacity
                .ensure(0.0)
                .add_keyframe(t_start_ms, start_opacity, Easing::Linear);
            track
                .highlight
                .highlight_opacity
                .ensure(0.0)
                .add_keyframe(t_end_ms, 0.0, easing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, InlineItem, Modifier, Property, Stmt, Time};
    use crate::timeline::Timeline;

    /// Helper: create a minimal Fragment track inside an Equation track.
    fn make_equation_with_fragment() -> Vec<Stmt> {
        vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "eq".to_string(),
                    array_index: None,
                    ty: "Equation".to_string(),
                    props: vec![],
                    modifiers: vec![],
                    children: vec![InlineItem::Labeled {
                        label: "f1".to_string(),
                        array_index: None,
                        ty: "Fragment".to_string(),
                        props: vec![Property {
                            name: "content".to_string(),
                            value: Expr::Str("x^2".to_string()),
                            value_span: None,
                            trailing_comment: None,
                        }],
                        modifiers: vec![],
                        children: vec![],
                    }],
                    span: None,
                },
            ],
            span: None,
        }]
    }

    #[test]
    fn highlight_adds_opacity_keyframes() {
        let mut ast = make_equation_with_fragment();
        // Append a highlight action at t=1s
        if let Stmt::Keyframe { body, .. } = &mut ast[0] {
            body.push(Stmt::Action(
                Action {
                    verb: "highlight".to_string(),
                    targets: vec!["f1".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("800ms".to_string()),
                    }],
                    byte_span: None,
                },
                None,
            ));
        }

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        if let Some(track) = report.output.tracks.get("f1") {
            // highlight_opacity should have keyframes
            assert!(
                track
                    .highlight
                    .highlight_opacity
                    .as_ref()
                    .map(|t| !t.keyframes.is_empty())
                    .unwrap_or(false),
                "highlight_opacity should have keyframes"
            );
        }
        // No panics or critical errors
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.severity != crate::diagnostics::DiagnosticSeverity::Error),
            "unexpected errors: {:?}",
            report.diagnostics
        );
    }

    #[test]
    fn unhighlight_adds_opacity_keyframes() {
        let mut ast = make_equation_with_fragment();
        if let Stmt::Keyframe { body, .. } = &mut ast[0] {
            body.push(Stmt::Action(
                Action {
                    verb: "unhighlight".to_string(),
                    targets: vec!["f1".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: None,
                        value: Expr::Ident("600ms".to_string()),
                    }],
                    byte_span: None,
                },
                None,
            ));
        }

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
        if let Some(track) = report.output.tracks.get("f1") {
            assert!(
                track
                    .highlight
                    .highlight_opacity
                    .as_ref()
                    .map(|t| !t.keyframes.is_empty())
                    .unwrap_or(false),
                "highlight_opacity should have keyframes for unhighlight"
            );
        }
        assert!(
            report
                .diagnostics
                .iter()
                .all(|d| d.severity != crate::diagnostics::DiagnosticSeverity::Error),
            "unexpected errors: {:?}",
            report.diagnostics
        );
    }
}
