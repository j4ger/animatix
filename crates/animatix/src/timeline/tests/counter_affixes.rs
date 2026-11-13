//! Counter frame-time affix and typography regressions.

use super::support::build_output;
use super::*;
use kurbo::Shape as _;

fn same_glyph_geometry(
    left: &[crate::renderer::types::TextPath],
    right: &[crate::renderer::types::TextPath],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.bounds == right.bounds && left.glyph_center == right.glyph_center
        })
}

fn counter_frame(
    timeline: &Timeline,
    time_s: f64,
) -> (Vec<crate::renderer::types::TextPath>, kurbo::Rect, kurbo::Rect) {
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
        paths,
        clip_path: Some(clip_path),
        ..
    } = &item.commands[0]
    else {
        panic!("Counter should emit text paths with a slot clip");
    };
    (
        paths.to_vec(),
        clip_path.bounding_box(),
        timeline.precise_bounds_for("count").expect("Counter bounds"),
    )
}

#[test]
fn counter_rebuilds_affixes_and_glyph_metrics_for_keyframes_and_always_overrides() {
    let timeline = build_output(
        r#"
#0s
count: Counter, value: 1234, prefix: "$", suffix: "k", font_size: 32, at: (300, 180)

#1s
count.prefix = "USD " [0s]
count.suffix = " million" [0s]
count.font_size = 64 [0s]
count.decimals = 2 [0s]
count.comma = true [0s]

#2s
count.prefix = "$" [0s]
count.suffix = "k" [0s]
count.font_size = 32 [0s]
count.decimals = 0 [0s]
count.comma = false [0s]
"#,
    );
    let (initial_paths, initial_clip, initial_bounds) = counter_frame(&timeline, 0.0);
    let (updated_paths, updated_clip, updated_bounds) = counter_frame(&timeline, 1.0);
    let (restored_paths, restored_clip, restored_bounds) = counter_frame(&timeline, 2.0);
    let (reverse_paths, reverse_clip, reverse_bounds) = counter_frame(&timeline, 1.0);
    assert!(
        updated_paths.len() > initial_paths.len(),
        "affixes and number formatting update"
    );
    assert!(
        updated_clip.width() > initial_clip.width(),
        "clip includes changed affixes and digits"
    );
    assert!(updated_clip.height() > initial_clip.height(), "font size updates slot bounds");
    assert!(
        updated_bounds.width() > initial_bounds.width(),
        "precise bounds follow glyph width"
    );
    let glyph_height = |paths: &[crate::renderer::types::TextPath]| {
        paths
            .iter()
            .map(|text_path| text_path.path.bounding_box().height())
            .fold(0.0f64, f64::max)
    };
    assert!(glyph_height(&updated_paths) > glyph_height(&initial_paths));
    assert_eq!(restored_paths.len(), initial_paths.len());
    assert_eq!(restored_clip, initial_clip);
    assert_eq!(restored_bounds, initial_bounds);
    assert_eq!(reverse_paths.len(), updated_paths.len());
    assert_eq!(reverse_clip, updated_clip);
    assert_eq!(reverse_bounds, updated_bounds);

    let always_timeline = build_output(
        r#"
count: Counter, value: 12, prefix: "$", suffix: "k", at: (300, 180)
always {
  count.prefix = if (t < 1.0) { "$" } else { "USD " }
  count.suffix = if (t < 1.0) { "k" } else { " million" }
}
"#,
    );
    let (before_override, before_clip, before_bounds) = counter_frame(&always_timeline, 0.0);
    let (during_override, during_clip, during_bounds) = counter_frame(&always_timeline, 1.5);
    let (override_again, override_clip_again, override_bounds_again) =
        counter_frame(&always_timeline, 1.5);
    let cloned_timeline = always_timeline.clone();
    let (cloned_paths, cloned_clip, cloned_bounds) = counter_frame(&cloned_timeline, 1.5);
    assert!(during_override.len() > before_override.len());
    assert!(during_clip.width() > before_clip.width());
    assert!(during_bounds.width() > before_bounds.width());
    assert_eq!(override_again.len(), during_override.len());
    assert_eq!(override_clip_again, during_clip);
    assert_eq!(override_bounds_again, during_bounds);
    assert_eq!(cloned_paths.len(), during_override.len());
    assert_eq!(cloned_clip, during_clip);
    assert_eq!(cloned_bounds, during_bounds);
}

#[test]
fn counter_same_length_affixes_rebuild_geometry_and_round_trip_on_reverse_seek() {
    let timeline = build_output(
        r#"
#0s
count: Counter, value: 12, prefix: "II", suffix: "xx", at: (300, 180)
#1s
count.prefix = "WW" [0s]
count.suffix = "OO" [0s]
"#,
    );
    let (before, before_clip, _) = counter_frame(&timeline, 0.0);
    let (after, after_clip, _) = counter_frame(&timeline, 1.0);
    let (reverse, reverse_clip, _) = counter_frame(&timeline, 0.0);
    assert_eq!(before.len(), after.len(), "affixes contain the same glyph count");
    assert!(
        before.iter().zip(&after).any(|(before, after)| before.bounds != after.bounds),
        "different affix glyph geometry must be compiled"
    );
    assert_ne!(before_clip.width(), after_clip.width(), "advance widths update the clip");
    assert!(same_glyph_geometry(&reverse, &before), "reverse seek restores original paths");
    assert_eq!(reverse_clip, before_clip);
}

#[test]
fn counter_always_font_overrides_match_a_static_font_sample() {
    let animated = build_output(
        r#"
count: Counter, value: 123, prefix: "$", suffix: "k", font_family: "Open Sans", font_size: 32, at: (300, 180)
always {
  count.font_family = if (t < 1.0) { "Open Sans" } else { "serif" }
  count.font_size = if (t < 1.0) { 32 } else { 64 }
}
"#,
    );
    let reference = build_output(
        r#"
count: Counter, value: 123, prefix: "$", suffix: "k", font_family: "serif", font_size: 64, at: (300, 180)
"#,
    );
    let (animated_paths, animated_clip, _) = counter_frame(&animated, 1.5);
    let (reference_paths, reference_clip, _) = counter_frame(&reference, 0.0);
    assert!(same_glyph_geometry(&animated_paths, &reference_paths));
    assert_eq!(animated_clip, reference_clip);
}

#[test]
fn counter_carries_updated_affix_and_prebaked_glyphs_between_scenes() {
    let mut source = build_output(
        r#"
#0s
count: Counter, value: 12, prefix: "$", suffix: "k", at: (300, 180)
#1s
count.prefix = "USD " [0s]
count.suffix = " million" [0s]
count.font_size = 64 [0s]
"#,
    );
    source.set_persistence_flag("count", true);
    let carry = source.compute_carry_bag(1_000, true);
    let entry = carry.entries.get("count").expect("persistent Counter carried");
    assert!(
        entry.text.counter_data.is_some(),
        "cached Counter glyphs survive in-memory carry"
    );
    let mut next_scene = Timeline::new();
    next_scene.inject_carry_bag(&carry, &source, 0, [640.0, 360.0], &mut Vec::new());
    let (paths, clip, bounds) = counter_frame(&next_scene, 0.0);
    assert!(paths.len() > 2, "carried affixes and number render in the next scene");
    assert!(clip.width() > 2.0 * clip.height(), "carried clip includes the long affixes");
    assert!(clip.height() > 60.0, "carried typography updates the digit slot height");
    assert!(bounds.width() > clip.height(), "carried precise bounds use dynamic glyphs");
}

#[test]
fn omitted_counter_defaults_hold_until_the_first_future_assignment() {
    let animated = build_output(
        "#0s\ncount: Counter\n#1s\ncount.value = 1234 [0s]\n\
         count.prefix = \"USD \" [0s]\ncount.suffix = \" units\" [0s]",
    );
    let reference = build_output("#0s\ncount: Counter, value: 0");
    let (expected_paths, expected_clip, _) = counter_frame(&reference, 0.0);
    for time in [0.0, 0.999, 0.0] {
        let (paths, clip, _) = counter_frame(&animated, time);
        assert!(same_glyph_geometry(&paths, &expected_paths));
        assert_eq!(clip, expected_clip);
    }
    let (paths, clip, _) = counter_frame(&animated, 1.0);
    assert!(paths.len() > expected_paths.len());
    assert!(clip.width() > expected_clip.width());
}

#[test]
fn counter_symbol_and_digit_paths_occupy_ordered_nonoverlapping_columns() {
    let timeline = build_output(
        r#"
count: Counter, value: 1234567, prefix: "USD ", suffix: "k", comma: true, at: (300, 180)
"#,
    );
    let (paths, _, _) = counter_frame(&timeline, 0.0);
    let id = timeline.world.tracks.get_id("count").expect("Counter entity");
    let data = timeline.world.text_or_default(id).counter_data.as_ref().expect("Counter data");

    let prefix_end = data.prefix.0.len();
    let digit_count = |digit: usize| data.digits[digit].len();
    let comma_count = data.comma.0.len();
    let mut cursor = prefix_end;
    let digit1 = &paths[cursor..cursor + digit_count(1)];
    cursor += digit_count(1);
    let comma1 = &paths[cursor..cursor + comma_count];
    cursor += comma_count;
    let digit2 = &paths[cursor..cursor + digit_count(2)];
    cursor += digit_count(2);
    let digit3 = &paths[cursor..cursor + digit_count(3)];
    cursor += digit_count(3);
    let digit4 = &paths[cursor..cursor + digit_count(4)];
    cursor += digit_count(4);
    let comma2 = &paths[cursor..cursor + comma_count];
    cursor += comma_count;
    let digit5 = &paths[cursor..cursor + digit_count(5)];
    cursor += digit_count(5);
    let digit6 = &paths[cursor..cursor + digit_count(6)];
    cursor += digit_count(6);
    let digit7 = &paths[cursor..cursor + digit_count(7)];
    cursor += digit_count(7);
    let suffix = &paths[cursor..];

    let bounds = |group: &[crate::renderer::types::TextPath]| {
        group
            .iter()
            .map(|glyph| glyph.path.bounding_box())
            .reduce(|left, right| left.union(right))
            .expect("non-empty glyph group")
    };
    let prefix_bounds = bounds(&paths[..prefix_end]);
    let digit1_bounds = bounds(digit1);
    let comma1_bounds = bounds(comma1);
    let digit2_bounds = bounds(digit2);
    let digit3_bounds = bounds(digit3);
    let digit4_bounds = bounds(digit4);
    let comma2_bounds = bounds(comma2);
    let digit5_bounds = bounds(digit5);
    let digit6_bounds = bounds(digit6);
    let digit7_bounds = bounds(digit7);
    let suffix_bounds = bounds(suffix);

    assert!(prefix_bounds.x1 <= digit1_bounds.x0 + 1.0, "prefix overlaps the first digit");
    assert!(
        digit1_bounds.x1 <= comma1_bounds.x0 + 1.0,
        "first comma follows the first digit"
    );
    assert!(
        comma1_bounds.x1 <= digit2_bounds.x0 + 1.0,
        "comma separates first and second digits"
    );
    assert!(digit2_bounds.x1 <= digit3_bounds.x0 + 1.0, "second digit precedes third digit");
    assert!(digit3_bounds.x1 <= digit4_bounds.x0 + 1.0, "third digit precedes fourth digit");
    assert!(
        digit4_bounds.x1 <= comma2_bounds.x0 + 1.0,
        "second comma follows the fourth digit"
    );
    assert!(
        comma2_bounds.x1 <= digit5_bounds.x0 + 1.0,
        "second comma separates digit groups"
    );
    assert!(digit5_bounds.x1 <= digit6_bounds.x0 + 1.0, "fifth digit precedes sixth digit");
    assert!(digit6_bounds.x1 <= digit7_bounds.x0 + 1.0, "sixth digit precedes seventh digit");
    assert!(digit7_bounds.x1 <= suffix_bounds.x0 + 1.0, "suffix follows the last digit");
    assert!(
        paths.iter().all(|glyph| glyph.bounds == glyph.path.bounding_box()),
        "translated glyph bounds must track actual path geometry"
    );
}
