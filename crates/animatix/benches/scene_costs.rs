use animatix::timeline::{SceneDimensions, Timeline};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

mod common;

fn build_many_actors_scene() -> Timeline {
    let mut source = String::from(
        r#"config { colorscheme: "editorial-dark" }

#0s
"#,
    );
    for i in 0..50 {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + i * 30,
            100 + (i % 10) * 50
        ));
    }
    common::parse_timeline(&source)
}

fn bench_scene_costs(c: &mut Criterion) {
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

criterion_group!(benches, bench_scene_costs);
criterion_main!(benches);
