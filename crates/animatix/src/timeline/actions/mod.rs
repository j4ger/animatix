/// Visual effect actions (bounce, pulse, shake).
pub mod effects;
/// Entrance actions that bring actors into view (fade-in, wipe-in).
pub mod entrance;
/// Exit actions that remove actors from view (fade-out).
pub mod exit;
/// Highlight actions for equation fragments (highlight, unhighlight).
pub mod highlight;
/// Motion actions that transform actor placement (move, shift, rotate, scale).
pub mod motion;
/// Action registry types: signatures, parameters, and the [`BuiltinAction`] trait.
pub mod registry;
/// Persistence actions (persist, remove) for multi-scene carry-forward.
pub mod persistence;
/// Reorder actions that change container child order (swap, reorder).
pub mod reorder;
/// Reveal actions that animate vector stroke and fill visibility
/// (draw-in, reveal-in, wipe-out, reveal-out, draw-out).
pub mod reveal;

use crate::ast::Action;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::timeline::property_track::{Interpolate, PropertyTrack, TrackAccessor};
use crate::timeline::actor_kind::ActorKindId;
use crate::timeline::Timeline;
use tracing::{debug, instrument, warn};
use effects::{Bounce, Pulse, Shake};
use entrance::{FadeIn, WipeIn};
use exit::FadeOut;
use highlight::{Highlight, Unhighlight};
use persistence::{Persist, Remove};
use motion::{Move, Rotate, Scale, Shift};
use registry::{ActionSignature, BuiltinAction};
use reorder::{Reorder, Swap};
use reveal::{DrawIn, DrawOut, RevealIn, RevealOut, WipeOut};

fn push_unknown_action_diagnostic(
    action: &Action,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) {
    diagnostics.push(
        Diagnostic::error(
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
        Diagnostic::error(
            DiagnosticCode::UnsupportedActionTarget,
            DiagnosticPhase::Build,
            format!("Action '{verb}' does not support target '{target}': {reason}."),
        )
        .with_subject(format!("{verb} {target}"))
        .with_ast_span(span),
    );
}

/// Insert a guard keyframe at `guard_time` for a single property track if one doesn't already exist.
/// This preserves the pre-delay value for instant-change actions (duration == 0) with delay.
pub(crate) fn ensure_guard_keyframe<T: Interpolate>(
    track: &mut Option<PropertyTrack<T>>,
    guard_time: u64,
    default: T,
) {
    if !track.has_keyframe_at(guard_time) {
        let prior = track.get(guard_time, default.clone());
        track.ensure(default).add_keyframe(guard_time, prior, Easing::Linear);
    }
}

/// Resolve a possibly-dotted action target (e.g. `"decomp_eq.f1"`) to the
/// actual track key in `timeline.tracks`.
///
/// Resolution order:
/// 1. If the full string is already a track key, return it directly.
/// 2. Split by `.` and walk the parent→child hierarchy. Return the leaf
///    track key if the path is valid.
/// 3. Return `None` if the path cannot be resolved.
fn resolve_action_target(timeline: &Timeline, target: &str) -> Option<String> {
    // Fast path: direct lookup (handles plain identifiers and pre-built keys)
    if timeline.tracks.contains_key(target) {
        return Some(target.to_string());
    }

    // Dotted path: walk hierarchy
    let segments: Vec<&str> = target.split('.').collect();
    if segments.len() < 2 {
        return None;
    }

    let mut current = segments[0].to_string();
    for &segment in &segments[1..] {
        let track = timeline.tracks.get(&current)?;
        if track.children.contains(&segment.to_string()) {
            current = segment.to_string();
        } else {
            return None;
        }
    }

    Some(current)
}

pub(crate) fn ensure_target_exists(
    timeline: &Timeline,
    target: &str,
    verb: &str,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) -> bool {
    if resolve_action_target(timeline, target).is_some() {
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

/// Expand group targets into their leaf children recursively.
/// A Group is identified by having children in its track. The group itself
/// is skipped; only non-group descendants are returned.
///
/// Container-only actions (reorder, swap) skip expansion — they need
/// to target the container itself.
pub(crate) fn expand_group_targets(
    timeline: &Timeline,
    targets: &[String],
    verb: &str,
) -> Vec<String> {
    // These actions operate on containers, not leaves (or need the root label
    // for metadata/flag purposes — expand would lose the container identity).
    let container_actions = ["reorder", "swap", "persist", "remove"];
    if container_actions.contains(&verb) {
        return targets.to_vec();
    }

    let mut result = Vec::new();
    let mut stack: Vec<String> = targets.to_vec();

    while let Some(label) = stack.pop() {
        // Resolve dotted paths (e.g. "decomp_eq.f1" → "f1") before track lookup
        let resolved = resolve_action_target(timeline, &label).unwrap_or(label.clone());
        if let Some(track) = timeline.tracks.get(&resolved) {
            if track.children.is_empty() {
                // Leaf actor — keep the resolved key
                result.push(resolved);
            } else if is_layout_container(track.kind) {
                // Layout container (Row, Col, Grid, etc.) — recurse into children
                for child in track.children.iter().rev() {
                    stack.push(child.clone());
                }
            } else {
                // Non-layout container with children (e.g. Graph with tick labels)
                // — keep the container itself as the target
                result.push(resolved);
            }
        } else {
            // Target doesn't exist — pass through and let the action handler report it
            result.push(label);
        }
    }

    result
}

/// Returns true if the actor kind is a layout container whose children
/// should be expanded by `expand_group_targets`. Plot containers like
/// Graph (which have tick label children) are NOT layout containers.
fn is_layout_container(kind: ActorKindId) -> bool {
    matches!(
        kind,
        ActorKindId::Row
            | ActorKindId::Col
            | ActorKindId::Grid
            | ActorKindId::Stack
            | ActorKindId::Group
            | ActorKindId::Mask
            | ActorKindId::Filter
    )
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

    if track.kind == crate::timeline::ActorKindId::Image {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "image targets only support opacity-based actions right now",
            diagnostics,
            span,
        );
        return false;
    }

    #[cfg(feature = "render")]
    if track.image.as_ref().and_then(|t| t.last_value()).is_some() {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "image targets only support opacity-based actions right now",
            diagnostics,
            span,
        );
        return false;
    }

    if track.text.text_paths.as_ref().map(|t| !t.default_value.is_empty() || !t.keyframes.is_empty()).unwrap_or(false) {
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
        && track.shape.vector_paths.as_ref().map(|t| t.default_value.is_empty() && t.keyframes.is_empty()).unwrap_or(true)
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
        Box::new(RevealIn),
        Box::new(DrawOut),
        Box::new(FadeOut),
        Box::new(Persist),
        Box::new(Remove),
        Box::new(WipeOut),
        Box::new(RevealOut),
        Box::new(Shake),
        Box::new(Pulse),
        Box::new(Bounce),
        Box::new(Highlight),
        Box::new(Unhighlight),
        Box::new(Swap),
        Box::new(Reorder),
    ]
}

/// Looks up the action by verb and executes it if found.
/// Group targets are automatically expanded into their leaf children.
#[instrument(skip(timeline, diagnostics), fields(verb = %action.verb, targets = ?action.targets))]
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
            let expanded_targets = expand_group_targets(timeline, &action.targets, &action.verb);
            if expanded_targets != action.targets {
                debug!(
                    "Expanded group targets for '{}': {:?} -> {:?}",
                    action.verb, action.targets, expanded_targets
                );
            }
            let mut expanded_action = action.clone();
            expanded_action.targets = expanded_targets;
            builtin.execute(&expanded_action, time_ms, timeline, diagnostics);
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
        AnimationTrack, ContainerMetadata, LayoutType, PropertyTrack,
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
    fn action_signatures_include_reveal_in() {
        assert!(
            get_action_signatures()
                .iter()
                .any(|signature| signature.name == "reveal-in")
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
        child_a.geometry.layout_size = Some(PropertyTrack::new([15.0, 20.0]));
        timeline.tracks.insert("a".to_string(), child_a);

        let mut child_b = AnimationTrack::new("b".to_string());
        child_b.geometry.layout_size = Some(PropertyTrack::new([15.0, 40.0]));
        timeline.tracks.insert("b".to_string(), child_b);

        // Set up container metadata with layout children
        timeline.container_metadata.insert(
            "row".to_string(),
            ContainerMetadata {
                layout_type: LayoutType::Row,
                gap: [8.0, 8.0],
                padding: [0.0, 0.0, 0.0, 0.0],
                align: "center".to_string(),
                vertical_align: "center".to_string(),
                cols: None,
                child_order: vec!["a".to_string(), "b".to_string()],
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
        assert_eq!(track.keyframes.len(), 2);
        let (start_order, _) = track.keyframes.get(&0).unwrap();
        assert_eq!(start_order, &vec!["a".to_string(), "b".to_string()]);
        let (end_order, _) = track.keyframes.get(&500).unwrap();
        assert_eq!(end_order, &vec!["b".to_string(), "a".to_string()]);
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
            child.geometry.layout_size = Some(PropertyTrack::new([15.0, 20.0]));
            timeline.tracks.insert(label.to_string(), child);
        }

        timeline.container_metadata.insert(
            "row".to_string(),
            ContainerMetadata {
                layout_type: LayoutType::Row,
                gap: [8.0, 8.0],
                padding: [0.0, 0.0, 0.0, 0.0],
                align: "center".to_string(),
                vertical_align: "center".to_string(),
                cols: None,
                child_order: vec!["a".to_string(), "b".to_string(), "c".to_string()],
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
        // Only the first swap's keyframes should exist (start + end)
        let track = timeline.child_orders.get("row").unwrap();
        assert_eq!(track.keyframes.len(), 2);
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
            child.geometry.layout_size = Some(PropertyTrack::new([15.0, 20.0]));
            timeline.tracks.insert(label.to_string(), child);
        }

        timeline.container_metadata.insert(
            "row".to_string(),
            ContainerMetadata {
                layout_type: LayoutType::Row,
                gap: [8.0, 8.0],
                padding: [0.0, 0.0, 0.0, 0.0],
                align: "center".to_string(),
                vertical_align: "center".to_string(),
                cols: None,
                child_order: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            },
        );

        let action = Action {
            verb: "reorder".to_string(),
            targets: vec!["row".to_string()],
            args: vec![],
            modifiers: vec![
                Modifier {
                    name: Some("order".to_string()),
                    value: crate::ast::Expr::List(vec![
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
        assert_eq!(track.keyframes.len(), 2);
        let (start_order, _) = track.keyframes.get(&0).unwrap();
        assert_eq!(start_order, &vec!["a".to_string(), "b".to_string(), "c".to_string()]);
        let (end_order, _) = track.keyframes.get(&500).unwrap();
        assert_eq!(end_order, &vec!["c".to_string(), "b".to_string(), "a".to_string()]);
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
            child.geometry.layout_size = Some(PropertyTrack::new([15.0, 20.0]));
            timeline.tracks.insert(label.to_string(), child);
        }

        timeline.container_metadata.insert(
            "row".to_string(),
            ContainerMetadata {
                layout_type: LayoutType::Row,
                gap: [8.0, 8.0],
                padding: [0.0, 0.0, 0.0, 0.0],
                align: "center".to_string(),
                vertical_align: "center".to_string(),
                cols: None,
                child_order: vec!["a".to_string(), "b".to_string()],
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
                    value: crate::ast::Expr::List(vec![
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
                    value: crate::ast::Expr::List(vec![
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
        // Only the first reorder's keyframes should exist (start + end)
        let track = timeline.child_orders.get("row").unwrap();
        assert_eq!(track.keyframes.len(), 2);
    }

    #[test]
    fn swap_action_bounds_transition_to_duration() {
        // Regression test: consecutive swaps should not create a transition
        // that spans the gap between the end of swap N and the end of swap N+1.
        let mut timeline = Timeline::new();

        let mut parent_track = AnimationTrack::new("row".to_string());
        parent_track.children = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        timeline.tracks.insert("row".to_string(), parent_track);

        for label in ["a", "b", "c"] {
            let mut child = AnimationTrack::new(label.to_string());
            child.geometry.layout_size = Some(PropertyTrack::new([15.0, 20.0]));
            timeline.tracks.insert(label.to_string(), child);
        }

        timeline.container_metadata.insert(
            "row".to_string(),
            ContainerMetadata {
                layout_type: LayoutType::Row,
                gap: [8.0, 8.0],
                padding: [0.0, 0.0, 0.0, 0.0],
                align: "center".to_string(),
                vertical_align: "center".to_string(),
                cols: None,
                child_order: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            },
        );

        // First swap at 0s, 500ms duration → keyframes at 0 and 500
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
        process_action(&action1, 0.0, &mut timeline, &mut Vec::new(), None);

        // Second swap at 1000ms, 500ms duration → keyframes at 1000 and 1500
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
        process_action(&action2, 1000.0, &mut timeline, &mut Vec::new(), None);

        let track = timeline.child_orders.get("row").unwrap();
        assert_eq!(track.keyframes.len(), 4, "expected start+end for each swap");

        // At 600ms (after first swap ends, before second starts) the order
        // should be stable — not interpolating toward the second swap.
        let layout_600 = timeline.compute_animated_layout("row", 600);
        let layout_500 = timeline.compute_animated_layout("row", 500);
        assert_eq!(layout_600, layout_500, "order should be stable between swaps");

        // At 1250ms (midway through second swap) the order should be
        // interpolating, not already finished.
        let layout_1000 = timeline.compute_animated_layout("row", 1000);
        let layout_1500 = timeline.compute_animated_layout("row", 1500);
        let layout_1250 = timeline.compute_animated_layout("row", 1250);
        assert_ne!(layout_1250, layout_1000, "should be mid-transition at 1250ms");
        assert_ne!(layout_1250, layout_1500, "should not be finished at 1250ms");
    }

    #[test]
    fn swap_action_animates_positions_smoothly() {
        // Regression test: verify that bar positions actually change during a swap.
        let mut timeline = Timeline::new();
        timeline.dynamic_layout = true;

        let mut parent_track = AnimationTrack::new("bars".to_string());
        parent_track.children = vec![
            "bar1".to_string(),
            "bar2".to_string(),
            "bar3".to_string(),
            "bar4".to_string(),
            "bar5".to_string(),
        ];
        timeline.tracks.insert("bars".to_string(), parent_track);

        let sizes: [f32; 2] = [30.0, 40.0]; // half-size (60x80 rects)
        for label in ["bar1", "bar2", "bar3", "bar4", "bar5"] {
            let mut child = AnimationTrack::new(label.to_string());
            child.geometry.layout_size = Some(PropertyTrack::new(sizes));
            timeline.tracks.insert(label.to_string(), child);
        }

        timeline.container_metadata.insert(
            "bars".to_string(),
            ContainerMetadata {
                layout_type: LayoutType::Row,
                gap: [8.0, 8.0],
                padding: [0.0, 0.0, 0.0, 0.0],
                align: "bottom".to_string(),
                vertical_align: "center".to_string(),
                cols: None,
                child_order: vec![
                    "bar1".to_string(),
                    "bar2".to_string(),
                    "bar3".to_string(),
                    "bar4".to_string(),
                    "bar5".to_string(),
                ],
            },
        );

        let action = Action {
            verb: "swap".to_string(),
            targets: vec!["bar1".to_string(), "bar2".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident("500ms".to_string()),
            }],
            byte_span: None,
        };
        process_action(&action, 2000.0, &mut timeline, &mut Vec::new(), None);

        // Positions at start (2000ms) and end (2500ms) should differ for swapped bars
        let layout_start = timeline.compute_animated_layout("bars", 2000);
        let layout_end = timeline.compute_animated_layout("bars", 2500);

        let bar1_start = layout_start.get("bar1").copied().unwrap();
        let bar1_end = layout_end.get("bar1").copied().unwrap();
        let bar2_start = layout_start.get("bar2").copied().unwrap();
        let bar2_end = layout_end.get("bar2").copied().unwrap();

        assert_ne!(
            bar1_start, bar1_end,
            "bar1 should move during swap (start={:?}, end={:?})",
            bar1_start, bar1_end
        );
        assert_ne!(
            bar2_start, bar2_end,
            "bar2 should move during swap (start={:?}, end={:?})",
            bar2_start, bar2_end
        );

        // Mid-transition position should be between start and end
        let layout_mid = timeline.compute_animated_layout("bars", 2250);
        let bar1_mid = layout_mid.get("bar1").copied().unwrap();
        let bar2_mid = layout_mid.get("bar2").copied().unwrap();

        assert_ne!(bar1_mid, bar1_start, "bar1 should have left start position");
        assert_ne!(bar1_mid, bar1_end, "bar1 should not yet be at end position");
        assert_ne!(bar2_mid, bar2_start, "bar2 should have left start position");
        assert_ne!(bar2_mid, bar2_end, "bar2 should not yet be at end position");
    }
}
