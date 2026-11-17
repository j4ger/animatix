use super::*;
use crate::easing::Easing;
use crate::timeline::world::PlanOwnership;
use crate::timeline::{ContainerMetadata, EntityId, LayoutType, PropertyTrack};
use std::collections::HashMap;

fn build(source: &str) -> Timeline {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "{errors:?}\n{source}");
    let report = Timeline::build_with_diagnostics(&ast.expect("AST"), &HashMap::new());
    assert!(
        report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "{:?}",
        report.diagnostics
    );
    report.output
}

#[test]
fn persistent_group_does_not_carry_actors_from_inline_plan_source() {
    let source = build(
        r#"
#0s
row: Row, gap: 8 {
  before: Rect, opacity: 1
  #{ local: Rect, opacity: 1 }
  after: Rect, opacity: 1
}
persist(row)
"#,
    );
    let row_id = source.world.get_id("row").expect("row actor");
    let private_children: Vec<_> = source
        .world
        .child_labels(row_id)
        .filter(|label| {
            source
                .world
                .get_id(label)
                .is_some_and(|id| source.world.plan_ownership(id).is_some())
        })
        .collect();
    assert_eq!(private_children.len(), 1, "source must contain one plan-owned row child");

    let bag = source.compute_carry_bag(0, true);
    let row = bag.entries.get("row").expect("persisted row is carried");
    assert_eq!(row.children.keys().map(String::as_str).collect::<Vec<_>>(), ["after", "before"]);
    assert!(private_children.iter().all(|label| !row.children.contains_key(*label)));
    assert!(
        !bag.entries
            .keys()
            .any(|label| private_children.iter().any(|private| *private == label))
    );
}

#[test]
fn carry_skips_plan_owned_subtrees_and_filters_order_references() {
    let mut source = Timeline::new();
    let row = source.world.spawn("row", "Row");
    let first = source.world.spawn("first", "Rect");
    let private_group = source.world.spawn("plan_mount.local_group", "Row");
    let private_rect = source.world.spawn("plan_mount.local_rect", "Rect");
    let explicit_child = source.world.spawn("caller_persisted", "Rect");
    let last = source.world.spawn("last", "Rect");

    source.world.link(row, first);
    source.world.link(row, private_group);
    source.world.link(private_group, private_rect);
    source.world.link(private_group, explicit_child);
    source.world.link(row, last);

    let owner = PlanOwnership::new(41);
    source.world_mut().view_mut(private_group).unwrap().set_plan_ownership(owner);
    source.world_mut().view_mut(private_rect).unwrap().set_plan_ownership(owner);

    source.set_persistence_flag("row", true);
    // Even a stale/direct flag on a private actor cannot make it a carry root.
    source.set_persistence_flag("plan_mount.local_group", true);
    // Persistence explicitly authored on a caller-owned actor below a private
    // parent survives as an independent root, but does not inherit the row.
    source.set_persistence_flag("caller_persisted", true);

    source.set_container_layout(
        "row",
        ContainerMetadata {
            layout_type: LayoutType::Row,
            gap: [0.0; 2],
            padding: [0.0; 4],
            align: "center".to_string(),
            vertical_align: "center".to_string(),
            cols: None,
            child_order: vec![first, private_group, last, EntityId(9999)],
        },
    );
    let mut order = PropertyTrack::new(vec![first, private_group, last]);
    order.add_keyframe(500, vec![last, private_group, first], Easing::Linear);
    assert!(source.set_child_order_track("row", order));

    let bag = source.compute_carry_bag(500, true);
    let row_entry = bag.entries.get("row").expect("caller row is carried");
    assert_eq!(
        row_entry.children.keys().map(String::as_str).collect::<Vec<_>>(),
        ["first", "last"]
    );
    assert_eq!(row_entry.container_layout.as_ref().unwrap().child_order, vec![first, last]);
    let child_order = row_entry.child_order.as_ref().expect("animated order survives");
    assert_eq!(child_order.default_order, ["first", "last"]);
    assert_eq!(child_order.keyframes[0].1, ["last", "first"]);

    assert!(bag.entries.contains_key("caller_persisted"));
    assert!(!bag.entries.contains_key("plan_mount.local_group"));
    assert!(!row_entry.children.contains_key("plan_mount.local_group"));
    assert!(!row_entry.children.contains_key("plan_mount.local_rect"));

    let mut destination = Timeline::new();
    destination.inject_carry_bag(&bag, &source, 0, [1280.0, 720.0], &mut Vec::new());
    assert!(destination.world.get_id("first").is_some());
    assert!(destination.world.get_id("last").is_some());
    assert!(destination.world.get_id("caller_persisted").is_some());
    assert!(destination.world.get_id("plan_mount.local_group").is_none());
    assert!(destination.world.get_id("plan_mount.local_rect").is_none());

    let destination_row = destination.world.get_id("row").unwrap();
    assert_eq!(
        destination
            .container_layout("row")
            .unwrap()
            .child_order
            .iter()
            .filter_map(|id| destination.world.label(*id))
            .collect::<Vec<_>>(),
        ["first", "last"]
    );
    assert_eq!(
        destination.get_child_order_ids("row", 500),
        [
            destination.world.get_id("last").unwrap(),
            destination.world.get_id("first").unwrap(),
        ]
    );
    assert!(
        destination
            .world
            .hierarchy(destination.world.get_id("caller_persisted").unwrap())
            .is_root()
    );
    assert_eq!(destination.world.label(destination_row), Some("row"));
}
