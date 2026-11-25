use super::support::{build, build_timeline};
use crate::scene::Draw;
use crate::timeline::{DebugRenderOptions, SceneDimensions};

#[test]
fn same_scene_handoff_bakes_scaled_parents_and_seeks_without_double_paint() {
    let timeline = build_timeline(
        r#"
config { canvas_size: (400, 180) }
#0s
left: Group, at: (80, 90) {
  source: Group, at: (40, 0), opacity: 1 {
    source_ink: Rect, size: (20, 20), color: white
  }
}
right: Group, at: (240, 90), scale: 0.5 {
  destination: Group, at: (20, 0), opacity: 0 {
    destination_ink: Rect, size: (20, 20), color: white
  }
}
proxy: Group, at: (0, 0), opacity: 0 {
  proxy_ink: Rect, size: (20, 20), color: white
}
left.scale = 2 [1s, ease: linear]
#500ms
handoff(target: proxy, from: source, to: destination, duration: 500ms, ease: linear)
#750ms
right.scale = 1.5 [250ms, ease: linear]
"#,
    );
    let dims = SceneDimensions {
        width: 400,
        height: 180,
    };
    let sample = |time| {
        let program = timeline.evaluate_program_with_debug(
            time,
            dims,
            DebugRenderOptions::default(),
            &mut None,
        );
        let mut visible = program.items.iter().filter(|item| {
            matches!(item.label.as_str(), "source_ink" | "proxy_ink" | "destination_ink")
                && item.opacity > 0.0
        });
        let item = visible.next().expect("one visible placement");
        assert!(visible.next().is_none(), "more than one visible placement at {time}s");
        let paints = program
            .scene
            .draws()
            .iter()
            .filter(|draw| {
                matches!(draw, Draw::Fill { brush: peniko::Brush::Solid(color), .. }
                if color.components == [1.0; 4])
            })
            .count();
        assert_eq!(paints, 1, "expected just one visible placement at {time}s");
        (item.label.clone(), item.transform.as_coeffs())
    };
    let times = [0.499, 0.5, 0.75, 0.999, 1.0];
    let forward = times.map(sample);
    for (index, time) in times.iter().copied().enumerate().rev() {
        assert_eq!(sample(time), forward[index], "reverse seek mismatch at {time}s");
    }
    for ((actor, transform), (expected_actor, scale, x)) in forward.iter().zip([
        ("source_ink", 1.499, 139.96),
        ("proxy_ink", 1.5, 140.0),
        ("proxy_ink", 1.5, 205.0),
        ("proxy_ink", 1.5, 269.74),
        ("destination_ink", 1.5, 270.0),
    ]) {
        assert_eq!(actor, expected_actor);
        assert!((transform[0] - scale).abs() < 0.01, "{transform:?}");
        assert!((transform[4] - x).abs() < 0.02, "{transform:?}");
    }
}

#[test]
fn same_scene_handoff_rejects_a_non_root_proxy() {
    let report = build(
        r#"
#0s
parent: Group {
  proxy: Group, opacity: 0
}
origin: Group, opacity: 1
destination: Group, opacity: 0
#100ms
handoff(target: proxy, from: origin, to: destination, duration: 300ms)
"#,
    );
    assert!(
        report
            .diagnostics
            .iter()
            .any(|d| { d.is_error() && d.message.contains("target must be a root Group") }),
        "{:?}",
        report.diagnostics
    );
}
