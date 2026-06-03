use animatix::timeline::{SceneDimensions, Timeline};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

mod common;

fn build_visible_scene(actor_count: usize) -> Timeline {
    // All actors visible on screen
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
    common::parse_timeline(&source)
}

fn build_mixed_visibility_scene(visible: usize, offscreen: usize) -> Timeline {
    // Some visible, some off-screen
    let mut source = String::from(r#"config { colorscheme: "editorial-dark" }

#0s
"#);
    for i in 0..visible {
        source.push_str(&format!(
            "v{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            100 + (i % 20) * 90,
            100 + (i / 20) * 90
        ));
    }
    for i in 0..offscreen {
        source.push_str(&format!(
            "o{i}: Rect, size: (50, 50), color: accent.primary, at: ({}, {})\n",
            3000 + i * 60,  // far off-screen
            3000
        ));
    }
    common::parse_timeline(&source)
}

fn bench_visibility(c: &mut Criterion) {
    let dims = SceneDimensions { width: 1920, height: 1080 };

    // Baseline: all visible
    let all_visible = build_visible_scene(100);
    c.bench_function("visible_100_actors", |b| {
        b.iter(|| {
            let mut fb = None;
            black_box(all_visible.evaluate_with_debug(
                black_box(0.5),
                dims,
                animatix::timeline::DebugRenderOptions { draw_bounds: true, compute_hit_regions: false },
                &mut fb,
            ));
        })
    });

    // Mixed: 50 visible, 50 off-screen
    let mixed = build_mixed_visibility_scene(50, 50);
    c.bench_function("mixed_50visible_50offscreen", |b| {
        b.iter(|| {
            let mut fb = None;
            black_box(mixed.evaluate_with_debug(
                black_box(0.5),
                dims,
                animatix::timeline::DebugRenderOptions { draw_bounds: true, compute_hit_regions: false },
                &mut fb,
            ));
        })
    });

    // All off-screen
    let all_offscreen = build_mixed_visibility_scene(0, 100);
    c.bench_function("offscreen_100_actors", |b| {
        b.iter(|| {
            let mut fb = None;
            black_box(all_offscreen.evaluate_with_debug(
                black_box(0.5),
                dims,
                animatix::timeline::DebugRenderOptions { draw_bounds: true, compute_hit_regions: false },
                &mut fb,
            ));
        })
    });
}

criterion_group!(benches, bench_visibility);
criterion_main!(benches);
