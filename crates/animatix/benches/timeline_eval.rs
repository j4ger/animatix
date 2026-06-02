use animatix::timeline::Timeline;
use chumsky::Parser;
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn build_test_timeline() -> Timeline {
    let source = r#"
# 0s
bg: Rect {
    size: (1920, 1080),
    color: black,
}
title: Text {
    content: "Hello World",
    position: (960, 540),
    font_size: 48,
}

# 1s
title.position = (960, 400)
title.opacity = 0.5

# 2s
title.position = (960, 600)
title.opacity = 1.0
"#;
    let (stmts, _) = animatix_syntax::parser::parser().parse(source).into_output_errors();
    Timeline::build(&stmts.unwrap())
}

fn bench_timeline_evaluate(c: &mut Criterion) {
    let timeline = build_test_timeline();

    let dims = animatix::timeline::SceneDimensions { width: 1920, height: 1080 };

    c.bench_function("timeline_evaluate_0s", |b| {
        b.iter(|| {
            timeline.evaluate(black_box(0.0), dims);
        })
    });

    c.bench_function("timeline_evaluate_1s", |b| {
        b.iter(|| {
            timeline.evaluate(black_box(1.0), dims);
        })
    });

    c.bench_function("timeline_evaluate_2s", |b| {
        b.iter(|| {
            timeline.evaluate(black_box(2.0), dims);
        })
    });
}

criterion_group!(benches, bench_timeline_evaluate);
criterion_main!(benches);
