//! Preserve existing delayed-step and duration-bearing assignment semantics.
use super::*;

#[test]
fn delayed_compound_assignments_hold_until_delay_expires() {
    for suffix in [" [delay:2s]", " [0s, delay:2s]"] {
        let timeline = build_timeline(&format!(
            "r: Rect, at:(20,30), size:(100,80), opacity:1\n\
             #0s\nr.at=(220,130){suffix}\nr.size=(200,160){suffix}\n"
        ));
        let id = timeline.world.get_id("r").unwrap();
        let geometry = timeline.world.geometry(id);
        for time in [0, 500, 1000, 1999, 2000, 2500, 0] {
            let before = time < 2000;
            assert_eq!(
                geometry.position.get(time, [0.0; 2]),
                if before { [20.0, 30.0] } else { [220.0, 130.0] },
            );
            assert_eq!(
                geometry.size.get(time, DEFAULT_LAYOUT_HALF_SIZE),
                if before { [50.0, 40.0] } else { [100.0, 80.0] },
            );
            assert_eq!(
                geometry.layout_size_get(time),
                Some(if before { [50.0, 40.0] } else { [100.0, 80.0] }),
            );
        }
    }
}

#[test]
fn timed_compound_assignments_still_interpolate() {
    let timeline = build_timeline(
        "r: Rect, at:(20,30), size:(100,80), opacity:1\n\
         #2s\nr.at=(220,130) [2s]\nr.size=(200,160) [2s]\n",
    );
    let id = timeline.world.get_id("r").unwrap();
    let geometry = timeline.world.geometry(id);
    assert_eq!(geometry.position.get(1000, [0.0; 2]), [20.0, 30.0]);
    assert_eq!(geometry.position.get(3000, [0.0; 2]), [120.0, 80.0]);
    assert_eq!(geometry.size.get(3000, DEFAULT_LAYOUT_HALF_SIZE), [75.0, 60.0]);
    assert_eq!(geometry.position.get(4000, [0.0; 2]), [220.0, 130.0]);
}

#[test]
fn delayed_size_assignment_preserves_undeclared_size_default() {
    let timeline = build_timeline(
        "r: Rect, at:(20,30), opacity:1\n#0s\nr.size=(200,160) [delay:2s]\n",
    );
    let geometry = timeline.world.geometry(timeline.world.get_id("r").unwrap());
    for time in [0, 1000, 1999, 2000, 2500, 0] {
        let expected = if time < 2000 {
            DEFAULT_LAYOUT_HALF_SIZE
        } else {
            [100.0, 80.0]
        };
        assert_eq!(geometry.size.get(time, DEFAULT_LAYOUT_HALF_SIZE), expected);
        assert_eq!(geometry.layout_size_get(time), Some(expected));
    }
}

#[test]
fn delayed_background_assignment_holds_until_delay_expires() {
    for suffix in [" [delay:2s]", " [0s, delay:2s]"] {
        let timeline = build_timeline(&format!(
            "scene.background_color=(0,0,0,1)\n\
             #0s\nscene.background_color=(1,0,0,1){suffix}\n"
        ));
        for time in [0, 1000, 1999, 2000, 2500, 0] {
            assert_eq!(
                timeline.background_color.evaluate(time),
                if time < 2000 {
                    [0.0, 0.0, 0.0, 1.0]
                } else {
                    [1.0, 0.0, 0.0, 1.0]
                },
            );
        }
    }
}

fn text_bounds(timeline: &Timeline, time_s: f64) -> kurbo::Rect {
    let mut backend = None;
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
        &mut backend,
    );
    assert!(program.diagnostics.iter().all(|d| !d.is_error()));
    *program.precise_bounds.get("title").expect("text bounds")
}

#[test]
fn instant_text_assignment_preserves_pre_step_bounds_and_sizes() {
    // Compare against a separately built no-assignment control. A self-baseline
    // sampled from the mutated timeline could hide a future write leaking to t=0.
    let source = "title: Text, text:\"BEFORE LONG\", at:(200,200), opacity:1\n";
    let control = build_timeline(source);
    let timeline = build_timeline(&format!("{source}#2s\ntitle.text=\"X\" [0s]\n"));
    let geometry = timeline.world.geometry(timeline.world.get_id("title").unwrap());
    let original = control.world.geometry(control.world.get_id("title").unwrap());
    let expected_bounds = text_bounds(&control, 0.0);
    let changed_bounds = text_bounds(&timeline, 2.0);
    assert_ne!(changed_bounds, expected_bounds, "fixture must change the text bounds");
    for time in [0, 1000, 1999, 0] {
        assert_eq!(
            geometry.size.get(time, DEFAULT_LAYOUT_HALF_SIZE),
            original.size.get(time, DEFAULT_LAYOUT_HALF_SIZE),
        );
        assert_eq!(geometry.layout_size_get(time), original.layout_size_get(time));
        assert_eq!(text_bounds(&timeline, time as f64 / 1000.0), expected_bounds);
    }
}
