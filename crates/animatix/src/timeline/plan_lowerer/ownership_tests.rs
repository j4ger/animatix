use super::*;
use std::path::Path;

use animatix_syntax::module::ModuleGraph;

fn build(source: &str) -> Timeline {
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "{errors:?}\n{source}");
    let report = Timeline::build_with_diagnostics(&ast.expect("AST"), &HashMap::new());
    assert!(
        report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "{:?}",
        report.diagnostics
    );
    assert!(
        report.output.build_plan_ownership.is_empty(),
        "mount ownership scope must restore"
    );
    report.output
}

fn build_expanded(source: &str) -> Timeline {
    let mut modules = ModuleGraph::new();
    let program = modules
        .load_program_with_source(Path::new("virtual:///plan_ownership.amx"), Some(source))
        .expect("load component source");
    let expanded = program.expand_components().expect("component expansion");

    let report = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        crate::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(
        report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "{:?}",
        report.diagnostics
    );
    report.output
}

#[test]
fn private_plan_labels_do_not_alias_legal_actor_labels_before_or_after_mount() {
    for source in [
        r#"
plan1_local: Rect, at: (-100, 0)
fn clip() { #{ local: Rect, at: (100, 0) } }
#0s
clip()
"#,
        r#"
fn clip() { #{ local: Rect, at: (100, 0) } }
#0s
clip()
plan1_local: Rect, at: (-100, 0)
"#,
    ] {
        let timeline = build(source);
        let authored = timeline.world.get_id("plan1_local").expect("authored actor");
        let owned = timeline
            .world
            .views()
            .filter(|actor| actor.plan_ownership().is_some())
            .collect::<Vec<_>>();
        assert_eq!(owned.len(), 1);
        assert_ne!(owned[0].id(), authored);
        assert!(owned[0].label().starts_with("@animatix:plan:"));
        assert_eq!(timeline.world.plan_ownership(authored), None);
    }
}

#[test]
fn plan_label_allocator_avoids_existing_world_labels_too() {
    let mut timeline = Timeline::new();
    let candidate = timeline.fresh_plan_actor_label(72, "local", None);
    timeline.world_mut().spawn(&candidate, "Rect");

    let next = timeline.fresh_plan_actor_label(72, "local", None);
    assert_ne!(candidate, next);
    assert!(next.starts_with("@animatix:plan:72:"));

    let parent = "frame";
    timeline.world_mut().spawn(parent, "Group");
    let qualified_candidate = format!("{parent}.@animatix:plan:73:local");
    timeline.world_mut().spawn(&qualified_candidate, "Rect");

    let next = timeline.fresh_plan_actor_label(73, "local", Some(parent));
    assert_ne!(qualified_candidate, format!("{parent}.{next}"));
    assert!(next.starts_with("@animatix:plan:73:"));
}

#[test]
fn nested_plan_mounts_own_each_spawned_entity_by_entity_id() {
    let timeline = build(
        r#"
fn leaf() { #{ inner: Rect, opacity: 1 } }
fn bundle() { #{ frame: Group { leaf() } } }
#0s
bundle()
bundle()
"#,
    );

    let owned = timeline
        .world
        .views()
        .filter_map(|actor| actor.plan_ownership().map(|owner| (actor, owner)))
        .collect::<Vec<_>>();
    assert_eq!(owned.len(), 4);
    assert_eq!(timeline.world.tracks.len(), 4);
    assert!(owned.iter().all(|(actor, _)| actor.label().contains("@animatix:plan:")));
    let mount_ids = owned
        .iter()
        .map(|(_, owner)| owner.mount_id())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(mount_ids.len(), 4, "outer and nested mounts have distinct owners");
    for root in timeline.world.root_entities() {
        assert_eq!(timeline.world.child_labels(*root).count(), 1);
        let child = timeline.world.hierarchy(*root).children[0];
        assert_ne!(timeline.world.plan_ownership(*root), timeline.world.plan_ownership(child));
    }
}

#[test]
fn plan_ownership_covers_component_arrays_and_anonymous_descendants() {
    let timeline = build_expanded(
        r#"
component Card(title: Str) {
  body: Group {
    title_text: Text, text: title
    for label, i in {"first", "second"} {
      dots[i]: Ellipse, opacity: 1
    }
    Rect
  }
}
fn make_cards() {
  return #{
    for title, i in {"one", "two"} {
      cards[i]: Card, title: title
    }
  }
}
#0s
make_cards()
make_cards()
caller: Rect
"#,
    );

    let private = timeline
        .world
        .views()
        .filter_map(|actor| actor.plan_ownership().map(|owner| (actor.label(), owner)))
        .collect::<Vec<_>>();
    assert!(private.len() >= 20, "component descendants missing: {private:?}");
    assert!(private.iter().all(|(label, _)| label.contains("@animatix:plan:")));
    for expected in ["title_text", "dots__0", "dots__1", "__anon_"] {
        assert!(
            private.iter().any(|(label, _)| label.contains(expected)),
            "plan ownership missed {expected}: {private:?}"
        );
    }

    let mount_ids = private
        .iter()
        .map(|(_, owner)| owner.mount_id())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(mount_ids.len(), 2, "repeated mounts need distinct ownership IDs");
    let caller = timeline.world.get_id("caller").expect("caller actor");
    assert_eq!(timeline.world.plan_ownership(caller), None);
}

#[test]
fn ownership_mutation_uses_timeline_cache_invalidation_and_survives_clone() {
    let mut timeline = build("#0s\nr: Rect");
    let id = timeline.world.get_id("r").expect("actor");
    let _ = timeline.evaluate(0.0, crate::timeline::SceneDimensions::default());
    assert!(timeline.eval_caches.frame_cache.borrow().is_some());

    timeline
        .entity_by_id_mut(id)
        .expect("mutable entity")
        .set_plan_ownership(crate::timeline::world::PlanOwnership::new(91));
    assert!(timeline.eval_caches.frame_cache.borrow().is_none());

    let snapshot = timeline.clone();
    assert_eq!(snapshot.world.plan_ownership(id).unwrap().mount_id(), 91);
}
