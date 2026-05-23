use animatix::timeline::{SceneDimensions, Timeline};
use chumsky::Parser;
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn build_scene(actor_count: usize) -> Timeline {
    let mut source = String::from(r#"config { colorscheme: "editorial-dark" }

#0s
"#);
    for i in 0..actor_count {
        source.push_str(&format!(
            "box{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + (i % 30) * 60,
            100 + (i / 30) * 60
        ));
    }
    let (stmts, _) = animatix::parser::parser().parse(&source).into_output_errors();
    Timeline::build(&stmts.unwrap())
}

fn bench_scaling_breakdown(c: &mut Criterion) {
    let dims = SceneDimensions { width: 1920, height: 1080 };

    for count in [50, 100, 200] {
        let timeline = build_scene(count);

        // Full evaluate (no cache)
        c.bench_function(&format!("full_{count}"), |b| {
            b.iter(|| {
                black_box(timeline.evaluate_with_debug(
                    black_box(0.5),
                    dims,
                    animatix::timeline::DebugRenderOptions { draw_bounds: true, compute_hit_regions: false },
                ));
            })
        });

        // Just frame env setup
        c.bench_function(&format!("env_{count}"), |b| {
            b.iter(|| {
                let env = timeline.frame(500, dims, &std::collections::HashMap::new());
                black_box(env);
            })
        });
    }
}

criterion_group!(benches, bench_scaling_breakdown);
criterion_main!(benches);
