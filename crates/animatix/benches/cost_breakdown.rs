use animatix::timeline::{SceneDimensions, Timeline, TrackAccessor};
use criterion::{Criterion, black_box, criterion_group, criterion_main};

mod common;

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

fn bench_cost_breakdown(c: &mut Criterion) {
    let timeline = build_many_actors_scene();
    let dims = SceneDimensions { width: 1920, height: 1080 };

    // Baseline: full evaluate
    c.bench_function("full_evaluate", |b| {
        b.iter(|| {
            black_box(timeline.evaluate(black_box(0.5), dims));
        })
    });

    // Just frame env setup (no rendering)
    c.bench_function("frame_env_only", |b| {
        b.iter(|| {
            let env = timeline.build_frame_env(500, dims, &std::collections::HashMap::new());
            black_box(env);
        })
    });

    // Property sampling for all tracks
    c.bench_function("sample_all_tracks", |b| {
        let time_ms = 500;
        b.iter(|| {
            for track in timeline.tracks().values() {
                let _ = track.geometry.size.get(time_ms, [50.0, 50.0]);
                let _ = track.geometry.position.get(time_ms, [0.0, 0.0]);
                let _ = track.style.color.get(time_ms, [1.0, 1.0, 1.0, 1.0]);
                let _ = track.style.opacity.get(time_ms, 1.0);
                let _ = track.geometry.rotation.get(time_ms, 0.0);
            }
        })
    });
}

criterion_group!(benches, bench_cost_breakdown);
criterion_main!(benches);
