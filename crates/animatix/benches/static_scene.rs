use animatix::timeline::SceneDimensions;
use criterion::{Criterion, black_box};

use crate::support::build_static_scene;

pub(crate) fn bench_static(c: &mut Criterion) {
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    // The GUI's exact frame path: hit regions are always requested for
    // picking, so this bench pins the static-subtree-cache-under-hit-regions
    // behavior: hit regions must not disable the static subtree cache.
    let timeline_50_hits = build_static_scene(50);
    c.bench_function("static_50_actors_hit_regions", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            let time_s = ((counter % 1000) as f64) / 1000.0;
            counter += 1;
            let mut fb = None;
            black_box(timeline_50_hits.evaluate_with_debug(
                black_box(time_s),
                dims,
                animatix::timeline::DebugRenderOptions {
                    compute_hit_regions: true,
                    ..animatix::timeline::DebugRenderOptions::default()
                },
                &mut fb,
            ));
        })
    });

    let timeline_50 = build_static_scene(50);
    c.bench_function("static_50_actors", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            // Vary time to defeat frame cache — each iteration uses a different time
            let time_s = ((counter % 1000) as f64) / 1000.0;
            counter += 1;
            let mut fb = None;
            black_box(timeline_50.evaluate_with_debug(
                black_box(time_s),
                dims,
                animatix::timeline::DebugRenderOptions::default(),
                &mut fb,
            ));
        })
    });

    let timeline_100 = build_static_scene(100);
    c.bench_function("static_100_actors", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            let time_s = ((counter % 1000) as f64) / 1000.0;
            counter += 1;
            let mut fb = None;
            black_box(timeline_100.evaluate_with_debug(
                black_box(time_s),
                dims,
                animatix::timeline::DebugRenderOptions::default(),
                &mut fb,
            ));
        })
    });

    let timeline_200 = build_static_scene(200);
    c.bench_function("static_200_actors", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            let time_s = ((counter % 1000) as f64) / 1000.0;
            counter += 1;
            let mut fb = None;
            black_box(timeline_200.evaluate_with_debug(
                black_box(time_s),
                dims,
                animatix::timeline::DebugRenderOptions::default(),
                &mut fb,
            ));
        })
    });
}
