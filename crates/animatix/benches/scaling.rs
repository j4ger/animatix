use animatix::timeline::SceneDimensions;
use criterion::{Criterion, black_box};

use crate::support::build_scene;

pub(crate) fn bench_scaling(c: &mut Criterion) {
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    for count in [10, 25, 50, 100, 200] {
        let timeline = build_scene(count);
        c.bench_function(&format!("evaluate_{count}_actors"), |b| {
            b.iter(|| {
                black_box(timeline.evaluate(black_box(0.5), dims));
            })
        });
    }
}
