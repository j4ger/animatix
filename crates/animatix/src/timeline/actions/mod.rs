pub mod effects;
pub mod entrance;
pub mod exit;
pub mod motion;
pub mod registry;
pub mod reorder;
pub mod reveal;

use crate::ast::Action;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::Timeline;
use effects::{Bounce, Pulse, Shake};
use entrance::{FadeIn, WipeIn};
use exit::FadeOut;
use motion::{Move, Rotate, Scale, Shift};
use registry::{ActionSignature, BuiltinAction};
use reorder::{Reorder, Swap};
use reveal::{DrawIn, DrawOut, RevealOut, WipeOut};

fn push_unknown_action_diagnostic(
    action: &Action,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) {
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::UnknownAction,
            DiagnosticPhase::Build,
            format!(
                "Unknown action '{}'; no built-in action matches this name.",
                action.verb
            ),
        )
        .with_subject(&action.verb)
        .with_ast_span(span),
    );
}

fn push_unsupported_action_target_diagnostic(
    verb: &str,
    target: &str,
    reason: &str,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) {
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::UnsupportedActionTarget,
            DiagnosticPhase::Build,
            format!("Action '{verb}' does not support target '{target}': {reason}."),
        )
        .with_subject(format!("{verb} {target}"))
        .with_ast_span(span),
    );
}

pub(crate) fn ensure_target_exists(
    timeline: &Timeline,
    target: &str,
    verb: &str,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) -> bool {
    if timeline.tracks.contains_key(target) {
        return true;
    }

    push_unsupported_action_target_diagnostic(
        verb,
        target,
        "the target is not declared yet",
        diagnostics,
        span,
    );
    false
}

pub(crate) fn ensure_vector_reveal_target(
    timeline: &Timeline,
    target: &str,
    verb: &str,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) -> bool {
    let Some(track) = timeline.tracks.get(target) else {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "the target is not declared yet",
            diagnostics,
            span,
        );
        return false;
    };

    if track.image.as_ref().map(|t| t.last_value()).flatten().is_some() {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "image targets only support opacity-based actions right now",
            diagnostics,
            span,
        );
        return false;
    }

    if track.text_paths.as_ref().map(|t| !t.default_value.is_empty() || !t.keyframes.is_empty()).unwrap_or(false) {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "text-like targets only support opacity-based actions right now",
            diagnostics,
            span,
        );
        return false;
    }

    if timeline
        .tracks
        .get(target)
        .is_some_and(|track| !track.children.is_empty())
        && track.vector_paths.as_ref().map(|t| t.default_value.is_empty() && t.keyframes.is_empty()).unwrap_or(true)
        && track.svg_paths.is_empty()
    {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "the target resolves to a container/group node rather than a renderable leaf; vector reveal actions must target leaf actors with vector paths",
            diagnostics,
            span,
        );
        return false;
    }

    true
}

/// Returns a list of all registered built-in actions.
fn get_builtin_actions() -> Vec<Box<dyn BuiltinAction>> {
    vec![
        Box::new(FadeIn),
        Box::new(WipeIn),
        Box::new(Move),
        Box::new(Shift),
        Box::new(Rotate),
        Box::new(Scale),
        Box::new(DrawIn),
        Box::new(DrawOut),
        Box::new(FadeOut),
        Box::new(WipeOut),
        Box::new(RevealOut),
        Box::new(Shake),
        Box::new(Pulse),
        Box::new(Bounce),
        Box::new(Swap),
        Box::new(Reorder),
    ]
}

/// Looks up the action by verb and executes it if found.
pub fn process_action(
    action: &Action,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) {
    let actions = get_builtin_actions();
    for builtin in actions {
        if builtin.signature().name == action.verb {
            builtin.execute(action, time_ms, timeline, diagnostics);
            return;
        }
    }

    push_unknown_action_diagnostic(action, diagnostics, span);
}

/// Exposes all action signatures for LSP/UI integration.
pub fn get_action_signatures() -> Vec<ActionSignature> {
    get_builtin_actions()
        .iter()
        .map(|a| a.signature())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Action, Modifier};
    use crate::diagnostics::DiagnosticCode;
    use crate::timeline::{
        AnimationTrack, ContainerLayoutChild, ContainerMetadata, LayoutType, PropertyTrack,
    };

    #[test]
    fn unknown_actions_emit_diagnostics() {
        let action = Action {
            verb: "spin-in".to_string(),
            targets: vec!["shape".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident("1s".to_string()),
            }],
            byte_span: None,
        };
        let mut timeline = Timeline::new();
        let mut diagnostics = Vec::new();

        process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::UnknownAction)
        );
    }

    #[test]
    fn action_signatures_include_reveal_out() {
        assert!(
            get_action_signatures()
                .iter()
                .any(|signature| signature.name == "reveal-out")
        );
    }

    #[test]
    fn action_signatures_include_draw_out() {
        assert!(
            get_action_signatures()
                .iter()
                .any(|signature| signature.name == "draw-out")
        );
    }

    #[test]
    fn vector_reveal_targets_reject_container_nodes_with_leaf_only_message() {
        let mut timeline = Timeline::new();
        let mut track = AnimationTrack::new("row".to_string());
        track.children.push("row.child".to_string());
        timeline.tracks.insert("row".to_string(), track);

        let mut diagnostics = Vec::new();
        let ok = ensure_vector_reveal_target(&timeline, "row", "draw-in", &mut diagnostics, None);

        assert!(!ok);
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == DiagnosticCode::UnsupportedActionTarget
                && diagnostic.message.contains("container/group node")
                && diagnostic.message.contains("leaf actors with vector paths")
        }));
    }

    #[test]
    fn swap_action_requires_exactly_two_targets() {
        let action = Action {
            verb: "swap".to_string(),
            targets: vec!["a".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident("500ms".to_string()),
            }],
            byte_span: None,
        };
        let mut timeline = Timeline::new();
        let mut diagnostics = Vec::new();

        process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

        assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::InvalidModifierValue));
        assert!(timeline.child_orders.is_empty());
    }

    #[test]
    fn swap_action_requires_existing_targets() {
        let action = Action {
            verb: "swap".to_string(),
            targets: vec!["a".to_string(), "b".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident("500ms".to_string()),
            }],
            byte_span: None,
        };
        let mut timeline = Timeline::new();
        let mut diagnostics = Vec::new();

        process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

        assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::UnsupportedActionTarget));
        assert!(timeline.child_orders.is_empty());
    }

    #[test]
    fn swap_action_requires_common_parent() {
        let mut timeline = Timeline::new();
        timeline.tracks.insert("a".to_string(), AnimationTrack::new("a".to_string()));
        timeline.tracks.insert("b".to_string(), AnimationTrack::new("b".to_string()));

        let action = Action {
            verb: "swap".to_string(),
            targets: vec!["a".to_string(), "b".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident("500ms".to_string()),
            }],
            byte_span: None,
        };
        let mut diagnostics = Vec::new();

        process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

        assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::UnsupportedActionTarget));
        assert!(timeline.child_orders.is_empty());
    }

    #[test]
    fn swap_action_sets_child_order_keyframe() {
        let mut timeline = Timeline::new();

        // Set up container with children
        let mut parent_track = AnimationTrack::new("row".to_string());
        parent_track.children.push("a".to_string());
        parent_track.children.push("b".to_string());
        timeline.tracks.insert("row".to_string(), parent_track);

        // Set up child tracks with layout size so they're admitted
        let mut child_a = AnimationTrack::new("a".to_string());
        child_a.layout_size = Some(PropertyTrack::new([15.0, 20.0]));
        timeline.tracks.insert("a".to_string(), child_a);

        let mut child_b = AnimationTrack::new("b".to_string());
        child_b.layout_size = Some(PropertyTrack::new([15.0, 40.0]));
        timeline.tracks.insert("b".to_string(), child_b);

        // Set up container metadata with layout children
        timeline.container_metadata.insert(
            "row".to_string(),
            ContainerMetadata {
                layout_type: LayoutType::Row,
                gap: 8.0,
                padding: 0.0,
                align: "center".to_string(),
                cols: None,
                child_order: vec!["a".to_string(), "b".to_string()],
                layout_children: vec![
                    ContainerLayoutChild { label: "a".to_string() },
                    ContainerLayoutChild { label: "b".to_string() },
                ],
            },
        );

        let action = Action {
            verb: "swap".to_string(),
            targets: vec!["a".to_string(), "b".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident("500ms".to_string()),
            }],
            byte_span: None,
        };
        let mut diagnostics = Vec::new();

        process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

        assert!(diagnostics.is_empty(), "unexpected diagnostics: {:?}", diagnostics);
        assert_eq!(timeline.child_orders.len(), 1);

        let track = timeline.child_orders.get("row").unwrap();
        assert_eq!(track.keyframes.len(), 1);
        let (order, _) = track.keyframes.get(&500).unwrap();
        assert_eq!(order, &vec!["b".to_string(), "a".to_string()]);
    }

    #[test]
    fn swap_action_detects_overlapping_swaps() {
        let mut timeline = Timeline::new();

        // Set up container with children
        let mut parent_track = AnimationTrack::new("row".to_string());
        parent_track.children.push("a".to_string());
        parent_track.children.push("b".to_string());
        parent_track.children.push("c".to_string());
        timeline.tracks.insert("row".to_string(), parent_track);

        for label in ["a", "b", "c"] {
            let mut child = AnimationTrack::new(label.to_string());
            child.layout_size = Some(PropertyTrack::new([15.0, 20.0]));
            timeline.tracks.insert(label.to_string(), child);
        }

        timeline.container_metadata.insert(
            "row".to_string(),
            ContainerMetadata {
                layout_type: LayoutType::Row,
                gap: 8.0,
                padding: 0.0,
                align: "center".to_string(),
                cols: None,
                child_order: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                layout_children: vec![
                    ContainerLayoutChild { label: "a".to_string() },
                    ContainerLayoutChild { label: "b".to_string() },
                    ContainerLayoutChild { label: "c".to_string() },
                ],
            },
        );

        // First swap: a,b at 0s, completes at 500ms
        let action1 = Action {
            verb: "swap".to_string(),
            targets: vec!["a".to_string(), "b".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident("500ms".to_string()),
            }],
            byte_span: None,
        };
        let mut diagnostics = Vec::new();
        process_action(&action1, 0.0, &mut timeline, &mut diagnostics, None);
        assert!(diagnostics.is_empty());

        // Second swap: b,c at 200ms, would complete at 700ms — overlaps with first
        let action2 = Action {
            verb: "swap".to_string(),
            targets: vec!["b".to_string(), "c".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident("500ms".to_string()),
            }],
            byte_span: None,
        };
        let mut diagnostics2 = Vec::new();
        process_action(&action2, 200.0, &mut timeline, &mut diagnostics2, None);

        assert!(diagnostics2.iter().any(|d| d.code == DiagnosticCode::ConflictingModifierKey));
        // Only the first swap's keyframe should exist
        let track = timeline.child_orders.get("row").unwrap();
        assert_eq!(track.keyframes.len(), 1);
    }

    #[test]
    fn reorder_action_requires_exactly_one_target() {
        let action = Action {
            verb: "reorder".to_string(),
            targets: vec![],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
        };
        let mut timeline = Timeline::new();
        let mut diagnostics = Vec::new();

        process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

        assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::InvalidModifierValue));
        assert!(timeline.child_orders.is_empty());
    }

    #[test]
    fn reorder_action_requires_order_modifier() {
        let mut timeline = Timeline::new();
        timeline.tracks.insert("row".to_string(), AnimationTrack::new("row".to_string()));

        let action = Action {
            verb: "reorder".to_string(),
            targets: vec!["row".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident("500ms".to_string()),
            }],
            byte_span: None,
        };
        let mut diagnostics = Vec::new();

        process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

        assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::InvalidModifierValue));
        assert!(timeline.child_orders.is_empty());
    }

    #[test]
    fn reorder_action_sets_child_order_keyframe() {
        let mut timeline = Timeline::new();

        // Set up container with children
        let mut parent_track = AnimationTrack::new("row".to_string());
        parent_track.children.push("a".to_string());
        parent_track.children.push("b".to_string());
        parent_track.children.push("c".to_string());
        timeline.tracks.insert("row".to_string(), parent_track);

        for label in ["a", "b", "c"] {
            let mut child = AnimationTrack::new(label.to_string());
            child.layout_size = Some(PropertyTrack::new([15.0, 20.0]));
            timeline.tracks.insert(label.to_string(), child);
        }

        timeline.container_metadata.insert(
            "row".to_string(),
            ContainerMetadata {
                layout_type: LayoutType::Row,
                gap: 8.0,
                padding: 0.0,
                align: "center".to_string(),
                cols: None,
                child_order: vec!["a".to_string(), "b".to_string(), "c".to_string()],
                layout_children: vec![
                    ContainerLayoutChild { label: "a".to_string() },
                    ContainerLayoutChild { label: "b".to_string() },
                    ContainerLayoutChild { label: "c".to_string() },
                ],
            },
        );

        let action = Action {
            verb: "reorder".to_string(),
            targets: vec!["row".to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("order".to_string()),
                    value: crate::ast::Expr::Tuple(vec![
                        crate::ast::Expr::Ident("c".to_string()),
                        crate::ast::Expr::Ident("b".to_string()),
                        crate::ast::Expr::Ident("a".to_string()),
                    ]),
                },
                Modifier {
                    name: None,
                    value: crate::ast::Expr::Ident("500ms".to_string()),
                },
            ],
            byte_span: None,
        };
        let mut diagnostics = Vec::new();

        process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

        assert!(diagnostics.is_empty(), "unexpected diagnostics: {:?}", diagnostics);
        assert_eq!(timeline.child_orders.len(), 1);

        let track = timeline.child_orders.get("row").unwrap();
        assert_eq!(track.keyframes.len(), 1);
        let (order, _) = track.keyframes.get(&500).unwrap();
        assert_eq!(order, &vec!["c".to_string(), "b".to_string(), "a".to_string()]);
    }

    #[test]
    fn reorder_action_detects_overlapping_reorders() {
        let mut timeline = Timeline::new();

        // Set up container with children
        let mut parent_track = AnimationTrack::new("row".to_string());
        parent_track.children.push("a".to_string());
        parent_track.children.push("b".to_string());
        timeline.tracks.insert("row".to_string(), parent_track);

        for label in ["a", "b"] {
            let mut child = AnimationTrack::new(label.to_string());
            child.layout_size = Some(PropertyTrack::new([15.0, 20.0]));
            timeline.tracks.insert(label.to_string(), child);
        }

        timeline.container_metadata.insert(
            "row".to_string(),
            ContainerMetadata {
                layout_type: LayoutType::Row,
                gap: 8.0,
                padding: 0.0,
                align: "center".to_string(),
                cols: None,
                child_order: vec!["a".to_string(), "b".to_string()],
                layout_children: vec![
                    ContainerLayoutChild { label: "a".to_string() },
                    ContainerLayoutChild { label: "b".to_string() },
                ],
            },
        );

        // First reorder at 0s, completes at 500ms
        let action1 = Action {
            verb: "reorder".to_string(),
            targets: vec!["row".to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("order".to_string()),
                    value: crate::ast::Expr::Tuple(vec![
                        crate::ast::Expr::Ident("b".to_string()),
                        crate::ast::Expr::Ident("a".to_string()),
                    ]),
                },
                Modifier {
                    name: None,
                    value: crate::ast::Expr::Ident("500ms".to_string()),
                },
            ],
            byte_span: None,
        };
        let mut diagnostics = Vec::new();
        process_action(&action1, 0.0, &mut timeline, &mut diagnostics, None);
        assert!(diagnostics.is_empty());

        // Second reorder at 200ms, would complete at 700ms — overlaps with first
        let action2 = Action {
            verb: "reorder".to_string(),
            targets: vec!["row".to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("order".to_string()),
                    value: crate::ast::Expr::Tuple(vec![
                        crate::ast::Expr::Ident("a".to_string()),
                        crate::ast::Expr::Ident("b".to_string()),
                    ]),
                },
                Modifier {
                    name: None,
                    value: crate::ast::Expr::Ident("500ms".to_string()),
                },
            ],
            byte_span: None,
        };
        let mut diagnostics2 = Vec::new();
        process_action(&action2, 200.0, &mut timeline, &mut diagnostics2, None);

        assert!(diagnostics2.iter().any(|d| d.code == DiagnosticCode::ConflictingModifierKey));
        let track = timeline.child_orders.get("row").unwrap();
        assert_eq!(track.keyframes.len(), 1);
    }
}
