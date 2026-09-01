//! Per-frame cost of `Equation` / `Fragment` scenes (PF-4).
//!
//! Equation children are compiled as one Typst document on every frame (the
//! `ChildProcessing::Equation` branch of `render_node_children`). Unlike
//! Text/Code declarations, that path does not go through the process-wide
//! `compile_text_cached` memo, so each frame pays a full Typst parse + eval +
//! layout even when no fragment content or font property changed — which is
//! the common case while scrubbing or playing back an equation-heavy scene.
//!
//! This bench pins that per-frame cost so the memoization is gated.

use animatix::timeline::{SceneDimensions, Timeline};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

mod common;

const DIMS: SceneDimensions = SceneDimensions {
    width: 1920,
    height: 1080,
};

/// A dynamic Equation scene.
///
/// The `always` block is essential: without it the subtree is static and the
/// static-subtree cache serves every frame, so the Equation branch runs once
/// and the bench would only measure a scene clone. Real equation scenes drift,
/// fade, or reveal fragments, which is exactly the case that recompiles Typst
/// every frame.
fn build_equation_scene() -> Timeline {
    let source = r#"
config { colorscheme: "editorial-dark" }

#0s
eq: Equation, font_size: 48, color: text.primary, at: (960, 540) {
  lhs: Fragment, content: "E"
  eq_sign: Fragment, content: " = "
  mass: Fragment, content: "m"
  c2: Fragment, content: "c^2"
}

always {
  eq.at = (960 + 40 * sin(t), 540)
}
"#;
    common::parse_timeline(source)
}

fn bench_equation_frame(c: &mut Criterion) {
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

criterion_group!(benches, bench_equation_frame);
criterion_main!(benches);
