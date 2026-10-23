//! Animated child-order (`swap` / `reorder`) storage tests.

use super::*;

use crate::ast::{Action, Modifier};
use crate::timeline::actions::process_action;

/// The reorder action must store `EntityId`s in the component, not labels.
#[test]
fn reorder_action_stores_ids() {
    let mut timeline = Timeline::new();
    let row_id = timeline.spawn_entity("row", "Row");
    let a = timeline.spawn_entity("a", "Rect");
    let b = timeline.spawn_entity("b", "Rect");
    timeline.world.link(row_id, a);
    timeline.world.link(row_id, b);
    assert!(timeline.set_container_layout(
        "row",
        ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [0.0, 0.0],
            padding: [0.0; 4],
            align: "center".to_string(),
            vertical_align: "center".to_string(),
            cols: None,
            child_order: vec![a, b],
        }
    ));

    let action = Action {
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
    process_action(&action, 0.0, &mut timeline, &mut diagnostics, None);
    assert!(diagnostics.is_empty(), "unexpected diagnostics: {:?}", diagnostics);

    let track = &timeline.child_order("row").expect("child order component").track;
    assert_eq!(track.keyframes.len(), 2);
    assert_eq!(track.keyframes.get(&0).unwrap().0, vec![a, b]);
    assert_eq!(track.keyframes.get(&500).unwrap().0, vec![b, a]);

    // The label view still follows the stored ids.
    assert_eq!(timeline.get_child_order("row", 0), vec!["a", "b"]);
    assert_eq!(timeline.get_child_order("row", 500), vec!["b", "a"]);
}
