//! Shape memo bounds must describe the sampled geometry, not an earlier frame.
use super::*;

fn assert_sampled_bounds(timeline: &Timeline, time: f64) {
    let program = timeline.evaluate_program_with_debug(
        time,
        SceneDimensions {
            width: 400,
            height: 200,
        },
        DebugRenderOptions {
            compute_hit_regions: true,
            ..Default::default()
        },
        &mut None,
    );
    let item = program.items.iter().find(|item| item.label == "box").expect("shape");
    let local = item
        .commands
        .iter()
        .filter_map(|c| c.local_bounds(None))
        .reduce(|a, b| a.union(b))
        .expect("command bounds");
    let expected = item.transform.transform_rect_bbox(local);
    assert_eq!(program.precise_bounds["box"], expected, "time={time}");
    let hit = timeline.hit_regions().into_iter().find(|(name, _)| name == "box").unwrap().1;
    assert_eq!(hit, expected, "picking at time={time}");
}

#[test]
fn shape_bounds_follow_geometry_and_style_across_seek_orders() {
    // Dash/gradient paths build commands directly rather than taking the memo.
    // An occupied slot must not prevent them from publishing current bounds.
    for shape in ["Rect", "Ellipse"] {
        for extra in [
            "",
            ", dash_pattern: {8, 4}",
            ", fill_gradient: linear(90, {\"#ff0000\", \"#0000ff\"})",
        ] {
            for order in [
                [0., 0.5, 1., 0.5, 0., 1., 1.],
                [1., 0.5, 0., 1., 0.5, 0.5, 1.],
            ] {
                let timeline = super::support::build_timeline(&format!(
                    r##"
config {{ resolution: (400,200), duration: 3 }}
box: {shape}, size: (60,40), at: (200,100), opacity: 1, stroke: "#ffffff", stroke_width: 2{extra}
always {{
 box.size = (60 + 180 * clamp(t,0,1),40)
 box.stroke_width = 2 + 4 * clamp(t,0,1)
}}
"##
                ));
                for time in order {
                    assert_sampled_bounds(&timeline, time);
                }
            }
        }
    }
}

#[test]
fn shape_bounds_follow_size_tween_and_static_cache_hits() {
    for animation in ["", "#0s\nbox.size = (240,40) [1s]"] {
        let timeline = super::support::build_timeline(&format!(
            r##"
config {{ resolution: (400,200), duration: 3 }}
box: Rect, size: (60,40), at: (200,100), opacity: 1
{animation}
"##
        ));
        for time in [0., 0.5, 1., 0.5, 0., 1., 1.] {
            assert_sampled_bounds(&timeline, time);
        }
    }
}

#[test]
fn shape_bounds_include_final_trim_and_shadow_without_polluting_memo() {
    for actor in [
        "Rect, size: (100,100)",
        "Ellipse, size: (100,100)",
        "Path, commands: {move_to(0,0), line_to(200,0)}",
    ] {
        for decoration in [
            "stroke_progress: 0.25",
            "shadow: (30,8,16,2)",
            "stroke_progress: 0.25, shadow: (30,8,16,2)",
        ] {
            let timeline = super::support::build_timeline(&format!(
                r##"
config {{ resolution: (400,200), duration: 3 }}
box: {actor}, at: (200,100), opacity: 1, fill_opacity: 0,
 stroke: "#ffffff", stroke_width: 6, {decoration}
always {{ box.at = (200 + t,100) }}
"##
            ));
            // Different times prevent the whole-frame cache from hiding a
            // shape-memo hit; then revisit and repeat frames as a scrubber does.
            for time in [0., 0.5, 1., 0.5, 0., 1., 1.] {
                assert_sampled_bounds(&timeline, time);
                let program = timeline.evaluate_program_with_debug(
                    time,
                    SceneDimensions {
                        width: 400,
                        height: 200,
                    },
                    DebugRenderOptions::default(),
                    &mut None,
                );
                let commands = &program.items.iter().find(|i| i.label == "box").unwrap().commands;
                let shadows = commands
                    .iter()
                    .filter(|c| matches!(c, crate::primitives::RenderCommand::Shadow { .. }))
                    .count();
                assert_eq!(
                    shadows,
                    usize::from(decoration.contains("shadow:")),
                    "{actor} {decoration} at {time}"
                );
            }
        }
    }
}

#[test]
fn trimmed_path_bounds_cover_only_the_revealed_segment() {
    let timeline = super::support::build_timeline(
        r##"
config { resolution:(400,200), duration:3 }
box: Path, commands:{move_to(0,0), line_to(200,0)}, at:(100,100),
 fill_opacity:0, stroke:"#ffffff", stroke_width:6, opacity:1
always { box.stroke_progress = t / 2 }
"##,
    );
    for time in [0.5, 1., 1.5, 0.5, 0.5] {
        assert_sampled_bounds(&timeline, time);
        let program = timeline.evaluate_program_with_debug(
            time,
            SceneDimensions {
                width: 400,
                height: 200,
            },
            DebugRenderOptions::default(),
            &mut None,
        );
        let bounds = program.precise_bounds["box"];
        assert!(
            (bounds.width() - 100.0 * time).abs() < 0.01,
            "trim must change the envelope: {bounds:?}"
        );
    }
}

#[test]
fn animated_shadow_can_toggle_without_leaking_decorations_into_memo() {
    let timeline = super::support::build_timeline(
        r#"
config { resolution:(400,200), duration:3 }
box: Rect, size:(100,60), at:(200,100), opacity:1, shadow:(0,0,0,0)
#0s
box.shadow = (20,8,12,2) [1s]
"#,
    );
    for time in [0., 0.5, 1., 0., 1., 0.5, 0.5, 0.] {
        assert_sampled_bounds(&timeline, time);
        let program = timeline.evaluate_program_with_debug(
            time,
            SceneDimensions {
                width: 400,
                height: 200,
            },
            DebugRenderOptions::default(),
            &mut None,
        );
        let commands = &program.items.iter().find(|i| i.label == "box").unwrap().commands;
        assert_eq!(
            commands
                .iter()
                .filter(|c| matches!(c, crate::primitives::RenderCommand::Shadow { .. }))
                .count(),
            usize::from(time > 0.)
        );
    }
}

#[test]
fn reactive_dashes_use_sampled_pattern_and_offset() {
    let timeline = super::support::build_timeline(
        r##"
config { resolution:(400,200), duration:3 }
box: Rect, size:(100,60), at:(200,100), opacity:1, stroke:"#ffffff", stroke_width:4
always {
  box.dash_pattern = {4 + t, 3}
  box.dash_offset = 10 * t
}
"##,
    );
    for time in [0., 0.5, 1., 0.5, 0., 1.] {
        let program = timeline.evaluate_program_with_debug(
            time,
            SceneDimensions {
                width: 400,
                height: 200,
            },
            DebugRenderOptions::default(),
            &mut None,
        );
        let item = program.items.iter().find(|item| item.label == "box").unwrap();
        let Some(crate::primitives::RenderCommand::Paths { paths }) = item.commands.first() else {
            panic!("expected vector paths at {time}s");
        };
        assert_eq!(paths[0].dash_pattern.as_deref(), Some([4. + time as f32, 3.].as_slice()));
        assert_eq!(paths[0].dash_offset, 10. * time as f32);
    }
}
