use animatix::timeline::SceneDimensions;
use criterion::{Criterion, black_box};

use crate::support::build_scene;

pub(crate) fn bench_scaling_no_cache(c: &mut Criterion) {
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    for count in [10, 25, 50, 100, 200] {
        let timeline = build_scene(count);
        c.bench_function(&format!("evaluate_{count}_actors_no_cache"), |b| {
            b.iter(|| {
                // Use debug options to defeat frame cache
                let mut fb = None;
                black_box(timeline.evaluate_with_debug(
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
}
