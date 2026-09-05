//! Layer-3 throughput driver for the **export/render path** (PF-7,
//! `docs/performance_evaluation.md` §5 P4).
//!
//! `export_alloc_driver` ranks the export path by *allocations*; this driver
//! ranks it by *time*: the same real-project pipeline (module load → build →
//! per-frame evaluate → vello encode → wgpu render → CPU readback) with the
//! PF-8 shared stage tracer drained per frame, so `sample` /
//! `build_frame_env` / `rasterize` and the unaccounted remainder (readback
//! wait, encode glue) split every frame wall-clock.
//!
//! Usage:
//!
//! ```text
//! cargo build --release -p animatix --example export_perf_driver
//! target/release/examples/export_perf_driver [path/to/project.amx] [frames]
//! ```
//!
//! Prints total FPS, frame wall-time percentiles, and per-stage means —
//! the PF-7 baseline for "is export GPU-bound or CPU-bound".

use animatix::composition::BuildTarget;
use animatix::perf;
use animatix::renderer::OffscreenRenderer;
use animatix::timeline::{SceneDimensions, Timeline};
use animatix_syntax::module::ModuleGraph;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .unwrap_or_else(|| "examples/gallery/dashboard_story.amx".to_string());
    let frames: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(120);
    let fps: f64 = 30.0;
    let path = std::path::PathBuf::from(path);

    // ── Load + build ──
    let mut graph = ModuleGraph::new();
    let mut program = graph.load_program(&path).expect("module load failed");
    let _ = program.typecheck();
    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    let context = std::sync::Arc::new(animatix::ExtensionRegistry::new());
    let report =
        BuildTarget::from_ast_with_context(&expanded, &program.namespaces, Some(&path), context);
    let duration_s = report.output.duration_s().max(0.001);
    let (timelines, composition): (Vec<Timeline>, Option<animatix::composition::Composition>) =
        match &report.output {
            BuildTarget::SingleScene(t) => (vec![t.clone()], None),
            BuildTarget::MultiScene(c) => {
                ((c.scenes.values().map(|s| s.timeline.clone()).collect()), Some(c.clone()))
            },
        };
    let dims = timelines
        .first()
        .and_then(|t| t.resolution())
        .map(|(w, h)| SceneDimensions {
            width: w,
            height: h,
        })
        .unwrap_or(SceneDimensions {
            width: 1280,
            height: 720,
        });

    let mut renderer = OffscreenRenderer::new().expect("offscreen renderer");
    let render_frame = |renderer: &mut OffscreenRenderer, t: f64| {
        if let Some(composition) = &composition {
            let (scene_name, local_time_s, transition_blend) = composition.evaluate(t);
            if let Some(blend) = transition_blend {
                let from = &composition.scenes[&blend.from_scene].timeline;
                let to = &composition.scenes[&blend.to_scene].timeline;
                std::hint::black_box(
                    renderer
                        .render_transition(
                            from,
                            blend.from_local,
                            to,
                            blend.to_local,
                            blend.progress as f32,
                            blend.id.clone(),
                            blend.easing,
                            dims,
                            Default::default(),
                        )
                        .expect("settle transition frame"),
                );
                return;
            }
            let scene = &composition.scenes[&scene_name].timeline;
            std::hint::black_box(
                renderer.render_timeline(scene, local_time_s, dims).expect("settle frame"),
            );
        } else {
            std::hint::black_box(
                renderer
                    .render_timeline(&timelines[0], t.min(duration_s), dims)
                    .expect("settle frame"),
            );
        }
    };

    // ── Settle: shader pipelines, caches, layout ──
    for i in 0..30u32 {
        render_frame(&mut renderer, ((i % 30) as f64) / fps);
    }

    // ── Measured window ──
    let mut stage_sum: std::collections::HashMap<String, std::time::Duration> =
        std::collections::HashMap::new();
    let mut stage_count: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut frame_walls: Vec<std::time::Duration> = Vec::with_capacity(frames as usize);

    for i in 0..frames {
        let t = (i as f64 / fps) % duration_s.max(0.001);
        let wall = std::time::Instant::now();
        render_frame(&mut renderer, t);
        let wall = wall.elapsed();
        frame_walls.push(wall);
        for (name, dur) in perf::take_measurements() {
            *stage_sum.entry(name.clone()).or_default() += dur;
            *stage_count.entry(name).or_default() += 1;
        }
    }
    // Drain once more so nothing leaks into the next run.
    perf::take_measurements();

    frame_walls.sort();
    let total: std::time::Duration = frame_walls.iter().sum();
    let p = |q: f64| -> f64 {
        frame_walls[((frame_walls.len() - 1) as f64 * q) as usize].as_secs_f64() * 1000.0
    };
    println!(
        "export throughput ({frames} frames, {} @ {}×{}):",
        path.display(),
        dims.width,
        dims.height
    );
    println!(
        "  fps: {:>8.1}   frame ms: p50 {:>7.2}  p90 {:>7.2}  mean {:>7.2}",
        frames as f64 / total.as_secs_f64(),
        p(0.50),
        p(0.90),
        total.as_secs_f64() * 1000.0 / frames as f64,
    );
    let mut rows: Vec<(String, f64, u64)> = stage_sum
        .into_iter()
        .map(|(name, sum)| {
            let count = stage_count[&name];
            (name, sum.as_secs_f64() * 1000.0 / frames as f64, count / frames as u64)
        })
        .collect();
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    println!("  stage breakdown (mean ms per frame, calls per frame):");
    let mut accounted = 0.0;
    for (name, ms, calls) in &rows {
        accounted += ms;
        println!("    {name:<22} {ms:>8.3} ms  ×{calls}");
    }
    let mean_frame = total.as_secs_f64() * 1000.0 / frames as f64;
    let unaccounted = mean_frame - accounted;
    println!(
        "    {:<22} {unaccounted:>8.3} ms  (unaccounted: readback wait, encode glue)",
        "unaccounted"
    );
}
