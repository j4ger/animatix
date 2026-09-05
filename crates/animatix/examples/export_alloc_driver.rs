//! Steady-state allocation + peak-heap driver for the **export/render path**
//! (PF-6 / `docs/performance_evaluation.md` §5 P3).
//!
//! `alloc_driver` measures the timeline `evaluate` loop on a synthetic
//! 60-actor scene; this driver measures the *whole export frame pipeline* on
//! a real `.amx` project — module load, `Timeline` build, per-frame
//! evaluation, vello encoding, wgpu render, and CPU readback — through
//! [`OffscreenRenderer`], which is exactly what `animatix image` / video
//! export drive. GPU-side memory lives in the driver, not the Rust heap, so
//! dhat sees only the CPU churn we can fix.
//!
//! Usage:
//!
//! ```text
//! cargo build --profile profiling -p animatix --example export_alloc_driver
//! target/profiling/examples/export_alloc_driver [path/to/project.amx] [frames]
//! ```
//!
//! Defaults to `examples/gallery/dashboard_story.amx` (multi-file imports,
//! dynamic layout, components, charts) at 30 fps for 90 frames. Output:
//! steady-state per-frame blocks/bytes (post-settle) plus the peak live heap
//! over the measured window — the PF-6 "peak RSS on real scenarios" proxy.

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use animatix::composition::BuildTarget;
use animatix::renderer::OffscreenRenderer;
use animatix::timeline::{SceneDimensions, Timeline};
use animatix_syntax::module::ModuleGraph;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .unwrap_or_else(|| "examples/gallery/dashboard_story.amx".to_string());
    let frames: u32 = args.next().and_then(|v| v.parse().ok()).unwrap_or(90);
    let fps: f64 = 30.0;
    let path = std::path::PathBuf::from(path);

    // ── Load + build (outside the measured window; dhat not installed yet) ──
    let mut graph = ModuleGraph::new();
    let mut program = graph.load_program(&path).expect("module load failed");
    let type_diagnostics = program.typecheck();
    for d in &type_diagnostics {
        eprintln!("typecheck: {}", d.message);
    }
    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    for e in &expansion_errors {
        eprintln!("expansion: {e}");
    }
    let context = std::sync::Arc::new(animatix::ExtensionRegistry::new());
    let report =
        BuildTarget::from_ast_with_context(&expanded, &program.namespaces, Some(&path), context);
    for d in &report.diagnostics {
        eprintln!("build: {}", d.message);
    }
    let duration_s = report.output.duration_s().max(0.001);
    let (timelines, composition): (Vec<Timeline>, Option<animatix::composition::Composition>) =
        match &report.output {
            BuildTarget::SingleScene(t) => (vec![t.clone()], None),
            BuildTarget::MultiScene(c) => {
                (c.scenes.values().map(|s| s.timeline.clone()).collect(), Some(c.clone()))
            },
        };
    println!(
        "loaded {} (duration {duration_s:.2}s, {} timeline(s), {} diagnostics)",
        path.display(),
        timelines.len(),
        report.diagnostics.len()
    );

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

    // ── GPU renderer + settle frames (caches, shader pipelines, layout) ──
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
    for i in 0..30u32 {
        render_frame(&mut renderer, ((i % 30) as f64) / fps);
    }

    // ── Measured window ──
    let profiler = dhat::Profiler::builder().file_name("dhat-export.json").build();
    let start = dhat::HeapStats::get();
    for i in 0..frames {
        render_frame(&mut renderer, (i as f64) / fps);
    }
    let end = dhat::HeapStats::get();
    drop(profiler);

    let per_frame_u64 = |a: u64, b: u64| -> f64 { a.saturating_sub(b) as f64 / frames as f64 };
    println!(
        "steady-state export-frame allocations ({frames} frames, {} @ {}×{}):",
        path.display(),
        dims.width,
        dims.height
    );
    println!("  blocks/frame: {:>10.1}", per_frame_u64(end.total_blocks, start.total_blocks));
    println!("  bytes/frame:  {:>10.1}", per_frame_u64(end.total_bytes, start.total_bytes));
    println!("  peak live:    {:>10} bytes ({} blocks)", end.max_bytes, end.max_blocks);
    println!("wrote dhat-export.json (site-level ranking; inspect with the dhat viewer)");
}
