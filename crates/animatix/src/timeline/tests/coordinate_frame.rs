//! Phase 4: plot-host coordinate frames live on the entity, carry across
//! scenes, and are the only source for hosted-child coordinate resolution.

use crate::ast::{Expr, TargetSegment};
use crate::diagnostics::DiagnosticSeverity;
use crate::timeline::{Timeline, Value};
use kurbo::Shape as _;

fn build(source: &str) -> Timeline {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new()).output
}

#[test]
fn plot_child_uses_component_domain() {
    let timeline = build(
        r#"
        g: Graph, size: (400, 200), x_domain: (0, 4), y_domain: (0, 4) {
            c: PlotCurve, kind: "cartesian", func: (x) => x
        }
        "#,
    );

    let frame = timeline.coordinate_frame_for("g").expect("Graph must carry a coordinate frame");
    assert_eq!(frame.x_domain, [0.0, 4.0]);
    assert_eq!(frame.y_domain, [0.0, 4.0]);

    // The env entry is an authored-expression variable derived from the
    // component, not a second source of truth.
    assert_eq!(
        timeline.env().get("g_x_domain"),
        Some(Value::Vec2([0.0, 4.0])),
        "the env side channel must mirror the component at build time"
    );

    let track = timeline.get_entity_id("c").expect("plot child track");
    let plot = timeline.world.procedural_plot(track).expect("procedural plot");
    assert_eq!(plot.p_x_domain, [0.0, 4.0], "child sampled the host domain");
    assert_eq!(plot.p_y_domain, [0.0, 4.0]);
    assert_eq!(plot.p_size, [400.0, 200.0], "child sampled the host full size");

    // The curve y = x over domain [0, 4] fills the 400x200 plot box: x maps to
    // [-200, 200] and y maps to [-100, 100]. A [-10, 10] fallback would keep
    // the curve in the middle of the box instead.
    let paths = timeline
        .world
        .shape_or_default(track)
        .vector_paths
        .as_ref()
        .expect("generated curve paths")
        .evaluate(0);
    assert!(!paths.is_empty(), "curve must generate at least one path");
    let bounds = paths[0].path.bounding_box();
    assert!(
        (bounds.x0 + 200.0).abs() < 1.0 && (bounds.x1 - 200.0).abs() < 1.0,
        "curve x extent should span the host domain [-200, 200], got {bounds:?}"
    );
    assert!(
        (bounds.y0 + 100.0).abs() < 1.0 && (bounds.y1 - 100.0).abs() < 1.0,
        "curve y extent should span the host domain [-100, 100], got {bounds:?}"
    );

    // A hosted child resolves through its parent and carries no frame itself.
    assert!(timeline.coordinate_frame_for("c").is_none());
}

#[test]
fn assignment_rebuild_uses_coordinate_frame() {
    let mut timeline =
        build(r#"g: Graph, size: (400, 300), x_domain: (-3, 3), y_domain: (-3, 3), ticks: true"#);

    // Change the domain through the component and leave the now-stale env
    // values the build derived. A rebuild that consulted env would draw the
    // old [-3, 3] domain (7 ticks per axis); the component's [-2, 2] domain
    // puts 9 ticks on each axis.
    let mut frame = timeline.coordinate_frame_for("g").expect("Graph frame").clone();
    frame.x_domain = [-2.0, 2.0];
    frame.y_domain = [-2.0, 2.0];
    assert!(timeline.set_coordinate_frame("g", frame), "component write must land");
    timeline.env_mut().set("g_x_domain", Value::Vec2([-10.0, 10.0]));
    timeline.env_mut().set("g_y_domain", Value::Vec2([-10.0, 10.0]));

    let target = vec![TargetSegment::Static("g".to_string())];
    let value = Expr::Tuple(vec![Expr::Num(600.0), Expr::Num(400.0)]);
    let mut diagnostics = Vec::new();
    timeline.process_assignment_statement(
        &target,
        "size",
        &value,
        &[],
        None,
        500.0,
        &mut diagnostics,
    );
    assert!(
        diagnostics.iter().all(|d| d.severity != DiagnosticSeverity::Error),
        "size assignment diagnostics: {diagnostics:?}"
    );

    let track = timeline.get_entity_id("g").expect("graph track");
    let paths = timeline
        .world()
        .shape_or_default(track)
        .vector_paths
        .as_ref()
        .expect("graph axis paths")
        .evaluate(500);

    // The tick path is the one stroked at width 1.5 (axis is 2.0, grid absent).
    let tick_path = paths
        .iter()
        .find(|path| path.stroke.as_ref().is_some_and(|(_, width)| (*width - 1.5).abs() < 1e-6))
        .expect("tick path (ticks: true)");
    assert_eq!(
        tick_path.path.elements().len(),
        36,
        "the rebuild must resolve 9 ticks per axis from the component's [-2, 2] \
         domain (2 elements per tick, two axes), not the stale [-3, 3] or [-10, 10] env"
    );

    // The size assignment does not rewrite the frame.
    assert_eq!(
        timeline.coordinate_frame_for("g").map(|frame| frame.x_domain),
        Some([-2.0, 2.0])
    );
}

#[test]
fn carried_graph_keeps_coordinate_frame() {
    let source = concat!(
        "# SceneA\n",
        "#0s\n",
        "g: Graph, size: (400, 200), x_domain: (0, 4), y_domain: (0, 2), padding: (4, 4, 2, 2) {\n",
        "    c: PlotCurve, kind: \"cartesian\", func: (x) => x / 2\n",
        "}\n",
        "#1s\n",
        "persist g\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
    );
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {parse_errors:?}");
    let ast = ast.expect("parsed AST");
    let report = crate::composition::Composition::build(&ast, &std::collections::HashMap::new());

    let errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|d| d.severity == DiagnosticSeverity::Error)
        .collect();
    assert!(errors.is_empty(), "carry build errors: {errors:?}");

    let scene_b = report.output.scenes.get("SceneB").expect("SceneB must exist");
    let frame = scene_b
        .timeline
        .coordinate_frame_for("g")
        .expect("the carried Graph must keep its coordinate frame component");
    assert_eq!(frame.x_domain, [0.0, 4.0]);
    assert_eq!(frame.y_domain, [0.0, 2.0]);
    assert_eq!(frame.padding, [4.0, 4.0, 2.0, 2.0]);

    // A plot child declared against the carried host resolves from the carried
    // component (this is the lookup `process_plot_actor` performs).
    let context = scene_b
        .timeline
        .resolve_coordinate_context(Some("g"))
        .expect("the carried graph must resolve a coordinate context");
    assert_eq!(context.host_id, scene_b.timeline.get_entity_id("g").unwrap());
    assert_eq!(context.x_domain, [0.0, 4.0]);
    assert_eq!(context.y_domain, [0.0, 2.0]);

    // The carried child keeps the geometry it was sampled with under that domain.
    let child = scene_b.timeline.get_entity_id("c").expect("carried plot child");
    let plot = scene_b.timeline.world.procedural_plot(child).expect("carried procedural plot");
    assert_eq!(plot.p_x_domain, [0.0, 4.0]);
    assert_eq!(plot.p_y_domain, [0.0, 2.0]);
}

#[test]
fn every_plot_host_has_coordinate_frame() {
    let timeline = build(
        r#"
        g: Graph, size: (300, 200), x_domain: (0, 6), y_domain: (-1, 1) {
            c: PlotCurve, kind: "cartesian", func: (x) => sin(x)
        }
        np: NumberPlane, size: (200, 200), x_domain: (-5, 5), y_domain: (-5, 5)
        "#,
    );

    let mut hosts = 0;
    for entity in timeline.world().views() {
        if timeline.world().caps(entity.id()).plot_host {
            hosts += 1;
            assert!(
                entity.coordinate_frame().is_some(),
                "plot host '{}' was declared without a coordinate frame",
                entity.label()
            );
        }
    }
    assert_eq!(hosts, 1, "the fixture has exactly one plot host (Graph)");
    assert_eq!(timeline.coordinate_frame_for("g").map(|frame| frame.x_domain), Some([0.0, 6.0]));

    // NumberPlane is a standalone coordinate system, not a plot host: it hosts
    // no child plots and carries no frame. A hosted plot child resolves through
    // its parent, so it carries none either.
    let number_plane = timeline.entity("np").expect("NumberPlane entity");
    assert!(!timeline.world().caps(number_plane.id()).plot_host);
    assert!(number_plane.coordinate_frame().is_none());
    assert!(timeline.entity("c").expect("curve entity").coordinate_frame().is_none());
}
