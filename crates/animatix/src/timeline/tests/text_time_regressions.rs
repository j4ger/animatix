//! Regressions for discrete text keyframes and Counter frame overrides.

use super::support::build_output;
use super::*;
use kurbo::{PathEl, Point};

#[test]
fn directed_wipes_and_ordered_exits_use_the_shaped_layout() {
    use kurbo::Shape;

    let timeline = support::build_timeline(
        r#"
#0s
wiped: Text, text: "Title", font_size: 36
reversed: Text, text: "One Two Three", font_size: 36
staggered: Text, text: r"UPPER
LOWER", font_size: 36
#1s
  wipe_in(target: wiped, direction: top, duration: 1s)
  draw_out(target: reversed, by: word, order: forward, duration: 1s)
  reveal_out(target: staggered, by: line, order: reverse, stagger: 250ms, offset_y: 16, mask: baseline, duration: 1s, ease: linear)
#3s
  wipe_out(target: wiped, direction: right, duration: 1s)
"#,
    );
    let id = timeline.world.get_id("wiped").unwrap();
    let text = timeline.world.text_or_default(id);
    let style = timeline.world.style(id);
    let resting = text.evaluate_paths(style, 2000);
    let ink = resting.iter().map(|glyph| glyph.bounds).reduce(|a, b| a.union(b)).unwrap();
    let clip = |time| text.evaluate_paths_and_clip(style, time).1.unwrap().bounding_box();
    assert!((clip(1500).y1 - ink.center().y).abs() < 0.001);
    assert!((clip(3500).x0 - ink.center().x).abs() < 0.001);
    let words = text_paths_at(&timeline, "reversed", 1500);
    assert!(words.iter().any(|g| g.word_idx == 1) && words.iter().any(|g| g.word_idx == 2));
    assert!(words.iter().all(|g| g.word_idx >= 1));
    let early = text_paths_at(&timeline, "staggered", 1200);
    let full = text_paths_at(&timeline, "staggered", 999);
    assert!(
        early
            .iter()
            .filter(|g| g.line_idx == 0)
            .zip(full.iter())
            .all(|(sampled, resting)| sampled.path == resting.path)
    );
    assert!(early.iter().any(|g| g.line_idx == 1 && !g.clips.is_empty()));
    assert!(text_paths_at(&timeline, "staggered", 2000).is_empty());
}

#[test]
fn draw_in_reveals_shaped_words_and_wrapped_lines_as_whole_groups() {
    let timeline = build_output(
        r#"
#0s
words: Text, text: "a     bb ccc", font_size: 36
wrapped: Text, text: "Alpha Bravo Charlie", font_size: 36, text_max_width: 120
#1s
draw_in(target: words, by: word, duration: 900ms)
draw_in(target: wrapped, by: line, duration: 900ms)
"#,
    );
    let authored = text_paths_at(&timeline, "words", 1900);
    assert_eq!(authored.iter().map(|g| g.word_idx).max(), Some(2));
    for (time, groups) in [(1000, 0), (1299, 1), (1301, 2), (1599, 2), (1601, 3)] {
        let current = text_paths_at(&timeline, "words", time);
        let expected = authored.iter().filter(|glyph| glyph.word_idx < groups);
        assert!(current.iter().map(|glyph| &glyph.path).eq(expected.map(|glyph| &glyph.path)));
        assert!(current.iter().all(|glyph| glyph.opacity == 1.0));
    }
    let wrapped = text_paths_at(&timeline, "wrapped", 1900);
    assert_eq!(wrapped.iter().map(|g| g.line_idx).max(), Some(2));
    let early = text_paths_at(&timeline, "wrapped", 1299);
    assert!(!early.is_empty());
    assert!(early.iter().all(|glyph| glyph.line_idx == 0 && glyph.opacity == 1.0));
}

#[test]
fn split_stagger_uses_elapsed_time_and_masks_each_visual_line() {
    use kurbo::Shape;

    let timeline = build_output(
        r#"
#0s
timed: Text, text: "A B C", font_size: 36
springy: Text, text: "A B", font_size: 36
masked: Text, text: r"HELLO

gjpqy", font_size: 36
#1s
reveal_in(target: timed, by: word, stagger: 200ms, ease: ease_out, duration: 1s)
reveal_in(target: springy, by: word, stagger: 650ms, ease: spring, duration: 1s)
reveal_in(target: masked, by: line, stagger: 200ms, offset_y: 32, mask: baseline, duration: 1s)
"#,
    );
    for (time, words) in [(1199, 1), (1200, 1), (1201, 2), (1401, 3)] {
        let sampled = text_paths_at(&timeline, "timed", time);
        assert_eq!(sampled.len(), words);
    }
    assert_eq!(text_paths_at(&timeline, "springy", 1500).len(), 1);
    assert_eq!(text_paths_at(&timeline, "springy", 1649).len(), 1);
    assert_eq!(text_paths_at(&timeline, "springy", 1700).len(), 2);
    let resting = text_paths_at(&timeline, "masked", 2000);
    assert_eq!(resting.iter().map(|g| g.line_idx).max(), Some(2));
    let sampled = text_paths_at(&timeline, "masked", 1600);
    for line in [0, 2] {
        let upper_bound = resting
            .iter()
            .filter(|g| g.line_idx == line)
            .map(|g| g.bounds.y1)
            .reduce(f64::max)
            .unwrap();
        let partial: Vec<_> = sampled.iter().filter(|g| g.line_idx == line).collect();
        assert!(!partial.is_empty(), "line {line} must be entering");
        assert!(partial.iter().any(|g| !g.clips.is_empty()));
        for glyph in partial {
            assert!(glyph.bounds.y1 <= upper_bound + 0.001);
            assert!(glyph.clips.iter().all(|clip| clip.bounding_box().y1 <= upper_bound + 0.001));
        }
    }
    assert!(resting.iter().all(|g| g.clips.is_empty()));
}

#[cfg(feature = "rich-text")]
#[test]
fn rich_text_word_reveal_separates_the_last_word_from_the_next_line() {
    let timeline = build_output(
        r#"
#0s
rich: Typst, text: r"A \
B", font_size: 36
code: Code, text: r"foo
bar", font_size: 36
draw_in(target: rich, by: word, duration: 1s)
draw_in(target: code, by: word, duration: 1s)
"#,
    );
    for label in ["rich", "code"] {
        let early = text_paths_at(&timeline, label, 250);
        let late = text_paths_at(&timeline, label, 1000);
        assert!(!early.is_empty(), "{label}");
        assert!(early.iter().all(|glyph| glyph.line_idx == 0), "{label}");
        assert!(late.iter().any(|glyph| glyph.line_idx > 0), "{label}");
    }
}

#[test]
fn text_reveal_modes_hold_between_actions_and_support_delayed_instant_exit() {
    let timeline = build_output(
        r#"
#0s
title: Text, text: "One two", opacity: 0.6, font_size: 36
#1s
wipe_in(target: title, duration: 1s)
#3s
draw_out(target: title, duration: 1s)
#5s
reveal_in(target: title, by: word, duration: 1s)
#7s
wipe_out(target: title, duration: 0ms, delay: 200ms)
"#,
    );
    let id = timeline.world.get_id("title").unwrap();
    let text = timeline.world.text_or_default(id);
    let style = timeline.world.style(id);
    let sample = |time| text.evaluate_paths_and_clip(style, time);
    let early_clip = sample(1500).1.expect("wipe clip");
    let full = sample(2000).0.len();
    assert!(full > 2);
    for time in [0, 999, 1000, 4000, 4999, 5000, 7200, 8000] {
        assert!(sample(time).0.is_empty(), "{time}ms");
    }
    for time in [2000, 2999, 3000, 6000, 7199] {
        assert_eq!(sample(time).0.len(), full, "{time}ms");
        assert!(sample(time).1.is_none(), "{time}ms");
    }
    let (outgoing, clip) = sample(3500);
    assert!(outgoing.len() < full && clip.is_none());
    assert!((style.opacity.get(1500, 1.0) - 0.6).abs() < f32::EPSILON);
    assert_eq!(sample(1500).1, Some(early_clip));
}

fn text_paths_at(
    timeline: &Timeline,
    label: &str,
    time_ms: u64,
) -> Vec<crate::renderer::types::TextPath> {
    let id = timeline.world.tracks.get_id(label).expect("text actor");
    let text = timeline.world.text_or_default(id);
    text.evaluate_paths(timeline.world.style(id), time_ms)
}

#[test]
fn text_fit_counts_empty_lines_and_uses_unclipped_ink() {
    let (timeline, diagnostics) = support::build_ignoring_warnings(
        r#"
#0s
heading: Text, text: r"A wide title

continued", font_size: 48, fit_min_size: 12,
  fit_max_height: 90, text_max_width: 240, fit_max_lines: 3
token: Text, text: "WWWWWWWWWWWWWW", font_size: 30, fit_min_size: 24,
  text_max_width: 30, overflow: "clip"
"#,
    );
    assert!(diagnostics.iter().all(|diag| !diag.is_error()), "{diagnostics:?}");
    assert!(diagnostics.iter().any(|diag| diag.message.contains("exceeds its fit bounds")));
    let heading = timeline.world.get_id("heading").unwrap();
    let size = timeline.world.text_or_default(heading).font_size.get(0, 0.0);
    assert!((12.0..32.0).contains(&size), "fit must reserve a blank line, got {size}");
    let paths = text_paths_at(&timeline, "heading", 0);
    assert_eq!(paths.iter().map(|p| p.line_idx).max(), Some(2));
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
fn final_text_commands_preserve_rich_color_clip_and_sampled_typography() {
    let timeline = build_output(
        r##"
config { duration: 4 }
#0s
plain: Text, text: "MMMMM", font_size: 30, at: (300, 80), backdrop: true
rich: Typst, text: "#text(fill: red)[MMMMM]", font_size: 30, at: (300, 150)
source: Code, text: "let x = 42;", language: "rust", at: (300, 200)
clipped: Typst, text: "#box(width: 240pt, height: 20pt, fill: red)", text_max_width: 100, overflow: "clip", at: (300, 250)
equation: $$ x^2 + 3 $$
always {
    rich.font_size = 70
}
#1s
plain.font_size = 70 [1s]
"##,
    );
    let evaluate = |time| {
        timeline.evaluate_program_with_debug(
            time,
            SceneDimensions {
                width: 640,
                height: 360,
            },
            DebugRenderOptions::default(),
            &mut None,
        )
    };
    let path_data = |program: &crate::timeline::scene_program::SceneProgram, label| {
        let item = program.items.iter().find(|item| item.label == label).unwrap();
        item.commands
            .iter()
            .find_map(|command| match command {
                crate::primitives::RenderCommand::Text {
                    paths, tint_color, ..
                } => {
                    Some((paths.clone(), *tint_color, command.local_bounds(None).unwrap().width()))
                },
                _ => None,
            })
            .unwrap()
    };
    let start = evaluate(0.0);
    let middle = evaluate(1.5);
    let end = evaluate(3.0);
    let back = evaluate(0.0);
    let width = path_data(&start, "plain").2;
    assert_eq!(path_data(&back, "plain").2, width);
    assert!(path_data(&middle, "plain").2 > width * 1.5);
    assert!(path_data(&end, "plain").2 > width * 2.0);
    let (paths, tint, width) = path_data(&start, "rich");
    assert!(width > 250.0 && tint.is_none());
    assert!(paths.iter().all(|p| p.color == [255, 65, 54, 255]));
    let (paths, tint, _) = path_data(&start, "source");
    assert!(tint.is_none() && paths.iter().any(|p| p.color != paths[0].color));
    assert!(paths.iter().any(|p| p.color == [255, 255, 255, 255]));
    let (paths, _, width) = path_data(&start, "clipped");
    assert!(width <= 100.01 && paths.iter().any(|p| !p.clips.is_empty()));
    assert_eq!(path_data(&start, "equation").0.len(), 4);
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
highlight(target: formula, duration: 1s)

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
