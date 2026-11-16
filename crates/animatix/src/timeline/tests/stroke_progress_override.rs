use super::support::build_clean;
use crate::primitives::RenderCommand;
use crate::timeline::{DebugRenderOptions, SceneDimensions, Timeline, VelloPath};

fn actor_paths(timeline: &Timeline, label: &str, time_s: f64) -> Vec<VelloPath> {
    let mut filter_backend = None;
    let program = timeline.evaluate_program_with_debug(
        time_s,
        SceneDimensions {
            width: 640,
            height: 360,
        },
        DebugRenderOptions::default(),
        &mut filter_backend,
    );

    let mut paths = Vec::new();
    for item in program.items.into_iter().filter(|item| item.label == label) {
        for command in item.commands {
            if let RenderCommand::Paths { paths: rendered } = command {
                paths.extend(rendered);
            }
        }
    }
    paths
}

fn stroke_paths(timeline: &Timeline, label: &str, time_s: f64) -> Vec<VelloPath> {
    actor_paths(timeline, label, time_s)
        .into_iter()
        .filter(|path| path.stroke.is_some())
        .collect()
}

fn endpoints(path: &VelloPath) -> (kurbo::Point, kurbo::Point) {
    let mut start = None;
    let mut end = None;
    for element in path.path.elements() {
        match element {
            kurbo::PathEl::MoveTo(point) => {
                start.get_or_insert(*point);
            },
            kurbo::PathEl::LineTo(point)
            | kurbo::PathEl::QuadTo(_, point)
            | kurbo::PathEl::CurveTo(_, _, point) => end = Some(*point),
            kurbo::PathEl::ClosePath => {},
        }
    }
    (start.expect("stroke path has a start"), end.expect("stroke path has an end"))
}

#[test]
fn always_override_controls_shape_stroke_progress() {
    let timeline = build_clean(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

signal: Path, commands: {move_to(100, 180), line_to(540, 180)},
  stroke: accent.primary, stroke_width: 6, fill_opacity: 0.0, opacity: 1.0

always { signal.stroke_progress = t / 2.0 }
"#,
    );

    assert!(stroke_paths(&timeline, "signal", 0.0).is_empty());

    let half = stroke_paths(&timeline, "signal", 1.0);
    assert_eq!(half.len(), 1);
    let (start, end) = endpoints(&half[0]);
    assert!((start.x - 100.0).abs() < 0.5 && (start.y - 180.0).abs() < 0.5);
    assert!((end.x - 320.0).abs() < 1.0 && (end.y - 180.0).abs() < 0.5, "got {end:?}");

    let full = stroke_paths(&timeline, "signal", 2.0);
    assert_eq!(full.len(), 1);
    let (_, end) = endpoints(&full[0]);
    assert!((end.x - 540.0).abs() < 0.5 && (end.y - 180.0).abs() < 0.5, "got {end:?}");
}

#[test]
fn always_override_controls_plot_stroke_progress() {
    let timeline = build_clean(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

graph: Graph, x_domain: (-10, 10), y_domain: (-10, 10), size: (400, 400), at: (320, 180) {
  signal: PlotCurve, kind: "cartesian", func: (x) => x,
    color: accent.primary, stroke_width: 4, opacity: 1.0
}

always { signal.stroke_progress = t / 2.0 }
"#,
    );

    assert!(stroke_paths(&timeline, "signal", 0.0).is_empty());

    let half = stroke_paths(&timeline, "signal", 1.0);
    assert_eq!(half.len(), 1);
    let (start, half_end) = endpoints(&half[0]);

    let full = stroke_paths(&timeline, "signal", 2.0);
    assert_eq!(full.len(), 1);
    let (_, full_end) = endpoints(&full[0]);
    let expected_midpoint = start + (full_end - start) * 0.5;
    assert!(
        (half_end - expected_midpoint).hypot() < 1.0,
        "half progress should end at the plot's arc-length midpoint: start={start:?}, half={half_end:?}, full={full_end:?}"
    );
}
