use super::*;

#[test]
fn spawn_returns_stable_ids_and_keeps_stores_aligned() {
    let mut world = World::new();
    let a = world.spawn("a", "Rect");
    let b = world.spawn("b", "Rect");
    assert_eq!(a.index(), 0);
    assert_eq!(b.index(), 1);
    assert_eq!(world.len(), 2);
    assert_eq!(world.hierarchy.len(), 2);
    assert_eq!(world.caps.len(), 2, "caps stays slot-aligned");
    assert_eq!(world.behavior.len(), 2, "behavior stays slot-aligned");
    assert_eq!(world.meta.len(), 2, "metadata stays slot-aligned");
    assert_eq!(world.payload.len(), 2, "payload stays slot-aligned");
    assert_eq!(world.container_layout.len(), 2);
    assert_eq!(world.coordinate_frame.len(), 2, "every store stays slot-aligned");
    assert_eq!(world.label(a), Some("a"));
    assert_eq!(world.get_id("b"), Some(b));

    assert_eq!(world.spawn("a", "Ellipse"), a);
    assert_eq!(world.len(), 2);
    assert_eq!(world.actor_type(a), "Rect");
}

#[test]
fn spawn_replacing_keeps_the_id_and_resets_hierarchy() {
    let mut world = World::new();
    let parent = world.spawn("parent", "Rect");
    let id = world.spawn("box", "Rect");
    world.link(parent, id);
    assert_eq!(world.hierarchy(id).parent, Some(parent));

    world.spawn_replacing("box", "Ellipse");
    assert_eq!(world.get_id("box"), Some(id));
    assert_eq!(world.hierarchy(id), &SceneHierarchyComponent::default());
    assert_eq!(world.actor_type(id), "Ellipse");
    assert!(!world.hierarchy(parent).children.contains(&id));
    assert!(world.is_root(id));
}

#[test]
fn spawn_replacing_clears_the_coordinate_frame() {
    let mut world = World::new();
    let id = world.spawn("g", "Graph");
    world.coordinate_frame.insert(
        id,
        CoordinateFrameComponent {
            x_domain: [0.0, 4.0],
            ..CoordinateFrameComponent::default()
        },
    );
    assert!(world.coordinate_frame(id).is_some());

    let replacement = world.spawn_replacing("g", "Graph");
    assert_eq!(replacement, id, "replacement keeps the entity id");
    assert!(
        world.coordinate_frame(id).is_none(),
        "spawn_replacing must not inherit the previous scene's frame"
    );
}

#[test]
fn views_resolve_by_label_and_id() {
    let world = {
        let mut world = World::new();
        world.spawn("a", "Rect");
        world.spawn("b", "Text");
        world
    };
    assert_eq!(world.entity("b").unwrap().id().index(), 1);
    assert_eq!(world.view(EntityId(0)).unwrap().label(), "a");
    assert!(world.entity("missing").is_none());
    assert!(world.view(EntityId(9)).is_none());
    assert_eq!(world.views().count(), 2);
}

#[test]
fn clear_drops_entities_and_components() {
    let mut world = World::new();
    world.spawn("a", "Rect");
    world.clear();
    assert!(world.is_empty());
    assert!(world.hierarchy.is_empty());
    assert!(world.caps.is_empty());
    assert!(world.behavior.is_empty());
    assert!(world.container_layout.is_empty());
    assert!(world.coordinate_frame.is_empty());
}

#[test]
fn declaration_span_is_a_component_and_resets_on_replacement() {
    let mut world = World::new();
    let id = world.spawn("box", "Rect");
    world.set_decl_span(id, animatix_syntax::ast::Span::new(1, 1, 1, 10));
    assert_eq!(world.decl_span(id).map(|s| s.start_line), Some(1));

    world.spawn_replacing("box", "Rect");
    assert!(world.decl_span(id).is_none(), "replacement starts from a clean span");
}

#[test]
fn entity_meta_defaults_and_resets_on_replacement() {
    let mut world = World::new();
    let id = world.spawn("box", "Rect");
    assert!(world.meta(id).visible);
    assert!(!world.meta(id).locked);
    assert!(world.meta(id).camera_follow);
    assert_eq!(world.meta(id).parallax, 1.0);

    world.meta_mut(id).visible = false;
    world.meta_mut(id).locked = true;
    world.meta_mut(id).hidden_by_default = true;
    world.set_first_seen(id, 100);
    world.spawn_replacing("box", "Rect");
    let meta = world.meta(id);
    assert!(meta.visible, "replacement starts visible");
    assert!(!meta.locked, "replacement starts unlocked");
    assert!(!meta.hidden_by_default, "replacement starts revealed");
    assert_eq!(world.first_seen(id), u64::MAX, "replacement starts unseen");
}
