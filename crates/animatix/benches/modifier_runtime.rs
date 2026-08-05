use animatix::timeline::Timeline;
use criterion::{Criterion, black_box, criterion_group, criterion_main};

mod common;

fn build_timeline_with_modifiers() -> Timeline {
    let source = r#"
# 0s
bg: Rect {
    size: (1920, 1080),
    color: black,
}
pulse: Rect {
    size: (100, 100),
    position: (960, 540),
}

always {
    pulse.opacity = t / 2
}
"#;
    common::parse_timeline(source)
}

fn bench_modifier_evaluation(c: &mut Criterion) {
    let timeline = build_timeline_with_modifiers();
    let dims = animatix::timeline::SceneDimensions {
        width: 1920,
        height: 1080,
    };

    c.bench_function("modifier_evaluate_at_0s", |b| {
        b.iter(|| {
            timeline.evaluate(black_box(0.0), dims);
        })
    });

    c.bench_function("modifier_evaluate_at_1s", |b| {
        b.iter(|| {
            timeline.evaluate(black_box(1.0), dims);
        })
    });
}

criterion_group!(benches, bench_modifier_evaluation);
criterion_main!(benches);
