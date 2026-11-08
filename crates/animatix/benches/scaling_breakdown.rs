use animatix::timeline::SceneDimensions;
use criterion::{Criterion, black_box};

use crate::support::build_scene;

pub(crate) fn bench_scaling_breakdown(c: &mut Criterion) {
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };

    for count in [50, 100, 200] {
        let timeline = build_scene(count);

        // Full evaluate (no cache)
        c.bench_function(&format!("full_{count}"), |b| {
            b.iter(|| {
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

        // Just frame env setup
        c.bench_function(&format!("env_{count}"), |b| {
            b.iter(|| {
                let env = timeline.build_frame_env(500, dims, &std::collections::HashMap::new());
                black_box(env);
            })
        });
    }
}
