use animatix::timeline::{SceneDimensions, Timeline};
use chumsky::Parser;
use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn build_reactive_timeline() -> Timeline {
    let source = r#"
config { colorscheme: "editorial-dark" }

#0s
center: Ellipse, size: (16, 16), color: text.primary, at: (640, 390)
orbiter: Ellipse, size: (64, 64), color: accent.primary, at: (820, 390)
pulse: Rect, size: (120, 120), color: (0.88, 0.42, 0.84, 1.0), at: (280, 390)
echo: Ellipse, size: (40, 40), color: accent.warning, at: pulse.at

always {
  orbiter.at = (640 + 180 * cos(t), 390 + 120 * sin(t * 2))
  pulse.size = if (t % 1.0) < 0.5 { (120, 120) } else { (180, 180) }
  echo.size = (pulse.size.x / 3, pulse.size.x / 3)
  echo.at = orbiter.at
}
"#;
    let (stmts, _) = animatix_syntax::parser::parser().parse(source).into_output_errors();
    Timeline::build(&stmts.unwrap())
}

fn bench_reactive_timeline(c: &mut Criterion) {
    let timeline = build_reactive_timeline();
    let dims = SceneDimensions { width: 1920, height: 1080 };

    // Benchmark evaluating at many different times to defeat the frame cache
    c.bench_function("reactive_playback_100frames", |b| {
        b.iter(|| {
            for i in 0..100 {
                let t = i as f64 / 60.0; // 100 frames at 60fps
                black_box(timeline.evaluate(black_box(t), dims));
            }
        })
    });

    // Also benchmark with constant time to show cache effect
    c.bench_function("reactive_cached_frame", |b| {
        b.iter(|| {
            black_box(timeline.evaluate(black_box(1.0), dims));
        })
    });
}

criterion_group!(benches, bench_reactive_timeline);
criterion_main!(benches);
