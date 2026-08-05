use animatix::timeline::{SceneDimensions, Timeline};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

mod common;

fn build_scene(actor_count: usize) -> Timeline {
    let mut source = String::from(
        r#"config { colorscheme: "editorial-dark" }

#0s
"#,
    );
    for i in 0..actor_count {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + (i % 30) * 60,
            100 + (i / 30) * 60
        ));
    }
    common::parse_timeline(&source)
}

fn bench_scaling_no_cache(c: &mut Criterion) {
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

criterion_group!(benches, bench_scaling_no_cache);
criterion_main!(benches);
