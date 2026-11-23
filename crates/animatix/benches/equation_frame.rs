//! Per-frame cost of `Equation` / `Fragment` scenes.
//!
//! Equation children form one Typst document. Changing placement forces the
//! renderer to sample the fragments and reuse the cached compiled document.

use animatix::timeline::{SceneDimensions, Timeline};
use criterion::{Criterion, black_box};

use crate::common;

const DIMS: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};

/// A dynamic Equation scene.
///
/// The `always` block is essential: without it the subtree is static and the
/// static-subtree cache serves every frame, so the Equation branch runs once
/// and the bench would only measure a scene clone.
fn build_equation_scene() -> Timeline {
    let source = r#"
config { colorscheme: "editorial-dark" }

#0s
eq: Equation, font_size: 48, color: text.primary, at: (960, 540) {
  lhs: Fragment, text: "E"
  eq_sign: Fragment, text: " = "
  mass: Fragment, text: "m"
  c2: Fragment, text: "c^2"
}

always {
  eq.at = (960 + 40 * sin(t), 540)
}
"#;
    common::parse_timeline(source)
}

pub(crate) fn bench_equation_frame(c: &mut Criterion) {
    let scene = build_equation_scene();

    // Distinct times per frame guarantee a frame-cache miss every frame — the
    // realistic worst case for authoring/scrubbing.
    c.bench_function("equation_frame_100frames", |b| {
        b.iter(|| {
            for i in 0..100 {
                let t = (i * 17 % 100) as f64 / 60.0;
                black_box(scene.evaluate(black_box(t), DIMS));
            }
        })
    });
}
