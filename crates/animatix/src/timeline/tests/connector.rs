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
