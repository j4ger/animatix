use criterion::{Criterion, criterion_group, criterion_main};

mod common;

fn build_reactive_source() -> String {
    r#"
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
"#
    .to_string()
}

fn bench_build_time(c: &mut Criterion) {
    let source = build_reactive_source();

    c.bench_function("build_reactive_timeline", |b| {
        b.iter(|| {
            let _ = common::parse_timeline(&source);
        })
    });
}

criterion_group!(benches, bench_build_time);
criterion_main!(benches);
