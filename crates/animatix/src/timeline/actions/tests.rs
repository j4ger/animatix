//! Action tests.

use std::collections::HashMap;
use std::sync::Arc;

use super::*;
use crate::ast::{Action, Modifier};
use crate::diagnostics::DiagnosticCode;
use crate::primitives::{BuildCtx, Primitive};
use crate::timeline::{
    ActorCategory, AnimationTrack, ContainerMetadata, LayoutType, PropertyTrack,
};

struct FlexContainer;

impl Primitive for FlexContainer {
    fn type_name(&self) -> &str {
        "FlexContainer"
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[crate::ast::Property],
        _modifiers: &[crate::ast::Modifier],
        _children: &[crate::ast::InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        let track_id = ctx.timeline.world.spawn(label, "Rect");
        ctx.timeline.world.rebuild_property_plan(track_id);
        Ok(())
    }
}

#[test]
fn extension_layout_container_expands_for_leaf_actions() {
    let (ast, errors) =
        animatix_syntax::parser::parse_source("root: FlexContainer { a: Rect, b: Rect }");
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("parsed AST");

    let mut registry = crate::primitives::PrimitiveRegistry::new();
    let mut flex_info =
        animatix_std::PrimitiveInfo::extension("FlexContainer", ActorCategory::Container);
    flex_info.capabilities.layout_container = true;
    flex_info.capabilities.is_container = true;
    registry
        .register(Arc::new(FlexContainer), flex_info)
        .expect("register FlexContainer");
    let report = Timeline::build_with_primitive_registry(&ast, &HashMap::new(), Arc::new(registry));
    let build_unexpected: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| !matches!(d.code, animatix_syntax::diagnostics::DiagnosticCode::NeverRevealed))
        .collect();
    assert!(build_unexpected.is_empty(), "unexpected diagnostics: {build_unexpected:?}");
    let mut timeline = report.output;
    let root_id = timeline.get_entity_id("root").expect("root track");
    let children: Vec<&str> = timeline.world.child_labels(root_id).collect();
    assert_eq!(children, vec!["a", "b"]);

    let action = Action {
        verb: "fade-in".to_string(),
        targets: vec!["root".to_string()],
        args: vec![],
        modifiers: vec![Modifier {
            name: None,
            value: crate::ast::Expr::Ident("1s".to_string()),
        }],
        byte_span: None,
        target_index: vec![],
    };
    let mut diagnostics = Vec::new();
    process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);
    let unexpected: Vec<_> = diagnostics
        .iter()
        .filter(|d| !matches!(d.code, animatix_syntax::diagnostics::DiagnosticCode::NeverRevealed))
        .collect();
    assert!(unexpected.is_empty(), "unexpected diagnostics: {unexpected:?}");
    assert!(
        timeline
            .world
            .style(timeline.world.get_id("root").expect("root"))
            .opacity
            .is_none(),
        "layout container itself should not receive the leaf action"
    );
    for label in ["a", "b"] {
        let opacity = timeline
            .world
            .style(timeline.world.get_id(label).expect(label))
            .opacity
            .get(0, 1.0);
        assert_eq!(opacity, 0.0, "{label} should receive the expanded leaf action");
    }
}

#[test]
fn runtime_action_names_match_builtins_registry() {
    let mut runtime: Vec<String> =
        get_builtin_actions().iter().map(|a| a.signature().name.clone()).collect();
    let mut registry: Vec<String> =
        animatix_syntax::builtins::ACTIONS.iter().map(|s| s.to_string()).collect();
    runtime.sort();
    registry.sort();
    assert_eq!(
        runtime, registry,
        "runtime action verbs drifted from animatix-syntax::builtins::ACTIONS"
    );
}

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
        target_index: vec![],
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
    assert!(get_action_signatures().iter().any(|signature| signature.name == "reveal-out"));
}

#[test]
fn action_signatures_include_draw_out() {
    assert!(get_action_signatures().iter().any(|signature| signature.name == "draw-out"));
}

#[test]
fn action_signatures_include_reveal_in() {
    assert!(get_action_signatures().iter().any(|signature| signature.name == "reveal-in"));
}

#[test]
fn vector_reveal_targets_reject_container_nodes_with_leaf_only_message() {
    let mut timeline = Timeline::new();
    let row_id = timeline.spawn_entity("row", "Rect");
    let child_id = timeline.spawn_entity("row.child", "Rect");
    timeline.world.link(row_id, child_id);

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
        target_index: vec![],
    };
    let mut timeline = Timeline::new();
    let mut diagnostics = Vec::new();

    process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

    assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::InvalidModifierValue));
    assert_eq!(timeline.child_orders_iter().count(), 0);
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
        target_index: vec![],
    };
    let mut timeline = Timeline::new();
    let mut diagnostics = Vec::new();

    process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

    assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::UnsupportedActionTarget));
    assert_eq!(timeline.child_orders_iter().count(), 0);
}

#[test]
fn swap_action_requires_common_parent() {
    let mut timeline = Timeline::new();
    timeline.spawn_entity("a", "Rect");
    timeline.spawn_entity("b", "Rect");

    let action = Action {
        verb: "swap".to_string(),
        targets: vec!["a".to_string(), "b".to_string()],
        args: vec![],
        modifiers: vec![Modifier {
            name: None,
            value: crate::ast::Expr::Ident("500ms".to_string()),
        }],
        byte_span: None,
        target_index: vec![],
    };
    let mut diagnostics = Vec::new();

    process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

    assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::UnsupportedActionTarget));
    assert_eq!(timeline.child_orders_iter().count(), 0);
}

#[test]
fn swap_action_sets_child_order_keyframe() {
    let mut timeline = Timeline::new();

    // Set up container with children
    let row_id = timeline.world.spawn_with_track(
        "row",
        "Rect",
        AnimationTrack::placeholder("row".to_string()),
    );

    // Set up child tracks with layout size so they're admitted
    let child_a =
        timeline
            .world
            .spawn_with_track("a", "Rect", AnimationTrack::placeholder("a".to_string()));
    timeline.world.geometry_mut(child_a).layout_size = Some(PropertyTrack::new([15.0, 20.0]));
    timeline.world.link(row_id, child_a);

    let child_b =
        timeline
            .world
            .spawn_with_track("b", "Rect", AnimationTrack::placeholder("b".to_string()));
    timeline.world.geometry_mut(child_b).layout_size = Some(PropertyTrack::new([15.0, 40.0]));
    timeline.world.link(row_id, child_b);

    // Set up container metadata with layout children
    timeline.set_container_layout(
        "row",
        ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [8.0, 8.0],
            padding: [0.0, 0.0, 0.0, 0.0],
            align: "center".to_string(),
            vertical_align: "center".to_string(),
            cols: None,
            child_order: timeline.world.hierarchy(row_id).children.clone(),
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
        target_index: vec![],
    };
    let mut diagnostics = Vec::new();

    process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

    assert!(diagnostics.is_empty(), "unexpected diagnostics: {:?}", diagnostics);
    assert_eq!(timeline.child_orders_iter().count(), 1);

    let track = &timeline.child_order("row").unwrap().track;
    assert_eq!(track.keyframes.len(), 2);
    let (start_order, _) = track.keyframes.get(&0).unwrap();
    assert_eq!(start_order, &vec![child_a, child_b]);
    let (end_order, _) = track.keyframes.get(&500).unwrap();
    assert_eq!(end_order, &vec![child_b, child_a]);
}

#[test]
fn swap_action_detects_overlapping_swaps() {
    let mut timeline = Timeline::new();

    // Set up container with children
    let row_id = timeline.world.spawn_with_track(
        "row",
        "Rect",
        AnimationTrack::placeholder("row".to_string()),
    );

    for label in ["a", "b", "c"] {
        let child_id = timeline.world.spawn_with_track(
            label,
            "Rect",
            AnimationTrack::placeholder(label.to_string()),
        );
        timeline.world.geometry_mut(child_id).layout_size = Some(PropertyTrack::new([15.0, 20.0]));
        timeline.world.link(row_id, child_id);
    }

    timeline.set_container_layout(
        "row",
        ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [8.0, 8.0],
            padding: [0.0, 0.0, 0.0, 0.0],
            align: "center".to_string(),
            vertical_align: "center".to_string(),
            cols: None,
            child_order: timeline.world.hierarchy(row_id).children.clone(),
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
        target_index: vec![],
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
        target_index: vec![],
    };
    let mut diagnostics2 = Vec::new();
    process_action(&action2, 200.0, &mut timeline, &mut diagnostics2, None);

    assert!(diagnostics2.iter().any(|d| d.code == DiagnosticCode::ConflictingModifierKey));
    // Only the first swap's keyframes should exist (start + end)
    let track = &timeline.child_order("row").unwrap().track;
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
        target_index: vec![],
    };
    let mut timeline = Timeline::new();
    let mut diagnostics = Vec::new();

    process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

    assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::InvalidModifierValue));
    assert_eq!(timeline.child_orders_iter().count(), 0);
}

#[test]
fn reorder_action_requires_order_modifier() {
    let mut timeline = Timeline::new();
    timeline
        .world
        .spawn_with_track("row", "Rect", AnimationTrack::placeholder("row".to_string()));

    let action = Action {
        verb: "reorder".to_string(),
        targets: vec!["row".to_string()],
        args: vec![],
        modifiers: vec![Modifier {
            name: None,
            value: crate::ast::Expr::Ident("500ms".to_string()),
        }],
        byte_span: None,
        target_index: vec![],
    };
    let mut diagnostics = Vec::new();

    process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

    assert!(diagnostics.iter().any(|d| d.code == DiagnosticCode::InvalidModifierValue));
    assert_eq!(timeline.child_orders_iter().count(), 0);
}

#[test]
fn reorder_action_sets_child_order_keyframe() {
    let mut timeline = Timeline::new();

    // Set up container with children
    let row_id = timeline.world.spawn_with_track(
        "row",
        "Rect",
        AnimationTrack::placeholder("row".to_string()),
    );

    for label in ["a", "b", "c"] {
        let child_id = timeline.world.spawn_with_track(
            label,
            "Rect",
            AnimationTrack::placeholder(label.to_string()),
        );
        timeline.world.geometry_mut(child_id).layout_size = Some(PropertyTrack::new([15.0, 20.0]));
        timeline.world.link(row_id, child_id);
    }

    timeline.set_container_layout(
        "row",
        ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [8.0, 8.0],
            padding: [0.0, 0.0, 0.0, 0.0],
            align: "center".to_string(),
            vertical_align: "center".to_string(),
            cols: None,
            child_order: timeline.world.hierarchy(row_id).children.clone(),
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
        target_index: vec![],
    };
    let mut diagnostics = Vec::new();

    process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);

    assert!(diagnostics.is_empty(), "unexpected diagnostics: {:?}", diagnostics);
    assert_eq!(timeline.child_orders_iter().count(), 1);

    let track = &timeline.child_order("row").unwrap().track;
    assert_eq!(track.keyframes.len(), 2);
    let (start_order, _) = track.keyframes.get(&0).unwrap();
    assert_eq!(start_order, &timeline.world.hierarchy(row_id).children);
    let (end_order, _) = track.keyframes.get(&500).unwrap();
    let reversed: Vec<_> = start_order.iter().rev().copied().collect();
    assert_eq!(end_order, &reversed);
}

#[test]
fn reorder_action_detects_overlapping_reorders() {
    let mut timeline = Timeline::new();

    // Set up container with children
    let row_id = timeline.world.spawn_with_track(
        "row",
        "Rect",
        AnimationTrack::placeholder("row".to_string()),
    );

    for label in ["a", "b"] {
        let child_id = timeline.world.spawn_with_track(
            label,
            "Rect",
            AnimationTrack::placeholder(label.to_string()),
        );
        timeline.world.geometry_mut(child_id).layout_size = Some(PropertyTrack::new([15.0, 20.0]));
        timeline.world.link(row_id, child_id);
    }

    timeline.set_container_layout(
        "row",
        ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [8.0, 8.0],
            padding: [0.0, 0.0, 0.0, 0.0],
            align: "center".to_string(),
            vertical_align: "center".to_string(),
            cols: None,
            child_order: timeline.world.hierarchy(row_id).children.clone(),
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
        target_index: vec![],
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
        target_index: vec![],
    };
    let mut diagnostics2 = Vec::new();
    process_action(&action2, 200.0, &mut timeline, &mut diagnostics2, None);

    assert!(diagnostics2.iter().any(|d| d.code == DiagnosticCode::ConflictingModifierKey));
    // Only the first reorder's keyframes should exist (start + end)
    let track = &timeline.child_order("row").unwrap().track;
    assert_eq!(track.keyframes.len(), 2);
}

#[test]
fn swap_action_bounds_transition_to_duration() {
    // Regression test: consecutive swaps should not create a transition
    // that spans the gap between the end of swap N and the end of swap N+1.
    let mut timeline = Timeline::new();

    let row_id = timeline.world.spawn_with_track(
        "row",
        "Rect",
        AnimationTrack::placeholder("row".to_string()),
    );

    for label in ["a", "b", "c"] {
        let child_id = timeline.world.spawn_with_track(
            label,
            "Rect",
            AnimationTrack::placeholder(label.to_string()),
        );
        timeline.world.geometry_mut(child_id).layout_size = Some(PropertyTrack::new([15.0, 20.0]));
        timeline.world.link(row_id, child_id);
    }

    timeline.set_container_layout(
        "row",
        ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [8.0, 8.0],
            padding: [0.0, 0.0, 0.0, 0.0],
            align: "center".to_string(),
            vertical_align: "center".to_string(),
            cols: None,
            child_order: timeline.world.hierarchy(row_id).children.clone(),
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
        target_index: vec![],
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
        target_index: vec![],
    };
    process_action(&action2, 1000.0, &mut timeline, &mut Vec::new(), None);

    let track = &timeline.child_order("row").unwrap().track;
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

    let bars_id = timeline.world.spawn_with_track(
        "bars",
        "Rect",
        AnimationTrack::placeholder("bars".to_string()),
    );

    let sizes: [f32; 2] = [30.0, 40.0]; // half-size (60x80 rects)
    for label in ["bar1", "bar2", "bar3", "bar4", "bar5"] {
        let child_id = timeline.world.spawn_with_track(
            label,
            "Rect",
            AnimationTrack::placeholder(label.to_string()),
        );
        timeline.world.geometry_mut(child_id).layout_size = Some(PropertyTrack::new(sizes));
        timeline.world.link(bars_id, child_id);
    }

    timeline.set_container_layout(
        "bars",
        ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [8.0, 8.0],
            padding: [0.0, 0.0, 0.0, 0.0],
            align: "bottom".to_string(),
            vertical_align: "center".to_string(),
            cols: None,
            child_order: timeline.world.hierarchy(bars_id).children.clone(),
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
        target_index: vec![],
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
