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
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
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
