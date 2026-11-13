//! TextPath cached bounds must follow each sampled glyph path.

use super::support::build_timeline;
use super::*;
use kurbo::Shape;

fn rects_match(left: kurbo::Rect, right: kurbo::Rect) -> bool {
    (left.x0 - right.x0).abs() < 0.05
        && (left.y0 - right.y0).abs() < 0.05
        && (left.x1 - right.x1).abs() < 0.05
        && (left.y1 - right.y1).abs() < 0.05
}

fn assert_text_actor_bounds_match_paths(timeline: &Timeline, label: &str, time_s: f64) {
    let mut filter_backend = None;
    let program = timeline.evaluate_program_with_debug(
        time_s,
        SceneDimensions {
            width: 1280,
            height: 720,
        },
        DebugRenderOptions {
            compute_hit_regions: true,
            ..Default::default()
        },
        &mut filter_backend,
    );
    assert!(
        program.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "sample at {time_s}s must not report text compile/render failures: {:?}",
        program.diagnostics
    );
    let item = program
        .items
        .iter()
        .find(|item| item.label == label)
        .unwrap_or_else(|| panic!("expected sampled text item '{label}' at {time_s}s"));

    let mut local_bounds = None;
    for command in &item.commands {
        let crate::primitives::RenderCommand::Text { paths, .. } = command else {
            continue;
        };
        for glyph in paths.iter() {
            let path_bounds = glyph.path.bounding_box();
            assert!(
                rects_match(glyph.bounds, path_bounds),
                "{label} at {time_s}s: cached glyph bounds {:?} differ from path bounds {:?}",
                glyph.bounds,
                path_bounds
            );
            local_bounds = Some(
                local_bounds.map_or(path_bounds, |current: kurbo::Rect| current.union(path_bounds)),
            );
        }
    }
    let local_bounds = local_bounds.expect("text command should contain glyph paths");
    let expected_world =
        crate::timeline::scene_eval::transform_rect_bbox(&item.transform, local_bounds);
    let precise = program.precise_bounds.get(label).copied().expect("precise text bounds");
    assert!(
        rects_match(precise, expected_world),
        "{label} at {time_s}s: precise bounds {:?} differ from transformed path bounds {:?}",
        precise,
        expected_world
    );
    let hit = timeline
        .hit_regions()
        .into_iter()
        .find(|(name, _)| name == label)
        .map(|(_, bounds)| bounds)
        .expect("text hit region");
    assert!(
        rects_match(hit, expected_world),
        "{label} at {time_s}s: hit region {:?} differs from transformed path bounds {:?}",
        hit,
        expected_world
    );
}

#[test]
fn anchored_text_paths_bounds_and_picking_follow_sampled_glyphs() {
    let timeline = build_timeline(
        r#"
#0s
left: Text, text: "LEFT ANCHOR", text_anchor: "left", at: (100, 100), font_size: 32
right: Text, text: "RIGHT ANCHOR", text_anchor: "right", at: (900, 100), font_size: 32
morph: Text, text: "BEFORE MORPH", text_anchor: "right", at: (900, 190), font_size: 32
reactive: Text, text: "i", text_anchor: "left", at: (100, 280), font_size: 32
always { reactive.text = if (t < 1.0) { "i" } else { "MMMMMMMM" } }
column: Col, size: (300, auto), at: (360, 430), gap: 8 {
  wrapped: Text, text: "Anchored text wraps inside its column and keeps every glyph bound aligned.",
    text_anchor: "left", font_size: 24, text_max_width: 260
}

#1s
left.text = "A SHORT REPLACEMENT" [0s]
right.text = "A LONGER RIGHT-ANCHORED REPLACEMENT" [0s]

#2s
reveal-in left [800ms, by: word, offset_y: 14, ease: ease-out]

#4s
morph: Text, text: "AFTER MORPH WITH A DIFFERENT WIDTH", text_anchor: "right",
  at: (900, 190), font_size: 32 [2s, ease: ease-in-out]
"#,
    );

    let wrapped_id = timeline.world.get_id("wrapped").expect("wrapped text actor");
    let wrapped_paths = timeline
        .world
        .text_or_default(wrapped_id)
        .evaluate_paths(timeline.world.style(wrapped_id), 0);
    let wrapped_bounds = wrapped_paths
        .iter()
        .map(|glyph| glyph.path.bounding_box())
        .reduce(|bounds, glyph| bounds.union(glyph))
        .expect("wrapped text has glyph paths");
    assert!(
        wrapped_bounds.height() > 36.0,
        "Col text fixture must actually wrap to multiple lines: {wrapped_bounds:?}"
    );

    for (label, time_s) in [
        ("left", 0.0),
        ("right", 0.0),
        ("morph", 0.0),
        ("reactive", 0.0),
        ("reactive", 2.0),
        ("wrapped", 0.0),
        ("left", 0.999),
        ("left", 1.0),
        ("right", 1.0),
        ("left", 2.4),
        ("left", 2.9),
        ("morph", 4.0),
        ("morph", 5.0),
        ("morph", 6.0),
    ] {
        assert_text_actor_bounds_match_paths(&timeline, label, time_s);
    }
}
