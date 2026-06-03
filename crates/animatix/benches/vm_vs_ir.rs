use animatix::timeline::{SceneDimensions, Timeline};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

mod common;

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
    common::parse_timeline(source)
}

fn build_static_timeline() -> Timeline {
    // Same scene but without always block
    let source = r#"
config { colorscheme: "editorial-dark" }

#0s
center: Ellipse, size: (16, 16), color: text.primary, at: (640, 390)
orbiter: Ellipse, size: (64, 64), color: accent.primary, at: (820, 390)
pulse: Rect, size: (120, 120), color: (0.88, 0.42, 0.84, 1.0), at: (280, 390)
echo: Ellipse, size: (40, 40), color: accent.warning, at: pulse.at
"#;
    common::parse_timeline(source)
}

fn bench_modifier_overhead(c: &mut Criterion) {
    let reactive = build_reactive_timeline();
    let static_tl = build_static_timeline();
    let dims = SceneDimensions { width: 1920, height: 1080 };

    c.bench_function("reactive_evaluate_100frames", |b| {
        b.iter(|| {
            for i in 0..100 {
                let t = i as f64 / 60.0;
                black_box(reactive.evaluate(black_box(t), dims));
            }
        })
    });

    c.bench_function("static_evaluate_100frames", |b| {
        b.iter(|| {
            for i in 0..100 {
                let t = i as f64 / 60.0;
                black_box(static_tl.evaluate(black_box(t), dims));
            }
        })
    });
}

criterion_group!(benches, bench_modifier_overhead);
criterion_main!(benches);
