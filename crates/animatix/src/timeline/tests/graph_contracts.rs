//! Runtime contract checks for Graph's boolean grid and tick flags.

use std::path::Path;

use animatix_syntax::module::ModuleGraph;

use super::support::{baked_paths, build};
use crate::timeline::Timeline;

fn path_stroke_widths(timeline: &Timeline, label: &str) -> Vec<f64> {
    baked_paths(timeline, label)
        .iter()
        .filter_map(|path| path.stroke.map(|(_, width)| width as f64))
        .collect()
}

fn has_width(widths: &[f64], expected: f64) -> bool {
    widths.iter().any(|width| (*width - expected).abs() < 1e-6)
}

#[test]
fn graph_boolean_variables_and_component_params_reach_runtime_paths() {
    let source = r#"
let enabled = true
pub component AxisGraph(draw: Bool) {
  graph: Graph, size: (300, 240), x_domain: (-2, 2), y_domain: (-2, 2),
    grid: draw, ticks: draw
}

from_variable: Graph, size: (300, 240), x_domain: (-2, 2), y_domain: (-2, 2),
  grid: enabled, ticks: enabled
from_component: AxisGraph, draw: true
disabled: AxisGraph, draw: false
"#;
    let mut modules = ModuleGraph::new();
    let mut program = modules
        .load_program_with_source(Path::new("virtual:///graph_flags.amx"), Some(source))
        .expect("load Graph flag source");
    let diagnostics = program.typecheck();
    assert!(
        diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "component argument diagnostics: {diagnostics:?}"
    );
    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    assert!(expansion_errors.is_empty(), "expansion errors: {expansion_errors:?}");
    let report = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        crate::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    let timeline = report.output;

    for label in ["from_variable", "from_component"] {
        let widths = path_stroke_widths(&timeline, label);
        assert!(has_width(&widths, 1.0), "{label} should render grid paths: {widths:?}");
        assert!(has_width(&widths, 1.5), "{label} should render tick paths: {widths:?}");
    }
    let disabled_widths = path_stroke_widths(&timeline, "disabled");
    assert!(
        !has_width(&disabled_widths, 1.0),
        "false must suppress grid paths: {disabled_widths:?}"
    );
    assert!(
        !has_width(&disabled_widths, 1.5),
        "false must suppress tick paths: {disabled_widths:?}"
    );
    assert!(has_width(&disabled_widths, 2.0), "axes remain visible: {disabled_widths:?}");
}

#[test]
fn number_plane_grid_is_independent_of_graph_grid_and_ticks_flags() {
    let source = r#"
plain: NumberPlane, size: (200, 160), x_domain: (-5, 5), y_domain: (-4, 4),
  x_range: (-5, 5, 1), y_range: (-4, 4, 1)
explicit: NumberPlane, size: (200, 160), x_domain: (-5, 5), y_domain: (-4, 4),
  x_range: (-5, 5, 1), y_range: (-4, 4, 1), grid: false, ticks: false
"#;
    let report = build(source);
    let plain = baked_paths(&report.output, "plain");
    let explicit = baked_paths(&report.output, "explicit");
    let encode = |paths: &[crate::timeline::VelloPath]| {
        paths
            .iter()
            .map(|path| (path.path.elements().to_vec(), path.stroke.map(|(_, width)| width)))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        encode(&plain),
        encode(&explicit),
        "NumberPlane draws from x_range/y_range and does not consume Graph-only flags"
    );
}

fn nested_graph_entrance_fixture() -> Timeline {
    let source = r#"
config { resolution: (1280, 720), duration: 5 }
pub component PlotCard() {
    graph: Graph, size: (400, 300), x_domain: (-pi, pi), y_domain: (-2, 2) {
        curve: PlotCurve, kind: "cartesian", func: (x) => sin(x), stroke_width: 4
        hidden: PlotCurve, kind: "cartesian", func: (x) => cos(x), stroke_width: 4, opacity: 0
    }
}
pub component Pane() {
    root: Group, at: (0, 0), opacity: 1 {
        card: PlotCard, at: (320, 240)
    }
}
live: Pane
idle: Pane
#1s
fade_in(target: live.card, duration: 1s)
#3s
fade_in(target: live.card.hidden, duration: 1s)
"#;
    let mut modules = ModuleGraph::new();
    let mut program = modules
        .load_program_with_source(Path::new("virtual:///nested_graph_entrance.amx"), Some(source))
        .expect("load nested Graph entrance fixture");
    let type_diagnostics = program.typecheck();
    assert!(type_diagnostics.iter().all(|d| !d.is_error()), "{type_diagnostics:?}");
    let mut errors = Vec::new();
    let expanded = program.expand_components(&mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    let report = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        crate::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    for label in [
        "live.card",
        "live.card.curve",
        "live.card.hidden",
        "idle.card",
        "idle.card.curve",
        "idle.card.hidden",
    ] {
        assert!(report.output.world.get_id(label).is_some(), "missing {label}");
    }
    report.output
}

#[test]
fn nested_component_graph_entrance_preserves_authored_hidden_child_across_seek_and_cache() {
    let timeline = nested_graph_entrance_fixture();
    let dimensions = crate::timeline::SceneDimensions {
        width: 1280,
        height: 720,
    };
    // Repeat frames for cache hits, seek backwards after the first entrance,
    // then repeat after the explicitly hidden child gets its own entrance.
    for time in [
        0.0, 1.0, 1.5, 2.0, 2.0, 0.0, 2.0, 3.0, 3.5, 4.0, 4.0, 0.0, 4.0,
    ] {
        let mut filter_backend = None;
        let program = timeline.evaluate_program_with_debug(
            time,
            dimensions,
            crate::timeline::DebugRenderOptions::default(),
            &mut filter_backend,
        );
        assert!(program.diagnostics.iter().all(|d| !d.is_error()), "{:?}", program.diagnostics);
        for label in ["live.card", "live.card.curve", "live.card.hidden"] {
            let start = if label.ends_with(".hidden") { 3.0 } else { 1.0 };
            let items: Vec<_> = program.items.iter().filter(|item| item.label == label).collect();
            if time <= start {
                assert!(
                    items.iter().all(|item| item.opacity == 0.0 || item.commands.is_empty()),
                    "{label} leaked before its entrance at {time}s"
                );
            } else {
                assert!(!items.is_empty(), "{label} missing after entrance at {time}s");
                assert!(items.iter().any(|item| !item.commands.is_empty()), "{label} has no draws");
                assert!(items.iter().all(|item| item.opacity > 0.0), "{label} still hidden");
                if time < start + 1.0 {
                    assert!(items.iter().all(|item| item.opacity < 1.0), "{label} skipped fade");
                } else {
                    assert!(items.iter().all(|item| (item.opacity - 1.0).abs() < 1e-6));
                }
                let bounds = program.precise_bounds.get(label).expect("visible actor bounds");
                assert!(bounds.width() > 0.0 && bounds.height() > 0.0, "{label}: {bounds:?}");
                assert_eq!(
                    timeline.precise_bounds_for(label),
                    Some(*bounds),
                    "{label} cache bounds"
                );
            }
        }
        for label in ["idle.card", "idle.card.curve", "idle.card.hidden"] {
            assert!(
                program
                    .items
                    .iter()
                    .filter(|item| item.label == label)
                    .all(|item| item.opacity == 0.0 || item.commands.is_empty()),
                "entrance leaked to the other component instance: {label} at {time}s"
            );
        }
    }
}
