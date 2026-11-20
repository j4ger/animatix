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
