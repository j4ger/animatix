use animatix::timeline::{SceneDimensions, Timeline};
use chumsky::Parser;
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn build_static_scene(actor_count: usize) -> Timeline {
    // Static scene: no keyframes, no modifiers
    let mut source = String::from(r#"config { colorscheme: "editorial-dark" }

#0s
"#);
    for i in 0..actor_count {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + (i % 20) * 90,
            100 + (i / 20) * 90
        ));
    }
    let (stmts, _) = animatix::parser::parser().parse(&source).into_output_errors();
    Timeline::build(&stmts.unwrap())
}

fn bench_static(c: &mut Criterion) {
    let dims = SceneDimensions { width: 1920, height: 1080 };

    let timeline_50 = build_static_scene(50);
    c.bench_function("static_50_actors", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            // Vary time to defeat frame cache — each iteration uses a different time
            let time_s = ((counter % 1000) as f64) / 1000.0;
            counter += 1;
            black_box(timeline_50.evaluate_with_debug(
                black_box(time_s),
                dims,
                animatix::timeline::DebugRenderOptions::default(),
            ));
        })
    });

    let timeline_100 = build_static_scene(100);
    c.bench_function("static_100_actors", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            let time_s = ((counter % 1000) as f64) / 1000.0;
            counter += 1;
            black_box(timeline_100.evaluate_with_debug(
                black_box(time_s),
                dims,
                animatix::timeline::DebugRenderOptions::default(),
            ));
        })
    });

    let timeline_200 = build_static_scene(200);
    c.bench_function("static_200_actors", |b| {
        let mut counter = 0u64;
        b.iter(|| {
            let time_s = ((counter % 1000) as f64) / 1000.0;
            counter += 1;
            black_box(timeline_200.evaluate_with_debug(
                black_box(time_s),
                dims,
                animatix::timeline::DebugRenderOptions::default(),
            ));
        })
    });
}

criterion_group!(benches, bench_static);
criterion_main!(benches);
