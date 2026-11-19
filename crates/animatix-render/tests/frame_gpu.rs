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
