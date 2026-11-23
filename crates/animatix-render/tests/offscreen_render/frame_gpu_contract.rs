//! Frame-level contracts for the candidate renderer.

use animatix::timeline::SceneDimensions;
use animatix_render::offscreen::{OffscreenRenderer, RenderedFrame};

use super::common;

const DIMS: SceneDimensions = SceneDimensions {
    width: 96,
    height: 64,
};

fn pixel(frame: &RenderedFrame, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * frame.width as usize + x) * 4;
    frame.rgba[offset..offset + 4].try_into().unwrap()
}

fn renderer_or_skip() -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new() {
        Ok(renderer) => Some(renderer),
        Err(_) => {
            animatix_render::testing::skip_if_no_gpu();
            None
        },
    }
}

#[test]
fn equation_fragment_move_and_highlight_render_on_the_authored_term() {
    let Some(mut renderer) = renderer_or_skip() else {
        return;
    };
    let timeline = common::build_checked(
        r#"
config { resolution: (280, 120), duration: 3 }
#0s
eq: Equation, at: (160, 60), font_size: 42, equation_transition: 0.5 {
  lhs: Fragment, text: "x", fragment_order: 1, fragment_color: rgba(0.1, 1, 0.1, 1)
  plus: Fragment, text: "+", fragment_order: 2
  rhs: Fragment, text: "x", fragment_order: 3
}
#0.4s
  highlight(target: eq.rhs, color: red, blend: normal, duration: 100ms)
#0.8s
  unhighlight(target: eq.rhs, duration: 100ms)
#1.2s
  eq.lhs.fragment_order = 3
  eq.rhs.fragment_order = 1
"#,
    );
    let dims = SceneDimensions {
        width: 280,
        height: 120,
    };
    let mut frame = |time| renderer.render_timeline(&timeline, time, dims).unwrap();
    let red_pixels = |f: &RenderedFrame| {
        f.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] > 160 && p[1] < 80 && p[2] < 80)
            .count()
    };
    let green_center = |f: &RenderedFrame| {
        let xs: Vec<_> = f
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(_, p)| p[1] > 140 && p[0] < 110 && p[2] < 110)
            .map(|(i, _)| i % f.width as usize)
            .collect();
        assert!(!xs.is_empty(), "named fragment must remain green");
        xs.iter().sum::<usize>() as f64 / xs.len() as f64
    };
    let before = frame(0.2);
    let highlighted = frame(0.6);
    let cleared = frame(1.0);
    assert_eq!(red_pixels(&before), 0);
    assert!(red_pixels(&highlighted) > 40, "translated term highlight must be visible");
    assert_eq!(before.rgba, cleared.rgba, "siblings are clear before and after");
    let from = green_center(&cleared);
    let during = green_center(&frame(1.45));
    let to = green_center(&frame(1.8));
    assert!(from + 5.0 < during && during + 5.0 < to, "{from}, {during}, {to}");
}

#[test]
fn ellipse_shadow_follows_the_silhouette_with_signed_spread() {
    let Some(mut renderer) = renderer_or_skip() else {
        return;
    };
    let render = |renderer: &mut OffscreenRenderer, blur: f32, spread: f32| {
        let timeline = common::build_checked(&format!(
            r##"
config {{ resolution: (96, 64) }}
#0s
  bg: Rect, at: (48,32), size: (96,64), color: hex_color("ffffff")
  oval: Ellipse, at: (33,14), size: (48,14), color: hex_color("406080"),
    shadow: (0,28,{blur},{spread}), shadow_color: (0,0,0,1)
"##,
        ));
        renderer.render_timeline(&timeline, 0.0, DIMS).expect("shadow frame")
    };
    let sharp = render(&mut renderer, 0.0, 0.0);
    assert!(pixel(&sharp, 12, 42)[0] < 10, "shadow should reach the wide end");
    assert_eq!(pixel(&sharp, 12, 36), [255; 4], "oval corner should be clear");
    let soft = render(&mut renderer, 4.0, 2.0);
    let penumbra = pixel(&soft, 33, 52)[0];
    assert!((35..245).contains(&penumbra), "expected a soft edge, got {penumbra}");
    assert_eq!(pixel(&soft, 60, 26), [255; 4], "no square corner behind the oval");
    let inset = render(&mut renderer, 0.0, -3.0);
    assert!(pixel(&inset, 33, 42)[0] < 10, "shrinking still leaves the center");
    assert_eq!(pixel(&inset, 33, 36), [255; 4], "negative spread shrinks the shadow");
    let collapsed = render(&mut renderer, 0.0, -8.0);
    assert_eq!(pixel(&collapsed, 33, 42), [255; 4], "the inset consumes the shadow");
}

#[test]
fn opaque_readback_matches_straight_alpha_conversion() {
    let Some(mut candidate) = renderer_or_skip() else {
        return;
    };
    let timeline = common::build_checked(
        r##"
config { resolution: (96, 64) }
#0s
  translucent: Rect, at: (28, 30), size: (46, 38), color: hex_color("ff804080")
  panel: Glass, at: (52, 32), size: (45, 38), corner_radius: 5 {
    frost: Blur, radius: 4
    tint: Rect, size: (43, 36), color: hex_color("ffeee033")
  }
"##,
    );
    let pending = candidate
        .begin_frame_with_debug(&timeline, 0.0, DIMS, Default::default())
        .expect("queue the frame");
    let direct = candidate.wait_frame(pending).expect("direct readback");
    let converted = candidate.readback_output(DIMS).expect("straight-alpha readback");

    assert_ne!(pixel(&direct, 12, 23), pixel(&direct, 0, 0));
    assert!(direct.rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255));
    assert_eq!(direct.rgba, converted.rgba);
}

#[test]
fn inline_filter_stays_below_later_sibling() {
    let Some(mut candidate) = renderer_or_skip() else {
        return;
    };
    let timeline = common::build_checked(
        r##"
config { resolution: (96, 64) }
#0s
  bg: Rect, at: (48, 32), size: (96, 64), color: hex_color("152436")
  filtered: Filter, at: (48, 32) {
    soft: Blur, radius: 4
    fill: Rect, size: (40, 28), color: hex_color("a8e676")
  }
  front: Rect, at: (48, 32), size: (10, 10), color: hex_color("fc786c")
"##,
    );
    let new = candidate.render_timeline(&timeline, 0.0, DIMS).expect("candidate frame");
    assert_eq!(pixel(&new, 48, 32), [252, 120, 108, 255]);
    let edge = pixel(&new, 28, 32);
    assert!(edge[1] > 55 && edge[1] < 220, "filter edge {edge:?}");
}

#[test]
fn bounded_grain_and_vignette_match_compute_coordinates() {
    let Some(mut candidate) = renderer_or_skip() else {
        return;
    };
    let timeline = common::build_checked(
        r##"
config { resolution: (96, 64), duration: 1 }
#0s
  bg: Rect, at: (48, 32), size: (96, 64), color: hex_color("152436")
  filtered: Filter, at: (38, 24), bounds: (17, 11, 42, 26) {
    grain: Grain, amount: 0.6, seed: 3, monochrome: false
    edges: Vignette, amount: 0.8, radius: 0.15, softness: 0.25
    fill: Rect, size: (42, 26), color: hex_color("778899")
  }
  panel: Glass, at: (76, 24), size: (26, 26) {
    glass_grain: Grain, amount: 0.6, seed: 3, monochrome: true
    glass_edges: Vignette, amount: 0.8, radius: 0.15, softness: 0.25
  }
"##,
    );
    // Sampled with Compute on the same fixture before removing the backend.
    let coordinates = [
        (19, 13),
        (27, 18),
        (40, 22),
        (55, 33),
        (66, 15),
        (79, 23),
        (87, 33),
    ];
    let reference = [
        [
            [21, 14, 36, 255],
            [10, 22, 26, 255],
            [24, 19, 52, 255],
            [128, 99, 162, 255],
            [12, 15, 18, 255],
            [4, 7, 11, 255],
            [79, 93, 109, 255],
        ],
        [
            [9, 26, 24, 255],
            [34, 38, 38, 255],
            [43, 35, 39, 255],
            [118, 89, 194, 255],
            [16, 19, 22, 255],
            [1, 4, 8, 255],
            [0, 0, 0, 255],
        ],
    ];
    let mut samples = Vec::new();
    for (time, expected_pixels) in [0.0, 0.1].into_iter().zip(reference) {
        let new = candidate.render_timeline(&timeline, time, DIMS).expect("candidate frame");
        for ((x, y), expected) in coordinates.into_iter().zip(expected_pixels) {
            let actual = pixel(&new, x, y);
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 2),
                "pixel ({x}, {y}) at {time}s: {actual:?} vs {expected:?}"
            );
        }
        samples.push(pixel(&new, 40, 22));
    }
    assert_ne!(samples[0], samples[1], "Grain must advance with the playhead");
}

#[test]
fn vignette_clear_area_follows_animated_center() {
    let Some(mut renderer) = renderer_or_skip() else {
        return;
    };
    let timeline = common::build_checked(
        r##"
config { resolution: (96, 64), duration: 2 }
#0s
  plate: Filter, at: (48, 32), bounds: (0, 0, 96, 64) {
    shade: Vignette, amount: 1, radius: 0.1, softness: 0.2, center: (0.25, 0.25)
    fill: Rect, size: (96, 64), color: hex_color("ffffff")
  }
#0.5s
  plate.shade.center = (0.75, 0.75) [1s, ease: linear]
"##,
    );
    let positions = [(24, 16), (48, 32), (72, 48)];
    for (focus, time) in [0.0, 1.0, 1.5].into_iter().enumerate() {
        let frame = renderer.render_timeline(&timeline, time, DIMS).expect("vignette frame");
        for (index, (x, y)) in positions.into_iter().enumerate() {
            let expected = if index == focus {
                [255; 4]
            } else {
                [0, 0, 0, 255]
            };
            assert_eq!(pixel(&frame, x, y), expected, "pixel ({x}, {y}) at {time}s");
        }
    }
}

#[test]
fn vignette_follows_later_actor_points_inside_a_padded_crop() {
    let Some(mut renderer) = renderer_or_skip() else {
        return;
    };
    let timeline = common::build_checked(
        r##"
config { resolution: (96, 64), duration: 2 }
#0s
  plate: Filter, at: (48, 32), bounds: (8, 8, 80, 48) {
    shade: Vignette, amount: 1, radius: 0.08, softness: 0.12, center: cards[0].center
    fill: Rect, size: (96, 64), color: hex_color("ffffff")
  }
  for i in {0, 1} {
    cards[i]: Rect, at: (24 + 48 * i, 32), size: (4, 4), opacity: 0
  }
#0.5s
  plate.shade.center = cards[1].origin [1s, ease: linear]
"##,
    );
    let positions = [24, 48, 72];
    for (focus, time) in [0.0, 1.0, 1.5].into_iter().enumerate() {
        let frame = renderer.render_timeline(&timeline, time, DIMS).expect("vignette frame");
        for (index, x) in positions.into_iter().enumerate() {
            let light = if index == focus { 255 } else { 0 };
            assert_eq!(pixel(&frame, x, 32), [light, light, light, 255], "pixel {x} at {time}s");
        }
    }
}

#[test]
fn deferred_glass_panel_stays_above_later_base_draws() {
    let Some(mut candidate) = renderer_or_skip() else {
        return;
    };
    let timeline = common::build_checked(
        r##"
config { resolution: (96, 64) }
#0s
  bg: Rect, at: (48, 32), size: (96, 64), color: hex_color("152436")
  panel: Glass, at: (48, 32), size: (60, 48), corner_radius: 6 {
    frost: Blur, radius: 4
    badge: Rect, size: (12, 12), color: hex_color("fc786c")
  }
  later: Rect, at: (48, 32), size: (30, 30), color: hex_color("a8e676")
"##,
    );
    let new = candidate.render_timeline(&timeline, 0.0, DIMS).expect("candidate frame");
    assert_eq!(pixel(&new, 48, 32), [252, 120, 108, 255]);
    assert_eq!(pixel(&new, 82, 32), [21, 36, 54, 255]);
}
