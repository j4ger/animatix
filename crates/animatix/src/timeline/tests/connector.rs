use super::support::build;
use crate::primitives::connector::{build_connector_path, route_connector};
use crate::timeline::{ConnectorRouting, SceneDimensions};

#[test]
fn test_connector_routing_algorithms() {
    let from = [100.0, 100.0];
    let norm_from = [1.0, 0.0]; // Right
    let to = [300.0, 200.0];
    let norm_to = [-1.0, 0.0]; // Left

    // Straight
    let straight_pts =
        route_connector(from, norm_from, to, norm_to, ConnectorRouting::Straight, 20.0);
    assert_eq!(straight_pts.len(), 2);
    assert_eq!(straight_pts[0], from);
    assert_eq!(straight_pts[1], to);

    // L-Bend
    let lbend_pts = route_connector(from, norm_from, to, norm_to, ConnectorRouting::LBend, 20.0);
    assert_eq!(lbend_pts.len(), 3);
    assert_eq!(lbend_pts[0], from);
    assert_eq!(lbend_pts[1], [to[0], from[1]]);
    assert_eq!(lbend_pts[2], to);

    // Elbow
    let elbow_pts = route_connector(from, norm_from, to, norm_to, ConnectorRouting::Elbow, 20.0);
    assert_eq!(elbow_pts.len(), 4);
    assert_eq!(elbow_pts[0], from);
    assert_eq!(elbow_pts[1], [200.0, from[1]]); // Mid-x
    assert_eq!(elbow_pts[2], [200.0, to[1]]); // Mid-x
    assert_eq!(elbow_pts[3], to);
}

#[test]
fn test_connector_fillet_and_arrow() {
    let waypoints = vec![
        [100.0, 100.0],
        [200.0, 100.0],
        [200.0, 200.0],
        [300.0, 200.0],
    ];

    let (shaft, arrow) = build_connector_path(&waypoints, 8.0, true, 10.0);
    assert!(!shaft.is_empty(), "Shaft path should be populated");
    assert!(arrow.is_some(), "Arrowhead path should be generated when arrow: true");

    let (shaft_no_arrow, no_arrow) = build_connector_path(&waypoints, 8.0, false, 10.0);
    assert!(!shaft_no_arrow.is_empty());
    assert!(no_arrow.is_none(), "No arrowhead when arrow: false");
}

#[test]
fn test_connector_build_and_evaluate() {
    let source = r#"
#0s
box1: Rect, size: (100, 60), at: (200, 300)
box2: Rect, size: (100, 60), at: (500, 300)
wire: Connector, from: (box1, right), to: (box2, left), routing: "elbow", arrow: true, corner_radius: 8
"#;
    let report = build(source);
    if !report.output.world.tracks.contains_key("wire") {
        panic!("Connector wire track missing! Diagnostics: {:#?}", report.diagnostics);
    }
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = report.output.evaluate(0.0, dims);
    assert!(
        report.output.world.tracks.contains_key("wire"),
        "Connector wire track should be present"
    );
}

#[test]
fn nested_target_connector_stays_on_world_edges_at_multiple_depths() {
    // Identity wrappers must not change meaning; translations must apply once.
    // No font/GPU dependency: evaluate collects precise vector command bounds.
    for depth in [1, 2, 8, 16] {
        let mut source = String::from(
            "config { resolution:(1280,720) }
",
        );
        for level in 0..depth {
            source.push_str(&format!(
                "g{level}: Group, at:(10,5), opacity:1 {{
"
            ));
        }
        source.push_str(
            "leaf_a: Rect, at:(200,200), size:(40,40), opacity:1
             leaf_b: Rect, at:(400,200), size:(40,40), opacity:1
             wire: Connector, from:(leaf_a,right), to:(leaf_b,left), routing:\"straight\", arrow:false, stroke_width:2, opacity:1
",
        );
        source.push_str(
            &"}
"
            .repeat(depth),
        );
        let report = build(&source);
        assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
        let timeline = report.output;
        let dims = SceneDimensions {
            width: 1280,
            height: 720,
        };
        for _ in 0..2 {
            let _scene = timeline.evaluate(0.0, dims);
            let a = timeline.precise_bounds_for("leaf_a").unwrap();
            let b = timeline.precise_bounds_for("leaf_b").unwrap();
            let wire = timeline.precise_bounds_for("wire").unwrap();
            assert!(
                (wire.x0 - a.x1).abs() < 2.0 && (wire.x1 - b.x0).abs() < 2.0,
                "depth {depth}: connector endpoints transformed twice: {a:?} {b:?} {wire:?}"
            );
            assert!(
                (wire.center().y - a.center().y).abs() < 2.0,
                "depth {depth}: connector left target vertically: {a:?} {wire:?}"
            );
        }
    }
}

#[test]
fn connector_target_resolution_converts_world_points_and_directions_to_local() {
    use crate::primitives::connector::resolve_connector_endpoint_local;
    use crate::timeline::callout_geometry::TargetResolver;
    use crate::timeline::{ConnectorEndpoint, SceneAnchor};
    struct Bounds;
    impl TargetResolver for Bounds {
        fn target_bounds(
            &self,
            _name: &str,
            _time: u64,
            _dims: SceneDimensions,
        ) -> Option<([f32; 2], [f32; 2])> {
            Some(([300.0, 200.0], [20.0, 20.0]))
        }
    }
    let dims = SceneDimensions {
        width: 1280,
        height: 720,
    };
    let target = ConnectorEndpoint::Target {
        actor: "target".into(),
        anchor: Some(SceneAnchor::Right),
    };
    let transforms = [
        kurbo::Affine::IDENTITY,
        kurbo::Affine::translate((80.0, 40.0)),
        kurbo::Affine::new([2.0, 0.0, 0.0, 0.5, 80.0, 40.0]),
        kurbo::Affine::rotate(0.7),
        kurbo::Affine::new([1.0, 0.3, 0.6, 1.0, 80.0, 40.0]),
    ];
    for transform in transforms {
        let (point, normal) =
            resolve_connector_endpoint_local(&target, Some(&Bounds), 0, dims, None, transform)
                .unwrap();
        let world = transform * kurbo::Point::new(point[0] as f64, point[1] as f64);
        assert!(
            (world.x - 320.0).abs() < 0.001 && (world.y - 200.0).abs() < 0.001,
            "target must land on world edge exactly once: {transform:?} {world:?}"
        );
        let [a, b, c, d, _, _] = transform.as_coeffs();
        let world_normal = [
            a * normal[0] as f64 + c * normal[1] as f64,
            b * normal[0] as f64 + d * normal[1] as f64,
        ];
        assert!(world_normal[0] > 0.0 && world_normal[1].abs() < 0.0001);
        let numeric = resolve_connector_endpoint_local(
            &ConnectorEndpoint::Point([5.0, 7.0]),
            Some(&Bounds),
            0,
            dims,
            None,
            transform,
        )
        .unwrap();
        assert_eq!(numeric.0, [5.0, 7.0], "numeric endpoints stay local");
    }
    let auto = ConnectorEndpoint::Target {
        actor: "target".into(),
        anchor: None,
    };
    let transform = kurbo::Affine::translate((400.0, 0.0));
    let (point, _) = resolve_connector_endpoint_local(
        &auto,
        Some(&Bounds),
        0,
        dims,
        Some([0.0, 200.0]),
        transform,
    )
    .unwrap();
    assert_eq!(point, [-80.0, 200.0], "auto anchor uses other endpoint in world space");
    assert!(
        resolve_connector_endpoint_local(&target, None, 0, dims, None, kurbo::Affine::IDENTITY,)
            .is_none(),
        "missing targets must not become a fabricated origin endpoint"
    );
    let collapsed = kurbo::Affine::scale(0.0);
    assert!(
        resolve_connector_endpoint_local(&target, Some(&Bounds), 0, dims, None, collapsed)
            .is_none()
    );
    assert!(
        resolve_connector_endpoint_local(
            &ConnectorEndpoint::Point([5.0, 7.0]),
            Some(&Bounds),
            0,
            dims,
            None,
            collapsed
        )
        .is_some()
    );
}

#[test]
fn mixed_connector_draw_commands_attach_target_and_keep_numeric_endpoint_local() {
    use crate::primitives::RenderCommand;
    use crate::timeline::DebugRenderOptions;
    for extra in ["", ", rotation: 30", ", transform: (2,0,0,0.5,0,0)"] {
        for reverse in [false, true] {
            let endpoints = if reverse {
                "from: (0,200), to: target"
            } else {
                "from: target, to: (0,200)"
            };
            let report = build(&format!(
                r#"
target: Rect, at: (300,200), size: (40,40), opacity: 1
holder: Group, at: (800,0), opacity: 1{extra} {{
    wire: Connector, {endpoints}, routing: "straight", arrow: false, stroke_width: 2, opacity: 1
}}
"#
            ));
            assert!(
                report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
                "{:?}",
                report.diagnostics
            );
            let timeline = report.output;
            for _ in 0..2 {
                let program = timeline.evaluate_program_with_debug(
                    0.0,
                    SceneDimensions {
                        width: 1280,
                        height: 720,
                    },
                    DebugRenderOptions {
                        compute_hit_regions: true,
                        ..Default::default()
                    },
                    &mut None,
                );
                let item = program
                    .items
                    .iter()
                    .find(|item| item.label == "wire")
                    .expect("connector draw item");
                let paths = item
                    .commands
                    .iter()
                    .find_map(|command| {
                        if let RenderCommand::Paths { paths } = command {
                            Some(paths)
                        } else {
                            None
                        }
                    })
                    .expect("connector path command");
                assert_eq!(paths.len(), 1, "straight shaft without arrow");
                let elements = paths[0].path.elements();
                let kurbo::PathEl::MoveTo(start) = elements[0] else {
                    panic!("shaft must start with MoveTo")
                };
                let kurbo::PathEl::LineTo(end) = elements[elements.len() - 1] else {
                    panic!("straight shaft must end with LineTo")
                };
                let (target_world, numeric_world) = if reverse {
                    (item.transform * end, item.transform * start)
                } else {
                    (item.transform * start, item.transform * end)
                };
                let expected_numeric = item.transform * kurbo::Point::new(0.0, 200.0);
                let target = program.precise_bounds["target"];
                assert!(
                    expected_numeric.x - target.center().x
                        > (expected_numeric.y - target.center().y).abs(),
                    "fixture must select target's right edge: {expected_numeric:?} {target:?}"
                );
                assert!(
                    (target_world.x - target.x1).abs() < 0.001
                        && (target_world.y - target.center().y).abs() < 0.001,
                    "target anchor must be world-space, not translated twice: {target_world:?} {target:?}"
                );
                assert!(
                    (numeric_world.x - expected_numeric.x).abs() < 0.001
                        && (numeric_world.y - expected_numeric.y).abs() < 0.001,
                    "numeric endpoint must retain local transform: {numeric_world:?} {expected_numeric:?}"
                );
                assert!(program.precise_bounds.contains_key("wire"));
                assert!(timeline.hit_regions().iter().any(|(name, _)| name == "wire"));
            }
        }
    }
}

#[test]
fn singular_target_connector_emits_no_draw_item_precise_bounds_or_hit_region() {
    use crate::timeline::DebugRenderOptions;
    for scale in [0, 1] {
        let report = build(&format!(
            r#"
a: Rect, at: (200,200), size: (40,40), opacity: 1
b: Rect, at: (400,200), size: (40,40), opacity: 1
holder: Group, at: (10,5), scale: {scale}, opacity: 1 {{
    wire: Connector, from: (a,right), to: (b,left), routing: "straight", arrow: false, opacity: 1
}}
"#
        ));
        assert!(
            report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
            "{:?}",
            report.diagnostics
        );
        let timeline = report.output;
        for _ in 0..2 {
            let program = timeline.evaluate_program_with_debug(
                0.0,
                SceneDimensions {
                    width: 1280,
                    height: 720,
                },
                DebugRenderOptions {
                    compute_hit_regions: true,
                    ..Default::default()
                },
                &mut None,
            );
            let expected = scale != 0;
            assert_eq!(
                program.items.iter().any(|item| item.label == "wire"),
                expected,
                "draw item at scale={scale}"
            );
            assert_eq!(
                program.precise_bounds.contains_key("wire"),
                expected,
                "precise bounds at scale={scale}"
            );
            assert_eq!(
                timeline.hit_regions().iter().any(|(name, _)| name == "wire"),
                expected,
                "hit region at scale={scale}"
            );
        }
    }
}

#[test]
fn target_connector_clears_bounds_and_hits_across_singular_frames_and_reverse_seek() {
    use crate::timeline::DebugRenderOptions;
    let report = build(
        r#"
a: Rect, at: (200,200), size: (40,40), opacity: 1
b: Rect, at: (400,200), size: (40,40), opacity: 1
holder: Group, at: (10,5), scale: 1, opacity: 1 {
    wire: Connector, from: (a,right), to: (b,left), routing: "straight", arrow: false, opacity: 1
}
#1s
holder.scale = 0 [0s]
#2s
holder.scale = 1 [0s]
#3s
"#,
    );
    assert!(
        report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "{:?}",
        report.diagnostics
    );
    let timeline = report.output;
    // Forward, reverse, and repeated exact-frame cache hits on the SAME timeline.
    for time in [0.5, 1.5, 2.5, 2.5, 1.5, 1.5, 0.5, 2.5, 1.5, 0.5] {
        let program = timeline.evaluate_program_with_debug(
            time,
            SceneDimensions {
                width: 1280,
                height: 720,
            },
            DebugRenderOptions {
                compute_hit_regions: true,
                ..Default::default()
            },
            &mut None,
        );
        let visible = !(1.0..2.0).contains(&time);
        assert_eq!(
            program.items.iter().any(|item| item.label == "wire"),
            visible,
            "draw at t={time}"
        );
        assert_eq!(
            program.precise_bounds.contains_key("wire"),
            visible,
            "program bounds at t={time}"
        );
        assert_eq!(
            timeline.precise_bounds_for("wire").is_some(),
            visible,
            "cached bounds at t={time}"
        );
        assert_eq!(
            timeline.hit_regions().iter().any(|(name, _)| name == "wire"),
            visible,
            "hits at t={time}"
        );
        if visible {
            let wire = program.precise_bounds["wire"];
            let a = program.precise_bounds["a"];
            let b = program.precise_bounds["b"];
            assert!(
                (wire.x0 - a.x1).abs() < 2.0 && (wire.x1 - b.x0).abs() < 2.0,
                "restored connector must recover endpoints at t={time}: {wire:?} {a:?} {b:?}"
            );
        }
    }
}

#[test]
fn connector_local_resolution_queries_target_once_and_handles_absent_actor() {
    use crate::primitives::connector::resolve_connector_endpoint_local;
    use crate::timeline::ConnectorEndpoint;
    use crate::timeline::callout_geometry::TargetResolver;
    use std::cell::Cell;
    struct Bounds {
        calls: Cell<usize>,
        present: bool,
    }
    impl TargetResolver for Bounds {
        fn target_bounds(
            &self,
            _name: &str,
            _time: u64,
            _dims: SceneDimensions,
        ) -> Option<([f32; 2], [f32; 2])> {
            self.calls.set(self.calls.get() + 1);
            self.present.then_some(([300.0, 200.0], [20.0, 20.0]))
        }
    }
    for present in [false, true] {
        let resolver = Bounds {
            calls: Cell::new(0),
            present,
        };
        let result = resolve_connector_endpoint_local(
            &ConnectorEndpoint::Target {
                actor: "target".into(),
                anchor: None,
            },
            Some(&resolver),
            0,
            SceneDimensions {
                width: 1280,
                height: 720,
            },
            Some([400.0, 200.0]),
            kurbo::Affine::IDENTITY,
        );
        assert_eq!(result.is_some(), present);
        assert_eq!(resolver.calls.get(), 1);
    }
}

#[test]
fn delightful_kinetic_connectors_reference_existing_actor_anchors() {
    use std::path::Path;

    use animatix_syntax::module::ModuleGraph;

    use crate::timeline::{ConnectorEndpoint, SceneAnchor, TrackAccessor};

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/animation/40_delightful_kinetic.amx");
    let mut modules = ModuleGraph::new();
    let program = modules.load_program(&path).expect("load kinetic example");
    let expanded = program.expand_components().expect("expand kinetic example");
    let report = crate::timeline::Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        crate::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let timeline = report.output;
    for (wire, from, to) in [("conn1", "client", "auth"), ("conn2", "auth", "database")] {
        let id = timeline.world.get_id(wire).expect("connector actor");
        let geometry = timeline.world.geometry(id);
        let fallback = ConnectorEndpoint::Point([0.0, 0.0]);
        let from_endpoint = geometry.connector_from.get(2500, fallback.clone());
        let to_endpoint = geometry.connector_to.get(2500, fallback);
        for (endpoint, target, anchor) in [
            (from_endpoint, from, SceneAnchor::Right),
            (to_endpoint, to, SceneAnchor::Left),
        ] {
            assert!(
                matches!(endpoint, ConnectorEndpoint::Target { actor, anchor: Some(actual) }
                    if actor == target && actual == anchor),
                "{wire}: wrong target anchor"
            );
            assert!(timeline.world.get_id(target).is_some(), "missing target {target}");
        }
    }
    for time in [2.5, 5.0, 2.5] {
        let program = timeline.evaluate_program_with_debug(
            time,
            SceneDimensions {
                width: 1280,
                height: 720,
            },
            crate::timeline::DebugRenderOptions::default(),
            &mut None,
        );
        for wire in ["conn1", "conn2"] {
            assert!(
                program.items.iter().any(|item| item.label == wire && !item.commands.is_empty()),
                "{wire} must draw at {time}s"
            );
            assert!(program.precise_bounds.contains_key(wire), "{wire} must have world bounds");
        }
    }
}

// Named probes retain the combined regression and isolate status-only failures.
fn kinetic_probe_timeline() -> crate::timeline::Timeline {
    use std::path::Path;

    use animatix_syntax::module::ModuleGraph;

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/animation/40_delightful_kinetic.amx");
    let mut modules = ModuleGraph::new();
    let program = modules.load_program(&path).expect("load kinetic example");
    let expanded = program.expand_components().expect("expand kinetic example");
    let report = crate::timeline::Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        crate::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    report.output
}

fn kinetic_probe_endpoint(wire: &str, from: bool, target: &str) {
    use crate::timeline::{ConnectorEndpoint, SceneAnchor, TrackAccessor};

    let timeline = kinetic_probe_timeline();
    assert!(timeline.world.get_id(target).is_some());
    let geometry = timeline.world.geometry(timeline.world.get_id(wire).unwrap());
    let fallback = ConnectorEndpoint::Point([0.0, 0.0]);
    let endpoint = if from {
        geometry.connector_from.get(2500, fallback)
    } else {
        geometry.connector_to.get(2500, fallback)
    };
    let anchor = if from {
        SceneAnchor::Right
    } else {
        SceneAnchor::Left
    };
    assert!(matches!(endpoint, ConnectorEndpoint::Target { actor, anchor: Some(actual) }
            if actor == target && actual == anchor));
}

fn kinetic_probe_frames(times: &[f64], bounds: bool) {
    let timeline = kinetic_probe_timeline();
    for &time in times {
        let program = timeline.evaluate_program_with_debug(
            time,
            SceneDimensions {
                width: 1280,
                height: 720,
            },
            crate::timeline::DebugRenderOptions::default(),
            &mut None,
        );
        for wire in ["conn1", "conn2"] {
            if bounds {
                assert!(program.precise_bounds.contains_key(wire), "{wire} at {time}");
            } else {
                assert!(
                    program
                        .items
                        .iter()
                        .any(|item| item.label == wire && !item.commands.is_empty()),
                    "{wire} at {time}"
                );
            }
        }
    }
}

#[test]
fn kinetic_probe_conn1_from_stores_target_anchor() {
    kinetic_probe_endpoint("conn1", true, "client");
}

#[test]
fn kinetic_probe_conn1_to_stores_target_anchor() {
    kinetic_probe_endpoint("conn1", false, "auth");
}

#[test]
fn kinetic_probe_conn2_from_stores_target_anchor() {
    kinetic_probe_endpoint("conn2", true, "auth");
}

#[test]
fn kinetic_probe_conn2_to_stores_target_anchor() {
    kinetic_probe_endpoint("conn2", false, "database");
}

#[test]
fn kinetic_probe_draws_after_entrance() {
    kinetic_probe_frames(&[2.5], false);
}

#[test]
fn kinetic_probe_draws_at_end() {
    kinetic_probe_frames(&[5.0], false);
}

#[test]
fn kinetic_probe_world_bounds() {
    kinetic_probe_frames(&[2.5, 5.0], true);
}

#[test]
fn kinetic_probe_draws_after_reverse_seek() {
    kinetic_probe_frames(&[2.5, 5.0, 2.5], false);
}

#[test]
fn connector_actor_anchor_binding_survives_numeric_build_environment() {
    use crate::ast::Expr;
    use crate::primitives::connector::parse_connector_endpoint;
    use crate::timeline::plan_value::ActorRef;
    use crate::timeline::{ConnectorEndpoint, Environment, SceneAnchor, Value};

    let mut env = Environment::new();
    env.set("node", Value::ActorRef(ActorRef::caller("node")));
    env.set("node.at", Value::Vec2([100.0, 200.0]));
    env.set("node.size", Value::Vec2([40.0, 60.0]));
    let expr = Expr::Path(vec!["node".into(), "right".into()]);
    assert_eq!(
        crate::timeline::evaluate_expr(&expr, &env).unwrap(),
        Value::Vec2([120.0, 200.0])
    );
    let endpoint = parse_connector_endpoint(&expr, &env).unwrap();
    assert!(
        matches!(endpoint, ConnectorEndpoint::Target { actor, anchor: Some(SceneAnchor::Right) }
        if actor == "node")
    );

    env.set("alias", Value::ActorRef(ActorRef::caller("node")));
    let alias = Expr::Path(vec!["alias".into(), "left".into()]);
    let endpoint = parse_connector_endpoint(&alias, &env).unwrap();
    assert!(
        matches!(endpoint, ConnectorEndpoint::Target { actor, anchor: Some(SceneAnchor::Left) }
        if actor == "node")
    );
}

#[test]
fn connector_non_actor_anchor_named_values_remain_numeric_points() {
    use crate::ast::Expr;
    use crate::primitives::connector::parse_connector_endpoint;
    use crate::timeline::{ConnectorEndpoint, Environment, Value};

    let mut env = Environment::new();
    env.set(
        "options",
        Value::Object("Options".into(), [("right".into(), Value::Vec2([5.0, 7.0]))].into()),
    );
    env.set("scene.right", Value::Vec2([1280.0, 360.0]));
    for (name, expected) in [("options", [5.0, 7.0]), ("scene", [1280.0, 360.0])] {
        let expr = Expr::Path(vec![name.into(), "right".into()]);
        let endpoint = parse_connector_endpoint(&expr, &env).unwrap();
        assert!(matches!(endpoint, ConnectorEndpoint::Point(point) if point == expected));
    }
}

#[test]
fn connector_anchor_respects_object_shadowing_and_nested_actor_aliases() {
    use crate::ast::Expr;
    use crate::primitives::connector::parse_connector_endpoint;
    use crate::timeline::plan_value::ActorRef;
    use crate::timeline::{ConnectorEndpoint, Environment, SceneAnchor, Value};

    let mut env = Environment::new();
    env.set("node", Value::ActorRef(ActorRef::caller("node")));
    env.set(
        "node",
        Value::Object("Options".into(), [("right".into(), Value::Vec2([5.0, 7.0]))].into()),
    );
    let expr = Expr::Path(vec!["node".into(), "right".into()]);
    let endpoint = parse_connector_endpoint(&expr, &env).unwrap();
    assert!(matches!(endpoint, ConnectorEndpoint::Point(point) if point == [5.0, 7.0]));

    let child = Value::ActorRef(ActorRef::caller("actual.child"));
    env.set("holder", Value::Object("Holder".into(), [("child".into(), child)].into()));
    let expr = Expr::Path(vec!["holder".into(), "child".into(), "right".into()]);
    let endpoint = parse_connector_endpoint(&expr, &env).unwrap();
    assert!(
        matches!(endpoint, ConnectorEndpoint::Target { actor, anchor: Some(SceneAnchor::Right) }
        if actor == "actual.child")
    );
}
