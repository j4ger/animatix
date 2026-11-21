use super::{ActorPoint, SceneAnchor, Value};
use crate::timeline::{SceneDimensions, Timeline};

#[test]
fn bound_static_point_is_folded_and_rebuilt_after_an_edit() {
    let source = r#"
#0s
  group: Group, at: (120, 80), transform: (2, 1) {
    marker: Rect, at: (10, 0), size: (20, 20)
  }
  edge: Line, from: group.marker.right, to: (200, 80)
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "{errors:?}");
    let report = Timeline::build_with_diagnostics(&ast.unwrap(), &Default::default());
    assert!(
        !report.diagnostics.iter().any(|diag| diag.is_error()),
        "{:?}",
        report.diagnostics
    );
    let mut timeline = report.output;
    let id = timeline.world.get_id("marker").unwrap();
    let key = (id, ActorPoint::Bounds(SceneAnchor::Right));
    let folded = timeline.eval_caches.fixed_actor_points.borrow()[&key].unwrap().position;
    let overrides = Default::default();
    let frame = Timeline::point_query_frame(100, SceneDimensions::default(), &overrides);
    assert_eq!(timeline.actor_point(id, key.1, &frame, true), Some(folded));
    assert_eq!(folded, timeline.sample_actor_point(id, key.1, &frame, true));

    let overrides =
        [("marker".to_owned(), [("at".to_owned(), Value::Vec2([30.0, 0.0]))].into())].into();
    let override_frame = Timeline::point_query_frame(100, SceneDimensions::default(), &overrides);
    let shifted = timeline.sample_actor_point(id, key.1, &override_frame, true);
    assert_ne!(folded, shifted);
    assert_eq!(timeline.actor_point(id, key.1, &override_frame, true), Some(shifted));

    timeline.world_mut().geometry.get_mut(id).position =
        Some(crate::timeline::PropertyTrack::new([30.0, 0.0]));
    assert!(timeline.eval_caches.fixed_actor_points.borrow().is_empty());
    assert_eq!(timeline.actor_point(id, key.1, &frame, true), Some(shifted));
}

#[test]
fn viewport_bound_ancestor_prevents_folding() {
    let source = r#"
#0s
  group: Group, anchor: scene.center {
    marker: Rect, at: (10, 0), size: (20, 20)
  }
  edge: Line, from: group.marker.origin, to: (200, 80)
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "{errors:?}");
    let report = Timeline::build_with_diagnostics(&ast.unwrap(), &Default::default());
    assert!(
        !report.diagnostics.iter().any(|diag| diag.is_error()),
        "{:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let id = timeline.world.get_id("marker").unwrap();
    let point = ActorPoint::Origin;
    assert!(timeline.eval_caches.fixed_actor_points.borrow()[&(id, point)].is_none());
    let overrides = Default::default();
    let sample = |width| {
        let frame =
            Timeline::point_query_frame(0, SceneDimensions { width, height: 100 }, &overrides);
        timeline.actor_point(id, point, &frame, true).unwrap()
    };
    assert_eq!(sample(200) - sample(100), kurbo::Vec2::new(50.0, 0.0));
}

#[test]
fn polygon_centroid_is_area_weighted_and_independent_of_winding() {
    let triangle = [[0.0, 0.0], [90.0, 0.0], [0.0, 60.0]];
    let reverse = [triangle[0], triangle[2], triangle[1]];
    let centroid = kurbo::Point::new(30.0, 20.0);
    assert_eq!(super::polygon_centroid(&triangle), Some(centroid));
    assert_eq!(super::polygon_centroid(&reverse), Some(centroid));
    assert_eq!(super::polygon_centroid(&triangle[..2]), None);
}

#[test]
fn actor_point_in_a_function_follows_nested_transforms_and_frame_writes() {
    let source = r#"
fn edge(target: Actor) -> Vec2 { return target.right }
#0s
  cluster: Group, at: (120, 80), transform: (2, 1) {
    child: Rect, at: (10, 0), size: (20, 20)
  }
  pointer: Rect, size: (4, 4)
  always {
    child.at = (20, 0)
    pointer.at = edge(cluster.child)
  }
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "{errors:?}");
    let report = Timeline::build_with_diagnostics(&ast.unwrap(), &Default::default());
    assert!(
        !report.diagnostics.iter().any(|diag| diag.is_error()),
        "{:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let frame = timeline.evaluate_program_with_debug(
        0.0,
        SceneDimensions::default(),
        Default::default(),
        &mut None,
    );
    assert!(
        timeline.runtime_diagnostics().is_empty(),
        "{:?}",
        timeline.runtime_diagnostics()
    );
    let child = frame.precise_bounds["child"];
    let pointer = frame.precise_bounds["pointer"].center();
    assert_eq!(pointer, kurbo::Point::new(child.x1, child.center().y));
}

#[test]
fn length_member_matches_the_legacy_callable() {
    use crate::timeline::{Environment, Value, evaluate_expr};
    let mut env = Environment::new();
    env.set("items", Value::List(vec![Value::Num(4.0), Value::Num(5.0)].into()));
    env.set("text", Value::Str("é".into()));
    for receiver in ["items", "text"] {
        let field = crate::ast::Expr::Path(vec![receiver.into(), "length".into()]);
        let method = crate::ast::Expr::Call(
            Box::new(crate::ast::Expr::Field(
                Box::new(crate::ast::Expr::Ident(receiver.into())),
                "length".into(),
            )),
            vec![],
        );
        assert_eq!(evaluate_expr(&field, &env).unwrap(), Value::Num(2.0));
        assert_eq!(evaluate_expr(&field, &env).unwrap(), evaluate_expr(&method, &env).unwrap());
    }
}
