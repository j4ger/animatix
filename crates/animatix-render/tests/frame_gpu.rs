//! Frame-level contracts for the candidate renderer.

#![cfg(feature = "vello-gpu")]

use animatix::timeline::SceneDimensions;
use animatix_render::offscreen::{OffscreenRenderer, RenderedFrame};

mod common;

const DIMS: SceneDimensions = SceneDimensions {
    width: 96,
    height: 64,
};

fn pixel(frame: &RenderedFrame, x: usize, y: usize) -> [u8; 4] {
    let offset = (y * frame.width as usize + x) * 4;
    frame.rgba[offset..offset + 4].try_into().unwrap()
}

fn pair() -> Option<(OffscreenRenderer, OffscreenRenderer)> {
    let Ok(reference) = OffscreenRenderer::new_legacy() else {
        animatix_render::testing::skip_if_no_gpu();
        return None;
    };
    let candidate = OffscreenRenderer::new_frame_gpu().expect("candidate adapter");
    Some((reference, candidate))
}

#[test]
fn opaque_readback_matches_straight_alpha_conversion() {
    let Some((_, mut candidate)) = pair() else {
        return;
    };
    let timeline = common::build_checked(
        r##"
config { resolution: (96, 64) }
#0s
  translucent: Rect, at: (28, 30), size: (46, 38), color: "#FF804080"
  panel: Glass, at: (52, 32), size: (45, 38), corner_radius: 5 {
    frost: Blur, radius: 4
    tint: Rect, size: (43, 36), color: "#FFEEE033"
  }
"##,
    );
    let pending = candidate
        .begin_frame_with_debug(&timeline, 0.0, DIMS, Default::default())
        .expect("queue the frame");
    let direct = candidate.wait_frame(pending).expect("direct readback");
    let converted = candidate.readback_output(DIMS).expect("straight-alpha readback");

    assert_ne!(pixel(&direct, 12, 23), pixel(&direct, 0, 0));
    assert!(direct.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
    assert_eq!(direct.rgba, converted.rgba);
}

#[test]
fn inline_filter_stays_below_later_sibling() {
    let Some((mut reference, mut candidate)) = pair() else {
        return;
    };
    let timeline = common::build_checked(
        r##"
config { resolution: (96, 64) }
#0s
  bg: Rect, at: (48, 32), size: (96, 64), color: "#152436"
  filtered: Filter, at: (48, 32) {
    soft: Blur, radius: 4
    fill: Rect, size: (40, 28), color: "#A8E676"
  }
  front: Rect, at: (48, 32), size: (10, 10), color: "#FC786C"
"##,
    );
    let old = reference.render_timeline(&timeline, 0.0, DIMS).expect("legacy frame");
    let new = candidate.render_timeline(&timeline, 0.0, DIMS).expect("candidate frame");
    assert_eq!(pixel(&old, 48, 32), [252, 120, 108, 255]);
    assert_eq!(pixel(&new, 48, 32), pixel(&old, 48, 32));
    let edge = pixel(&new, 28, 32);
    assert!(edge[1] > 55 && edge[1] < 220, "filter edge {edge:?}");
}

#[test]
fn bounded_grain_and_vignette_match_compute_coordinates() {
    let Some((mut reference, mut candidate)) = pair() else {
        return;
    };
    let timeline = common::build_checked(
        r##"
config { resolution: (96, 64), duration: 1 }
#0s
  bg: Rect, at: (48, 32), size: (96, 64), color: "#152436"
  filtered: Filter, at: (38, 24), bounds: (17, 11, 42, 26) {
    grain: Grain, amount: 0.6, seed: 3, monochrome: false
    edges: Vignette, amount: 0.8, radius: 0.15, softness: 0.25
    fill: Rect, size: (42, 26), color: "#778899"
  }
  panel: Glass, at: (76, 24), size: (26, 26) {
    glass_grain: Grain, amount: 0.6, seed: 3, monochrome: true
    glass_edges: Vignette, amount: 0.8, radius: 0.15, softness: 0.25
  }
"##,
    );
    let mut samples = Vec::new();
    for time in [0.0, 0.1] {
        let old = reference.render_timeline(&timeline, time, DIMS).expect("legacy frame");
        let new = candidate.render_timeline(&timeline, time, DIMS).expect("candidate frame");
        for (x, y) in [
            (19, 13),
            (27, 18),
            (40, 22),
            (55, 33),
            (66, 15),
            (79, 23),
            (87, 33),
        ] {
            let expected = pixel(&old, x, y);
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
    let Some((mut reference, mut candidate)) = pair() else {
        return;
    };
    let timeline = common::build_checked(
        r##"
config { resolution: (96, 64) }
#0s
  bg: Rect, at: (48, 32), size: (96, 64), color: "#152436"
  panel: Glass, at: (48, 32), size: (60, 48), corner_radius: 6 {
    frost: Blur, radius: 4
    badge: Rect, size: (12, 12), color: "#FC786C"
  }
  later: Rect, at: (48, 32), size: (30, 30), color: "#A8E676"
"##,
    );
    let old = reference.render_timeline(&timeline, 0.0, DIMS).expect("legacy frame");
    let new = candidate.render_timeline(&timeline, 0.0, DIMS).expect("candidate frame");
    assert_eq!(pixel(&old, 48, 32), [252, 120, 108, 255]);
    assert_eq!(pixel(&new, 48, 32), pixel(&old, 48, 32));
    assert_eq!(pixel(&new, 82, 32), pixel(&old, 82, 32));
}
