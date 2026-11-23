//! Observable output contracts for performance fixtures; no timing assertions.
#[path = "../benches/layout_invalidation_workload.rs"]
mod workload;

use animatix::primitives::RenderCommand;
use animatix::timeline::{DebugRenderOptions, Timeline};
use workload::{COUNTS, DIMENSIONS, GROUP, Mode, SAMPLE_COUNT, SampleClock, build, source};

#[derive(Clone, Debug)]
struct RectOutput {
    bounds: kurbo::Rect,
    rgba: [u8; 4],
}

fn outputs(timeline: &Timeline, count: usize, time: f64) -> Vec<RectOutput> {
    let frame = timeline.evaluate_program_with_debug(
        time,
        DIMENSIONS,
        DebugRenderOptions::default(),
        &mut None,
    );
    assert!(frame.diagnostics.is_empty(), "layout frame at {time}: {:?}", frame.diagnostics);
    let mut seen = vec![None; count];
    for item in frame.items {
        if item.label == "row" {
            assert!(item.commands.is_empty());
            continue;
        }
        let index: usize = item.label.strip_prefix('r').expect("Rect label").parse().unwrap();
        assert!(index < count && seen[index].is_none(), "unexpected/duplicate Rect");
        assert_eq!(item.opacity, 1.0);
        assert_eq!(item.commands.len(), 1, "one Rect path command");
        let command = &item.commands[0];
        let RenderCommand::Paths { paths } = command else {
            panic!("expected Rect paths")
        };
        assert_eq!(paths.len(), 1);
        assert!(paths[0].stroke.is_none() && paths[0].fill_gradient.is_none());
        let fill = paths[0].fill.expect("opaque Rect fill").to_rgba8();
        let local = command.local_bounds(None).expect("rendered Rect local bounds");
        seen[index] = Some(RectOutput {
            bounds: item.transform.transform_rect_bbox(local),
            rgba: [fill.r, fill.g, fill.b, fill.a],
        });
    }
    seen.into_iter()
        .map(|rect| rect.expect("every Rect appears in this same frame"))
        .collect()
}

fn near(actual: f64, expected: f64) {
    near_with(actual, expected, 0.01);
}

fn near_with(actual: f64, expected: f64, tolerance: f64) {
    assert!((actual - expected).abs() < tolerance, "{actual} != {expected} ({tolerance}px)");
}

fn same_rect(a: &RectOutput, b: &RectOutput) {
    for (actual, expected) in [a.bounds.x0, a.bounds.y0, a.bounds.x1, a.bounds.y1]
        .into_iter()
        .zip([b.bounds.x0, b.bounds.y0, b.bounds.x1, b.bounds.y1])
    {
        near(actual, expected);
    }
    assert_eq!(a.rgba, b.rgba);
}

#[test]
fn row_geometry_and_fill_change_in_the_same_frame_and_reset_on_seek() {
    for count in COUNTS {
        for mode in [Mode::Static, Mode::Color, Mode::Size] {
            let timeline = build(count, mode);
            let initial = outputs(&timeline, count, 0.0);
            for (i, rect) in initial.iter().enumerate() {
                near(rect.bounds.width(), 12.0);
                near(rect.bounds.height(), 24.0);
                assert_eq!(rect.rgba, [255, 0, 0, 255]);
                if i > 0 {
                    near(rect.bounds.x0 - initial[i - 1].bounds.x1, 4.0);
                }
            }
            let mut previous = initial.clone();
            for (ms, time) in [
                (250, 0.25),
                (500, 0.5),
                (750, 0.75),
                (984, 63.0 / 64.0),
                (1000, 1.0),
            ] {
                let frame = outputs(&timeline, count, time);
                let width = if mode == Mode::Size {
                    12.0 + 8.0 * ms as f64 / 1000.0
                } else {
                    12.0
                };
                // Taffy's allocated slot locations can be pixel-rounded at the
                // fractional 984ms width; integer checkpoints remain strict.
                // Rect path width is sampled independently and stays strict too.
                let tolerance = if mode == Mode::Size && ms == 984 {
                    1.01
                } else {
                    0.01
                };
                for (i, rect) in frame.iter().enumerate() {
                    near(rect.bounds.width(), width);
                    near(rect.bounds.height(), 24.0);
                    assert!(rect.bounds.x0 >= 0.0 && rect.bounds.x1 <= DIMENSIONS.width as f64);
                    assert!(rect.bounds.y0 >= 0.0 && rect.bounds.y1 <= DIMENSIONS.height as f64);
                    if i > 0 {
                        near_with(rect.bounds.x0 - frame[i - 1].bounds.x1, 4.0, tolerance);
                    }
                    if mode == Mode::Color {
                        // Read fill AND geometry from this evaluation, not stale
                        // tracks/lookup state. Do not assume a color interpolation space.
                        assert!(rect.rgba[0] <= previous[i].rgba[0]);
                        assert!(rect.rgba[2] >= previous[i].rgba[2]);
                        assert_ne!(
                            rect.rgba, previous[i].rgba,
                            "color changes within sampled window"
                        );
                        assert_eq!(rect.rgba[3], 255);
                        let mut unchanged_geometry = rect.clone();
                        unchanged_geometry.rgba = initial[i].rgba;
                        same_rect(&unchanged_geometry, &initial[i]);
                    } else {
                        assert_eq!(rect.rgba, [255, 0, 0, 255]);
                    }
                    if mode == Mode::Static {
                        same_rect(rect, &initial[i]);
                    }
                }
                if mode == Mode::Size {
                    // Adjacent centers must move apart, not merely wider paths
                    // being painted over old slots with incorrect spacing.
                    near_with(
                        frame[1].bounds.center().x - frame[0].bounds.center().x,
                        width + 4.0,
                        tolerance,
                    );
                }
                previous = frame;
            }
            if mode == Mode::Color {
                assert!(previous.iter().all(|rect| rect.rgba == [0, 0, 255, 255]));
            }
            let reset = outputs(&timeline, count, 0.0);
            for (actual, expected) in reset.iter().zip(&initial) {
                same_rect(actual, expected);
            }
            // Repeat the forward sample after a backward seek too.
            let middle = outputs(&timeline, count, 0.5);
            if mode == Mode::Size {
                near(middle[0].bounds.width(), 16.0);
            }
            if mode == Mode::Color {
                assert_ne!(middle[0].rgba, initial[0].rgba);
            }
        }
    }
}

#[test]
fn layout_manifest_and_clock_match_frozen_parameters() {
    let manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../benches/fixtures/layout-invalidation-v1/manifest.json"
    ))
    .unwrap();
    assert_eq!(manifest["group"], GROUP);
    assert_eq!(manifest["counts"], serde_json::json!(COUNTS));
    assert_eq!(
        manifest["modes"],
        serde_json::json!([Mode::Static.id(), Mode::Color.id(), Mode::Size.id()])
    );
    assert_eq!(manifest["samples"], SAMPLE_COUNT);
    assert_eq!(manifest["dimensions"], serde_json::json!([DIMENSIONS.width, DIMENSIONS.height]));
    let mut clock = SampleClock::default();
    let mut previous = None;
    for i in 0..SAMPLE_COUNT * 3 {
        let time = clock.next_time();
        assert_eq!(time, (i % 64) as f64 / 64.0);
        let ms = (time * 1000.0) as u64;
        assert_ne!(Some(ms), previous);
        previous = Some(ms);
    }
    assert!(std::panic::catch_unwind(|| source(17, Mode::Static)).is_err());
}

#[test]
fn layout_source_bytes_are_frozen() {
    // Lightweight snapshot checks only; metadata carries SHA256 provenance.
    let expected = [
        (16, Mode::Static, 0x71b30f35e84ffc49_u64),
        (16, Mode::Color, 0x9b13aad36e9701bd_u64),
        (16, Mode::Size, 0x40f36efee0d27deb_u64),
        (64, Mode::Static, 0xcd5ff691b4c380f5_u64),
        (64, Mode::Color, 0xd93805904ac260ed_u64),
        (64, Mode::Size, 0x8c704a725a8855bb_u64),
    ];
    for (count, mode, hash) in expected {
        let text = source(count, mode);
        let actual = text.bytes().fold(0xcbf29ce484222325_u64, |state, byte| {
            (state ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
        assert_eq!(actual, hash, "change workload version when changing inputs");
        assert_eq!(text.matches(": Rect,").count(), count);
    }
}
