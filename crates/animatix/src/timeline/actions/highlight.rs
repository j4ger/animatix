use super::registry::{ActionParam, ActionSignature, BuiltinAction, base_timing_params};
use crate::ast::Action;
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::timeline::actor_kind::ActorKindId;
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

/// Find the parent Equation track label for a given Fragment label, if any.
///
/// This enables exclusive group behavior: when a Fragment inside an Equation
/// is highlighted, sibling Fragments are automatically unhighlighted so that
/// only one Fragment (or one group via multi-target) is highlighted at a time.
fn find_equation_parent(timeline: &Timeline, fragment: &str) -> Option<String> {
    timeline
        .tracks
        .iter()
        .find(|(_, t)| {
            t.kind == ActorKindId::Equation && t.children.contains(&fragment.to_string())
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

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            // Set highlight color keyframe if specified
            if let Some(c) = color {
                track.highlight.highlight_color.ensure([1.0, 1.0, 1.0, 1.0]).add_keyframe(
                    t_start_ms,
                    c,
                    Easing::Linear,
                );
            }

            // Set highlight padding keyframe if specified
            if let Some(p) = padding {
                track.highlight.highlight_padding.ensure(4.0).add_keyframe(
                    t_start_ms,
                    p,
                    Easing::Linear,
                );
            }

            // Set highlight radius keyframe if specified
            if let Some(r) = radius {
                track.highlight.highlight_radius.ensure(2.0).add_keyframe(
                    t_start_ms,
                    r,
                    Easing::Linear,
                );
            }

            // Set blend mode (non-animated configuration value)
            track.highlight.highlight_blend = blend;

            // Animate highlight opacity: 0 → 1
            let start_opacity = track.highlight.highlight_opacity.get(t_start_ms, 0.0);
            track.highlight.highlight_opacity.ensure(0.0).add_keyframe(
                t_start_ms,
                start_opacity,
                Easing::Linear,
            );
            track
                .highlight
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
                    if let Some(parent_track) = timeline.tracks.get(&parent_label) {
                        for child in &parent_track.children {
                            if !target_set.contains(child) {
                                siblings.push((child.clone(), t_start_ms, t_end_ms));
                            }
                        }
                    }
                }
            }
            siblings
        };
        for (sibling, t_start, t_end) in siblings_to_unhighlight {
            if let Some(track) = timeline.tracks.get_mut(&sibling) {
                let start_opacity = track.highlight.highlight_opacity.get(t_start, 1.0);
                track.highlight.highlight_opacity.ensure(0.0).add_keyframe(
                    t_start,
                    start_opacity,
                    Easing::Linear,
                );
                track.highlight.highlight_opacity.ensure(0.0).add_keyframe(
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

            let track = match timeline.tracks.get_mut(target) {
                Some(t) => t,
                None => continue,
            };

            // Animate highlight opacity: current → 0
            let start_opacity = track.highlight.highlight_opacity.get(t_start_ms, 1.0);
            track.highlight.highlight_opacity.ensure(0.0).add_keyframe(
                t_start_ms,
                start_opacity,
                Easing::Linear,
            );
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
            body: vec![Stmt::ActorDecl {
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
            }],
            span: None,
        }]
    }

    /// Helper: create an Equation track with multiple Fragment children.
    fn make_equation_with_fragments(labels: &[&str]) -> Vec<Stmt> {
        let children: Vec<InlineItem> = labels
            .iter()
            .map(|label| InlineItem::Labeled {
                label: label.to_string(),
                array_index: None,
                ty: "Fragment".to_string(),
                props: vec![Property {
                    name: "content".to_string(),
                    value: Expr::Str("x".to_string()),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
            })
            .collect();

        vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "eq".to_string(),
                array_index: None,
                ty: "Equation".to_string(),
                props: vec![],
                modifiers: vec![],
                children,
                span: None,
            }],
            span: None,
        }]
    }

    /// Helper: create two Equations, each with its own Fragments.
    fn make_two_equations() -> Vec<Stmt> {
        vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "eqA".to_string(),
                    array_index: None,
                    ty: "Equation".to_string(),
                    props: vec![],
                    modifiers: vec![],
                    children: vec![
                        InlineItem::Labeled {
                            label: "fA1".to_string(),
                            array_index: None,
                            ty: "Fragment".to_string(),
                            props: vec![Property {
                                name: "content".to_string(),
                                value: Expr::Str("a1".to_string()),
                                value_span: None,
                                trailing_comment: None,
                            }],
                            modifiers: vec![],
                            children: vec![],
                        },
                        InlineItem::Labeled {
                            label: "fA2".to_string(),
                            array_index: None,
                            ty: "Fragment".to_string(),
                            props: vec![Property {
                                name: "content".to_string(),
                                value: Expr::Str("a2".to_string()),
                                value_span: None,
                                trailing_comment: None,
                            }],
                            modifiers: vec![],
                            children: vec![],
                        },
                    ],
                    span: None,
                },
                Stmt::ActorDecl {
                    is_pub: false,
                    is_anonymous: false,
                    label: "eqB".to_string(),
                    array_index: None,
                    ty: "Equation".to_string(),
                    props: vec![],
                    modifiers: vec![],
                    children: vec![
                        InlineItem::Labeled {
                            label: "fB1".to_string(),
                            array_index: None,
                            ty: "Fragment".to_string(),
                            props: vec![Property {
                                name: "content".to_string(),
                                value: Expr::Str("b1".to_string()),
                                value_span: None,
                                trailing_comment: None,
                            }],
                            modifiers: vec![],
                            children: vec![],
                        },
                        InlineItem::Labeled {
                            label: "fB2".to_string(),
                            array_index: None,
                            ty: "Fragment".to_string(),
                            props: vec![Property {
                                name: "content".to_string(),
                                value: Expr::Str("b2".to_string()),
                                value_span: None,
                                trailing_comment: None,
                            }],
                            modifiers: vec![],
                            children: vec![],
                        },
                    ],
                    span: None,
                },
            ],
            span: None,
        }]
    }

    /// Helper: count opacity keyframes for a track by label.
    fn opacity_keyframe_count(
        report: &crate::diagnostics::BuildReport<Timeline>,
        label: &str,
    ) -> usize {
        report
            .output
            .tracks
            .get(label)
            .and_then(|t| t.highlight.highlight_opacity.as_ref())
            .map(|t| t.keyframes.len())
            .unwrap_or(0)
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

    #[test]
    fn exclusive_highlight_unhighlights_siblings() {
        // Equation with f1, f2, f3 — highlight f2, check siblings get unhighlighted
        let mut ast = make_equation_with_fragments(&["f1", "f2", "f3"]);
        if let Stmt::Keyframe { body, .. } = &mut ast[0] {
            body.push(Stmt::Action(
                Action {
                    verb: "highlight".to_string(),
                    targets: vec!["f2".to_string()],
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

        // f2 should have 2 keyframes (0 → 1)
        assert_eq!(
            opacity_keyframe_count(&report, "f2"),
            2,
            "f2 should have 2 opacity keyframes (highlight in)"
        );

        // f1 should have 2 keyframes (current opacity at t_start → 0)
        assert_eq!(
            opacity_keyframe_count(&report, "f1"),
            2,
            "f1 should have 2 opacity keyframes (unhighlight sibling)"
        );

        // f3 should have 2 keyframes (current opacity at t_start → 0)
        assert_eq!(
            opacity_keyframe_count(&report, "f3"),
            2,
            "f3 should have 2 opacity keyframes (unhighlight sibling)"
        );

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
    fn multi_target_highlight_exclusive() {
        // Equation with f1, f2, f3 — highlight f1, f2 together
        // Both should be highlighted, f3 should be unhighlighted
        let mut ast = make_equation_with_fragments(&["f1", "f2", "f3"]);
        if let Stmt::Keyframe { body, .. } = &mut ast[0] {
            body.push(Stmt::Action(
                Action {
                    verb: "highlight".to_string(),
                    targets: vec!["f1".to_string(), "f2".to_string()],
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

        // f1 and f2 should be highlighted (2 keyframes each: 0 → 1)
        assert_eq!(
            opacity_keyframe_count(&report, "f1"),
            2,
            "f1 should have 2 opacity keyframes (highlight in)"
        );
        assert_eq!(
            opacity_keyframe_count(&report, "f2"),
            2,
            "f2 should have 2 opacity keyframes (highlight in)"
        );

        // f3 should be unhighlighted (2 keyframes: current → 0)
        assert_eq!(
            opacity_keyframe_count(&report, "f3"),
            2,
            "f3 should have 2 opacity keyframes (unhighlight sibling)"
        );

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
    fn different_equations_independent() {
        // Two Equations, each with Fragments. Highlight in eqA should NOT
        // affect Fragments in eqB.
        let mut ast = make_two_equations();
        if let Stmt::Keyframe { body, .. } = &mut ast[0] {
            body.push(Stmt::Action(
                Action {
                    verb: "highlight".to_string(),
                    targets: vec!["fA1".to_string()],
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

        // fA1 should be highlighted
        assert_eq!(
            opacity_keyframe_count(&report, "fA1"),
            2,
            "fA1 should have 2 opacity keyframes (highlight in)"
        );

        // fA2 should be unhighlighted (sibling in same Equation)
        assert_eq!(
            opacity_keyframe_count(&report, "fA2"),
            2,
            "fA2 should have 2 opacity keyframes (unhighlight sibling)"
        );

        // fB1 and fB2 should be UNTOUCHED (different Equation)
        assert_eq!(
            opacity_keyframe_count(&report, "fB1"),
            0,
            "fB1 should have NO opacity keyframes"
        );
        assert_eq!(
            opacity_keyframe_count(&report, "fB2"),
            0,
            "fB2 should have NO opacity keyframes"
        );

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
    fn highlight_with_color_and_blend_no_unsupported_modifier_warning() {
        // `color` and `blend` are action-effect modifiers declared in highlight's signature;
        // parse_timing_modifiers must not emit UnsupportedModifierKey for them.
        let mut ast = make_equation_with_fragment();
        if let Stmt::Keyframe { body, .. } = &mut ast[0] {
            body.push(Stmt::Action(
                Action {
                    verb: "highlight".to_string(),
                    targets: vec!["f1".to_string()],
                    args: vec![],
                    modifiers: vec![
                        Modifier {
                            name: None,
                            value: Expr::Ident("800ms".to_string()),
                        },
                        Modifier {
                            name: Some("color".to_string()),
                            value: Expr::Ident("RED".to_string()),
                        },
                        Modifier {
                            name: Some("blend".to_string()),
                            value: Expr::Str("difference".to_string()),
                        },
                    ],
                    byte_span: None,
                },
                None,
            ));
        }

        let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());

        let unsupported_key_warnings = report
            .diagnostics
            .iter()
            .filter(|d| {
                d.code == animatix_syntax::diagnostics::DiagnosticCode::UnsupportedModifierKey
                    && d.severity == crate::diagnostics::DiagnosticSeverity::Warning
            })
            .count();
        assert_eq!(
            unsupported_key_warnings, 0,
            "highlight with color/blend should not produce UnsupportedModifierKey warnings; got: {:?}",
            report.diagnostics
        );
    }
}
