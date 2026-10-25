use animatix::timeline::{Timeline, TrackAccessor};
use criterion::{Criterion, criterion_group, criterion_main};

mod common;

fn build_many_actors_scene() -> Timeline {
    let mut source = String::from(
        r#"config { colorscheme: "editorial-dark" }

#0s
"#,
    );
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

    // `full_evaluate` and `frame_env_only` were removed 2026-10-26: they
    // duplicate `stage__eval_total` / `stage__build_frame_env` and the
    // `full_N` / `env_N` benches in scaling_breakdown, which measure the same
    // paths through the shared stage tracer. This file keeps the one probe no
    // other suite isolates: raw per-actor property sampling.
    c.bench_function("sample_all_tracks", |b| {
        let time_ms = 500;
        b.iter(|| {
            for (_, id) in timeline.tracks().iter() {
                let geometry = timeline.world().geometry(id);
                let style = timeline.world().style(id);
                let _ = geometry.size.get(time_ms, [50.0, 50.0]);
                let _ = geometry.position.get(time_ms, [0.0, 0.0]);
                let _ = style.color.get(time_ms, [1.0, 1.0, 1.0, 1.0]);
                let _ = style.opacity.get(time_ms, 1.0);
                let _ = geometry.rotation.get(time_ms, 0.0);
            }
        })
    });
}

criterion_group!(benches, bench_cost_breakdown);
criterion_main!(benches);
