use super::*;

#[test]
fn local_actor_array_suffix_requires_a_complete_numeric_segment() {
    assert!(is_array_suffix("__0"));
    assert!(is_array_suffix("__12.child"));
    assert!(!is_array_suffix("__a"));
    assert!(!is_array_suffix("__1tail"));
}

#[test]
fn always_only_plan_does_not_add_a_world_track_baseline() {
    let mut timeline = Timeline::new();
    let id = timeline.world_mut().spawn("dot", "Rect");
    let mut position = crate::timeline::PropertyTrack::new([0.0, 0.0]);
    position.add_keyframe(0, [0.0, 0.0], Easing::Linear);
    position.add_keyframe(4_000, [400.0, 0.0], Easing::Linear);
    timeline.world_mut().geometry_mut(id).position = Some(position);
    timeline.world_mut().set_first_seen(id, 0);

    let plan_body = animatix_syntax::ast::PlanBody::new(vec![
        Stmt::Always {
            body: vec![Stmt::Assignment {
                target: vec![TargetSegment::Static("dot".into())],
                property: "position".into(),
                value: animatix_syntax::ast::Expr::Tuple(vec![
                    animatix_syntax::ast::Expr::Num(100.0),
                    animatix_syntax::ast::Expr::Num(0.0),
                ]),
                modifiers: Vec::new(),
                easing: None,
                value_span: None,
                span: None,
            }],
            span: None,
        },
        // The reactive clip still needs a finite time extent to be mounted.
        // An empty boundary gives it one without writing a static World track.
        Stmt::Keyframe {
            cue_group: None,
            time: animatix_syntax::ast::Time::Seconds(1.0),
            body: Vec::new(),
            span: None,
        },
    ]);
    let env = timeline.build_eval_env(1_000);
    let plan = crate::timeline::plan_value::TimelinePlan::capture_plan_body(plan_body, &env);
    let mut diagnostics = Vec::new();
    timeline.mount_plan(&plan, 1_000.0, None, 1.0, None, &mut diagnostics);

    assert!(diagnostics.iter().all(|diagnostic| !diagnostic.is_error()), "{diagnostics:?}");
    assert_eq!(
        timeline.world.property_keyframe_times(id, ActorField::Position),
        vec![0, 4_000],
        "modifier-only clips do not add keyframes to the host's static track"
    );
    assert_eq!(
        timeline.world.read_property_value(id, ActorField::Position, 500),
        Some(PropertyValue::Vec2([50.0, 0.0]))
    );
    assert_eq!(
        timeline.world.read_property_value(id, ActorField::Position, 2_500),
        Some(PropertyValue::Vec2([250.0, 0.0]))
    );
}
