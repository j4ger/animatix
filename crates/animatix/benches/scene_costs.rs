use animatix::timeline::{SceneDimensions, Timeline};
use chumsky::Parser;
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn build_many_actors_scene() -> Timeline {
    let mut source = String::from(r#"config { colorscheme: "editorial-dark" }

#0s
"#);
    for i in 0..50 {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + i * 30,
            100 + (i % 10) * 50
        ));
    }
    let (stmts, _) = animatix::parser::parser().parse(&source).into_output_errors();
    Timeline::build(&stmts.unwrap())
}

fn build_mixed_scene() -> Timeline {
    let mut source = String::from(r#"config { colorscheme: "editorial-dark" }

#0s
"#);
    for i in 0..20 {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + i * 80,
            100 + (i % 5) * 100
        ));
    }
    source.push_str(r#"
title: Text, content: "Hello World", font_size: 48, color: text.primary, at: (960, 400)
subtitle: Text, content: "Subtitle text", font_size: 24, color: text.secondary, at: (960, 500)
"#);
    let (stmts, _) = animatix::parser::parser().parse(&source).into_output_errors();
    Timeline::build(&stmts.unwrap())
}

fn bench_scene_costs(c: &mut Criterion) {
    let many_actors = build_many_actors_scene();
    let mixed = build_mixed_scene();
    let dims = SceneDimensions { width: 1920, height: 1080 };

    // Benchmark the cost of cloning the scene (frame cache)
    c.bench_function("many_actors_evaluate", |b| {
        b.iter(|| {
            black_box(many_actors.evaluate(black_box(0.5), dims));
        })
    });

    c.bench_function("many_actors_evaluate_no_cache", |b| {
        b.iter(|| {
            // Evaluate with non-default debug options to skip cache
            black_box(many_actors.evaluate_with_debug(
                black_box(0.5),
                dims,
                animatix::timeline::DebugRenderOptions { draw_bounds: true },
            ));
        })
    });

    c.bench_function("mixed_scene_evaluate", |b| {
        b.iter(|| {
            black_box(mixed.evaluate(black_box(0.5), dims));
        })
    });

    // Benchmark repeated evaluation at same time (cache hit)
    c.bench_function("many_actors_cache_hit", |b| {
        // Prime cache
        let _ = many_actors.evaluate(0.5, dims);
        b.iter(|| {
            black_box(many_actors.evaluate(black_box(0.5), dims));
        })
    });
}

criterion_group!(benches, bench_scene_costs);
criterion_main!(benches);
