use animatix::timeline::{DebugRenderOptions, SceneDimensions};
use criterion::{Criterion, black_box};

use crate::support::build_static_scene;

pub(crate) fn bench_static_item_collection(c: &mut Criterion) {
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let timeline = build_static_scene(50);

    c.bench_function("static_50_actors_scene_only", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            let time_s = ((counter % 1000) as f64) / 1000.0;
            counter += 1;
            let mut fb = None;
            black_box(timeline.evaluate_with_debug(
                black_box(time_s),
                dims,
                DebugRenderOptions::default(),
                &mut fb,
            ));
        })
    });

    c.bench_function("static_50_actors_with_items", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            let time_s = ((counter % 1000) as f64) / 1000.0;
            counter += 1;
            let mut fb = None;
            black_box(timeline.evaluate_program_with_debug(
                black_box(time_s),
                dims,
                DebugRenderOptions::default(),
                &mut fb,
            ));
        })
    });
}
