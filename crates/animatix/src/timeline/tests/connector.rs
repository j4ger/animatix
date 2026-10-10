use super::*;
use animatix_syntax::parser::parse_source;
use crate::primitives::connector::{build_connector_path, route_connector};
use crate::timeline::{ConnectorRouting, SceneDimensions};

#[test]
fn test_connector_routing_algorithms() {
    let from = [100.0, 100.0];
    let norm_from = [1.0, 0.0]; // Right
    let to = [300.0, 200.0];
    let norm_to = [-1.0, 0.0]; // Left

    // Straight
    let straight_pts = route_connector(from, norm_from, to, norm_to, ConnectorRouting::Straight, 20.0);
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
    assert_eq!(elbow_pts[2], [200.0, to[1]]);   // Mid-x
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
    let (stmts, errors) = parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = Timeline::build_with_diagnostics(&stmts.expect("parsed AST"), &std::collections::HashMap::new());
    if !report.output.tracks.contains_key("wire") {
        panic!("Connector wire track missing! Diagnostics: {:#?}", report.diagnostics);
    }
    let dims = SceneDimensions { width: 1920, height: 1080 };
    let _scene = report.output.evaluate(0.0, dims);
    assert!(report.output.tracks.contains_key("wire"), "Connector wire track should be present");
}
