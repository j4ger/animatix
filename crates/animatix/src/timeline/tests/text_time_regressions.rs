//! Regressions for discrete text keyframes and Counter frame overrides.

use super::support::build_output;
use super::*;
use kurbo::{PathEl, Point};

fn text_paths_at(
    timeline: &Timeline,
    label: &str,
    time_ms: u64,
) -> Vec<crate::renderer::types::TextPath> {
    let id = timeline.world.tracks.get_id(label).expect("text actor");
    let text = timeline.world.text_or_default(id);
    text.evaluate_paths(timeline.world.style(id), time_ms)
}

fn path_samples(path: &kurbo::BezPath) -> Vec<Point> {
    let mut samples = Vec::new();
    let mut current = Point::ORIGIN;
    let mut start = Point::ORIGIN;
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(point) => {
                current = point;
                start = point;
                samples.push(point);
            },
            PathEl::LineTo(end) => {
                sample_segment(&mut samples, |t| current.lerp(end, t));
                current = end;
            },
            PathEl::QuadTo(control, end) => {
                let from = current;
                sample_segment(&mut samples, |t| {
                    let u = 1.0 - t;
                    Point::new(
                        u * u * from.x + 2.0 * u * t * control.x + t * t * end.x,
                        u * u * from.y + 2.0 * u * t * control.y + t * t * end.y,
                    )
                });
                current = end;
            },
            PathEl::CurveTo(c1, c2, end) => {
                let from = current;
                sample_segment(&mut samples, |t| {
                    let u = 1.0 - t;
                    Point::new(
                        u.powi(3) * from.x
                            + 3.0 * u * u * t * c1.x
                            + 3.0 * u * t * t * c2.x
                            + t.powi(3) * end.x,
                        u.powi(3) * from.y
                            + 3.0 * u * u * t * c1.y
                            + 3.0 * u * t * t * c2.y
                            + t.powi(3) * end.y,
                    )
                });
                current = end;
            },
            PathEl::ClosePath => {
                sample_segment(&mut samples, |t| current.lerp(start, t));
                current = start;
            },
        }
    }
    samples
}

fn sample_segment(samples: &mut Vec<Point>, at: impl Fn(f64) -> Point) {
    for i in 1..=16 {
        samples.push(at(i as f64 / 16.0));
    }
}

fn assert_same_glyph_geometry(
    left: &[crate::renderer::types::TextPath],
    right: &[crate::renderer::types::TextPath],
) {
    assert_eq!(left.len(), right.len(), "glyph count changed before the step");
    for (left, right) in left.iter().zip(right) {
        assert_eq!(left.glyph_center, right.glyph_center, "glyph moved before the step");
        let left_samples = path_samples(&left.path);
        let right_samples = path_samples(&right.path);
        for distance in left_samples
            .iter()
            .map(|point| {
                right_samples
                    .iter()
                    .map(|candidate| point.distance(*candidate))
                    .fold(f64::INFINITY, f64::min)
            })
            .chain(right_samples.iter().map(|point| {
                left_samples
                    .iter()
                    .map(|candidate| point.distance(*candidate))
                    .fold(f64::INFINITY, f64::min)
            }))
        {
            assert!(
                distance < 0.01,
                "glyph geometry changed before the step: nearest sampled distance {distance}"
            );
        }
    }
}

fn counter_paths_from_program(
    timeline: &Timeline,
    time_s: f64,
) -> (Vec<crate::renderer::types::TextPath>, Option<kurbo::BezPath>) {
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
    assert!(
        program.diagnostics.iter().all(|diagnostic| diagnostic.code
            != crate::diagnostics::DiagnosticCode::ModifierRuntimeError),
        "Counter reactive expression must evaluate without runtime diagnostics: {:?}",
        program.diagnostics
    );
    let item = program.items.iter().find(|item| item.label == "count").expect("Counter item");
    let crate::primitives::RenderCommand::Text {
        paths, clip_path, ..
    } = &item.commands[0]
    else {
        panic!("Counter should emit text paths");
    };
    (paths.to_vec(), clip_path.clone())
}

#[test]
fn future_instant_text_assignments_do_not_change_past_or_reverse_seek_samples() {
    let timeline = build_output(
        r#"
#0s
title: Text, text: "BEFORE LONG", at: (200, 200)

#10s
title.text = "X"

#20s
title.text = "THIRD" [0s]
"#,
    );
    let id = timeline.world.tracks.get_id("title").expect("title actor");
    let text = timeline.world.text_or_default(id);
    let before = text_paths_at(&timeline, "title", 0);
    for time_ms in [5_000, 9_999, 5_000] {
        assert_eq!(text.text_content.get(time_ms, String::new()), "BEFORE LONG");
        assert_same_glyph_geometry(&before, &text_paths_at(&timeline, "title", time_ms));
    }
    assert_eq!(text.text_content.get(10_000, String::new()), "X");
    assert_eq!(text.text_content.get(19_999, String::new()), "X");
    assert_eq!(text.text_content.get(20_000, String::new()), "THIRD");
    assert_eq!(text.text_content.get(5_000, String::new()), "BEFORE LONG");
}

#[test]
fn explicit_zero_duration_assignment_is_a_step_for_ordinary_and_typst_text() {
    let timeline = build_output(
        r#"
#0s
title: Text, text: "旧い文章", at: (200, 200)
formula: Typst, text: "$x + y$", at: (200, 260)

#10s
title.text = "新しい文" [0s]
formula.text = "$a^2$" [0s]
"#,
    );
    for label in ["title", "formula"] {
        let id = timeline.world.tracks.get_id(label).expect("text actor");
        let text = timeline.world.text_or_default(id);
        let before = text_paths_at(&timeline, label, 0);
        assert_same_glyph_geometry(&before, &text_paths_at(&timeline, label, 5_000));
        assert_eq!(
            text.text_content.get(9_999, String::new()),
            if label == "title" {
                "旧い文章"
            } else {
                "$x + y$"
            }
        );
    }
    assert_eq!(
        timeline
            .world
            .text_or_default(timeline.world.tracks.get_id("title").unwrap())
            .text_content
            .get(10_000, String::new()),
        "新しい文"
    );
}

#[test]
fn future_text_redeclarations_keep_old_content_until_their_start_and_disable_static_cache() {
    let timeline = build_output(
        r#"
#0s
title: Text, text: "BEFORE LONG", at: (200, 200)

#10s
title: Text, text: "X" [0s]

#20s
title: Text, text: "OMEGA" [2s]
"#,
    );
    let id = timeline.world.tracks.get_id("title").expect("title actor");
    let text = timeline.world.text_or_default(id);
    let before = text_paths_at(&timeline, "title", 0);
    assert!(
        text.static_paths.is_none(),
        "multiple declarations cannot reuse the last declaration's glyphs"
    );
    assert!(
        text.static_split_clip.is_none(),
        "multiple declarations cannot reuse the last declaration's split clip"
    );
    assert_eq!(text.text_content.get(9_999, String::new()), "BEFORE LONG");
    assert_eq!(text.text_content.get(10_000, String::new()), "X");
    assert_eq!(text.text_content.get(19_999, String::new()), "X");
    assert_eq!(text.text_content.get(20_000, String::new()), "X");
    assert_eq!(text.text_content.get(22_000, String::new()), "OMEGA");
    assert_same_glyph_geometry(&before, &text_paths_at(&timeline, "title", 5_000));
    assert_eq!(text.text_content.get(10_000, String::new()), "X");
    assert_same_glyph_geometry(&before, &text_paths_at(&timeline, "title", 5_000));
}

#[test]
fn nonzero_text_assignment_is_fenced_until_its_declared_start() {
    let timeline = build_output(
        r#"
#0s
title: Text, text: "Alpha", at: (200, 200)

#10s
title.text = "Omega" [2s]
"#,
    );
    let id = timeline.world.tracks.get_id("title").expect("title actor");
    let text = timeline.world.text_or_default(id);
    assert_eq!(text.text_content.get(9_999, String::new()), "Alpha");
    assert_eq!(text.text_content.get(10_000, String::new()), "Alpha");
    assert_eq!(text.text_content.get(12_000, String::new()), "Omega");
    assert!(!text_paths_at(&timeline, "title", 11_000).is_empty());
}

fn assert_program_text_bounds_match_commands(
    timeline: &Timeline,
    label: &str,
    time_s: f64,
) -> Option<kurbo::Rect> {
    let mut filter_backend = None;
    let program = timeline.evaluate_program_with_debug(
        time_s,
        SceneDimensions {
            width: 640,
            height: 360,
        },
        DebugRenderOptions {
            compute_hit_regions: true,
            ..Default::default()
        },
        &mut filter_backend,
    );
    assert!(
        program.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "text frame should not report render/compile errors: {:?}",
        program.diagnostics
    );
    let Some(item) = program.items.iter().find(|item| item.label == label) else {
        assert!(!program.precise_bounds.contains_key(label), "empty text has no precise bounds");
        assert!(!timeline.hit_regions().iter().any(|(name, _)| name == label));
        return None;
    };
    let mut local = None;
    for command in &item.commands {
        if let Some(bounds) = command.local_bounds(None) {
            local = Some(local.map_or(bounds, |existing: kurbo::Rect| existing.union(bounds)));
        }
    }
    let expected = crate::timeline::scene_eval::transform_rect_bbox(&item.transform, local?);
    let actual = program.precise_bounds.get(label).copied().expect("precise bounds");
    assert_eq!(actual, expected, "precise bounds must use the sampled glyph commands");
    let hit = timeline
        .hit_regions()
        .into_iter()
        .find(|(name, _)| name == label)
        .map(|(_, rect)| rect)
        .expect("text hit region");
    assert_eq!(hit, expected, "picking must use the sampled glyph commands");
    Some(actual)
}

#[test]
fn sampled_text_bounds_follow_replacements_typography_and_reverse_seek() {
    let timeline = super::support::build_timeline(
        r#"
#0s
title: Text, text: "WWWWWWWW", font_size: 28, at: (200, 100)
formula: Typst, text: "$x + y$", font_size: 28, at: (200, 180)
math: Math, text: "x^2 + y^2", font_size: 28, at: (420, 180)
code: Code, text: "fn main() {}", font_size: 28, at: (200, 260)
reactive: Text, text: "i", font_size: 28, at: (420, 100)
always { reactive.text = if (t < 1.0) { "i" } else { "MMMMMMMM" } }

#1s
highlight formula [1s]

#10s
title.text = "i" [0s]
title.font_size = 72 [0s]
formula.text = "$x^2 + y^2 + z^2$" [0s]
code.text = "fn much_longer_name() { return 42; }" [0s]

#20s
title.text = "" [0s]

#21s
title.text = "MMMMMMMMMMMM" [0s]
"#,
    );

    let initial_title = assert_program_text_bounds_match_commands(&timeline, "title", 0.0)
        .expect("initial title bounds");
    let initial_formula = assert_program_text_bounds_match_commands(&timeline, "formula", 0.0)
        .expect("initial formula bounds");
    let mut filter_backend = None;
    let initial_program = timeline.evaluate_program_with_debug(
        2.0,
        SceneDimensions {
            width: 640,
            height: 360,
        },
        DebugRenderOptions::default(),
        &mut filter_backend,
    );
    let formula_item = initial_program
        .items
        .iter()
        .find(|item| item.label == "formula")
        .expect("Typst item");
    assert!(
        assert_program_text_bounds_match_commands(&timeline, "formula", 2.0)
            .is_some_and(|bounds| bounds.width() > initial_formula.width()),
        "highlight padding should expand the bounds"
    );
    assert!(
        formula_item.commands.iter().any(|command| matches!(
            command,
            crate::primitives::RenderCommand::HighlightLayer { .. }
        )),
        "Typst highlight should exercise the multi-command bounds path"
    );
    let initial_math = assert_program_text_bounds_match_commands(&timeline, "math", 0.0)
        .expect("initial Math bounds");
    assert!(initial_math.width() > 0.0);
    let initial_code = assert_program_text_bounds_match_commands(&timeline, "code", 0.0)
        .expect("initial code bounds");
    let reactive_short = assert_program_text_bounds_match_commands(&timeline, "reactive", 0.0)
        .expect("reactive text bounds");
    let reactive_long = assert_program_text_bounds_match_commands(&timeline, "reactive", 2.0)
        .expect("reactive text bounds");
    assert!(reactive_long.width() > reactive_short.width(), "always text updates bounds");

    // Check the frame immediately before, at, and after each discrete write,
    // then revisit the past to pin seek direction independence.
    for time_s in [9.999, 10.0, 10.001, 0.0, 10.0, 19.999] {
        assert_program_text_bounds_match_commands(&timeline, "title", time_s);
        assert_program_text_bounds_match_commands(&timeline, "formula", time_s);
        assert_program_text_bounds_match_commands(&timeline, "math", time_s);
        assert_program_text_bounds_match_commands(&timeline, "code", time_s);
    }
    let small_title = assert_program_text_bounds_match_commands(&timeline, "title", 10.0)
        .expect("replacement title bounds");
    assert!(small_title.width() < initial_title.width(), "short replacement shrinks bounds");
    let updated_formula = assert_program_text_bounds_match_commands(&timeline, "formula", 10.0)
        .expect("updated formula bounds");
    assert_ne!(updated_formula, initial_formula, "Typst content changes its bounds");
    let updated_code = assert_program_text_bounds_match_commands(&timeline, "code", 10.0)
        .expect("updated code bounds");
    assert!(updated_code.width() > initial_code.width(), "longer code expands bounds");

    assert!(
        assert_program_text_bounds_match_commands(&timeline, "title", 20.0).is_none(),
        "empty replacement emits no bounds"
    );
    let final_title = assert_program_text_bounds_match_commands(&timeline, "title", 21.0)
        .expect("second replacement bounds");
    assert!(final_title.width() > initial_title.width());
    assert_eq!(
        assert_program_text_bounds_match_commands(&timeline, "title", 0.0),
        Some(initial_title),
        "reverse seek restores original sampled bounds"
    );
}

#[test]
fn counter_always_value_is_sampled_on_forward_and_reverse_frame_evaluation() {
    let timeline = build_output(
        r#"
#0s
count: Counter, value: 0, prefix: "$", suffix: "k", at: (300, 180)
always { count.value = t * 10000 }
"#,
    );
    let (paths_at_zero, clip_at_zero) = counter_paths_from_program(&timeline, 0.0);
    let (paths_at_two, clip_at_two) = counter_paths_from_program(&timeline, 2.0);
    let (paths_at_one, clip_at_one) = counter_paths_from_program(&timeline, 1.0);
    assert!(
        paths_at_two.len() > paths_at_zero.len(),
        "time-driven value should add digit columns"
    );
    assert_eq!(
        paths_at_one.len(),
        paths_at_two.len(),
        "same digit width should render consistently"
    );
    let paths_at_two_again = counter_paths_from_program(&timeline, 2.0);
    assert_eq!(
        paths_at_two_again.0.len(),
        paths_at_two.len(),
        "reverse seek must remain deterministic"
    );
    use kurbo::Shape as _;
    let clip_width = |clip: Option<kurbo::BezPath>| {
        clip.expect("Counter must expose its digit-slot clip").bounding_box().width()
    };
    assert!(clip_width(clip_at_two) > clip_width(clip_at_zero));
    assert_eq!(clip_width(clip_at_one), clip_width(paths_at_two_again.1));
}
