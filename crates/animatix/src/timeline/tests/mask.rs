use super::support::{build, build_timeline};
use crate::scene::Draw;
use crate::timeline::{SceneDimensions, Timeline};
use kurbo::{BezPath, Shape};

const DIMS: SceneDimensions = SceneDimensions {
    width: 360,
    height: 180,
};

fn assert_two_clips(timeline: &Timeline) {
    let scene = timeline.evaluate(0.0, DIMS);
    let paths: Vec<_> = scene
        .draws()
        .iter()
        .filter_map(|draw| match draw {
            Draw::Push { clip, .. } => Some(clip),
            _ => None,
        })
        .collect();
    assert_eq!(paths.len(), 2);
    for path in &paths {
        assert_eq!(path.bounding_box(), kurbo::Rect::new(-55.0, -40.0, 55.0, 40.0));
        assert_rounded(path);
    }
    assert_ne!(paths[0].elements(), paths[1].elements());
    assert!(matches!(
        scene.draws(),
        [
            Draw::Fill { .. }, // canvas background
            Draw::Push { .. },
            Draw::Fill { .. },
            Draw::Pop,
            Draw::Push { .. },
            Draw::Fill { .. },
            Draw::Pop,
        ]
    ));
}

fn assert_rounded(path: &BezPath) {
    assert!(
        path.elements()
            .iter()
            .any(|element| matches!(element, kurbo::PathEl::CurveTo(..)))
    );
}

#[test]
fn explicit_mask_clips_are_independent() {
    assert_two_clips(&build_timeline(
        r#"
#0s
left: Mask, at: (90, 90), clip: left_clip {
  left_clip: Rect, size: (110, 80), corner_radius: 8
  left_content: Rect, size: (130, 100)
}
right: Mask, at: (270, 90), clip: right_clip {
  right_clip: Rect, size: (110, 80), corner_radius: 24
  right_content: Rect, size: (130, 100)
}
"#,
    ));
}

#[test]
fn component_instances_remap_mask_clip_to_their_own_child() {
    use animatix_syntax::module::ModuleGraph;
    let mut modules = ModuleGraph::new();
    let program = modules
        .load_program_with_source(
            std::path::Path::new("virtual:///component-mask.amx"),
            Some(
                r#"
component Tile(r: Num) {
  tile: Mask, clip: window {
    window: Rect, size: (110, 80), corner_radius: r
    content: Rect, size: (130, 100)
  }
}
#0s
first: Tile, at: (90, 90), r: 8
second: Tile, at: (270, 90), r: 24
"#,
            ),
        )
        .expect("load component scene");
    let expanded = program.expand_components().expect("expand component scene");
    let report = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        crate::timeline::BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(!report.diagnostics.iter().any(|d| d.is_error()), "{:?}", report.diagnostics);
    assert_two_clips(&report.output);
}

#[test]
fn mask_rejects_a_clip_in_another_mask() {
    let report = build(
        r#"
#0s
left: Mask {
  left_clip: Rect, size: (110, 80)
}
right: Mask, clip: left_clip {
  right_content: Rect, size: (130, 100)
}
"#,
    );
    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            diagnostic.is_error()
                && diagnostic.message.contains("must be one of its direct children")
                && diagnostic.location.subject.as_deref() == Some("right.clip")
        }),
        "{:?}",
        report.diagnostics
    );
}
