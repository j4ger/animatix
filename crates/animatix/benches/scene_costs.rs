use animatix::timeline::SceneDimensions;
use criterion::{Criterion, black_box};

use crate::support::build_many_actors_scene;

pub(crate) fn bench_scene_costs(c: &mut Criterion) {
    let many_actors = build_many_actors_scene();
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    // The cache-hit floors (`many_actors_evaluate`, `mixed_scene_evaluate`,
    // `many_actors_cache_hit`) were removed 2026-10-26: they measured a
    // ~0.3 µs frame-cache restore with layout-level noise, carried no
    // regression signal (the TINY_NS guard existed mostly to babysit them),
    // and the miss-path bench below covers the actual evaluation cost.
    c.bench_function("many_actors_evaluate_no_cache", |b| {
        b.iter(|| {
            // Evaluate with non-default debug options to skip cache
            let mut fb = None;
            black_box(many_actors.evaluate_with_debug(
                black_box(0.5),
                dims,
                animatix::timeline::DebugRenderOptions {
                    draw_bounds: true,
                    compute_hit_regions: false,
                    ..Default::default()
                },
                &mut fb,
            ));
        })
    });
}
