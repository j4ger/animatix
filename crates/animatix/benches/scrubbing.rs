use animatix::timeline::{SceneDimensions, Timeline};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

mod common;

fn build_text_scene() -> Timeline {
    let source = r#"
config { colorscheme: "editorial-dark" }

#0s
title: Text, content: "Hello World", font_size: 48, color: text.primary, at: (960, 540)
subtitle: Text, content: "Subtitle text here", font_size: 24, color: text.secondary, at: (960, 600)
"#;
    common::parse_timeline(source)
}

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
    common::parse_timeline(&source)
}

fn build_layout_scene() -> Timeline {
    let source = r#"
config { colorscheme: "editorial-dark", dynamic_layout: true }

#0s
container: Row, at: (100, 100), gap: 20, align: "center" {
  a: Rect, size: (80, 80), color: accent.primary
  b: Rect, size: (100, 100), color: accent.success
  c: Rect, size: (60, 60), color: accent.warning
  d: Rect, size: (90, 90), color: accent.danger
  e: Rect, size: (70, 70), color: accent.primary
}
"#;
    common::parse_timeline(source)
}

fn bench_scrubbing(c: &mut Criterion) {
    let text_scene = build_text_scene();
    let many_actors = build_many_actors_scene();
    let layout_scene = build_layout_scene();
    let dims = SceneDimensions { width: 1920, height: 1080 };

    // Benchmark random-access scrubbing (different times each iteration)
    c.bench_function("scrub_text_scene_100frames", |b| {
        b.iter(|| {
            for i in 0..100 {
                let t = (i * 17 % 100) as f64 / 60.0; // pseudo-random access pattern
                black_box(text_scene.evaluate(black_box(t), dims));
            }
        })
    });

    c.bench_function("scrub_many_actors_100frames", |b| {
        b.iter(|| {
            for i in 0..100 {
                let t = (i * 17 % 100) as f64 / 60.0;
                black_box(many_actors.evaluate(black_box(t), dims));
            }
        })
    });

    c.bench_function("scrub_layout_scene_100frames", |b| {
        b.iter(|| {
            for i in 0..100 {
                let t = (i * 17 % 100) as f64 / 60.0;
                black_box(layout_scene.evaluate(black_box(t), dims));
            }
        })
    });
}

criterion_group!(benches, bench_scrubbing);
criterion_main!(benches);
