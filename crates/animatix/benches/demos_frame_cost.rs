//! Steady-state per-frame cost of fixed workloads, GPU-rendered offscreen.
//!
//! This is the 30fps-preview gate: a demo previews smoothly when its
//! per-frame cost stays under ~33 ms. Each sample renders one frame through
//! [`OffscreenRenderer::render_timeline`] (evaluate → vello rasterize →
//! effect chains → readback) — the same pipeline the export path uses. The
//! readback adds ~1-2 ms versus the window-surface path, so numbers here are
//! slightly conservative for preview purposes.
//!
//! Demos load through [`ModuleGraph`] + [`BuildTarget`] exactly like the CLI,
//! so modules, typed components, colorschemes, and cross-file scenes all
//! resolve. Multi-scene demos measure the scene active at the probe time
//! (transition blending is a brief 2× overlap, not the steady state).

use std::path::Path;

use animatix::composition::BuildTarget;
use animatix::timeline::{BuildOptions, SceneDimensions};
use animatix_render::offscreen::{OffscreenRenderer, RenderedFrame};
use animatix_syntax::module::ModuleGraph;
use criterion::{Criterion, SamplingMode};

const DIMS: SceneDimensions = SceneDimensions {
    width: 1280,
    height: 720,
};
const FRAME_TIME_S: f64 = 1.5;

/// Fixed GPU workloads with durable Criterion IDs relative to the fixture root.
const WORKLOADS: &[&str] = &[
    "basics/01_shapes.amx",
    "data/23_plot_kinds.amx",
    "gallery/dashboard_story.amx",
    "gallery/sorting_theatre.amx",
    "generation/28_generation_reactive.amx",
    "projects/fft_explain.amx",
];

/// Build a demo through the CLI's own load path (modules + typecheck +
/// component expansion + scene grouping).
fn build_target(path: &Path) -> BuildTarget {
    let mut graph = ModuleGraph::new();
    let mut program = graph.load_program(path).expect("demo loads");
    program.typecheck();
    let expanded = program.expand_components().expect("component expansion");
    let report = BuildTarget::build_with_options(
        &expanded.statements,
        &program.namespaces,
        Some(path),
        BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    report.output
}

/// Render one frame at global time `t`, resolving the active scene for
/// multi-scene compositions.
fn render_frame(target: &BuildTarget, renderer: &mut OffscreenRenderer, t: f64) -> RenderedFrame {
    match target {
        BuildTarget::SingleScene(timeline) => {
            renderer.render_timeline(timeline, t, DIMS).expect("frame")
        },
        BuildTarget::MultiScene(composition) => {
            // The scene whose [start, start + duration) window contains `t`;
            // fall back to the first declared scene.
            let mut active: Option<(&str, f64)> = None;
            for (name, start) in &composition.scene_start_times {
                if *start <= t {
                    active = Some((name.as_str(), *start));
                }
            }
            let (name, start) = active.unwrap_or_else(|| {
                let (name, start) = composition
                    .scene_start_times
                    .iter()
                    .next()
                    .expect("a multi-scene composition has at least one scene");
                (name.as_str(), *start)
            });
            let scene = composition.scenes.get(name).expect("scene");
            let local_t = (t - start).clamp(0.0, scene.duration_s);
            renderer.render_timeline(&scene.timeline, local_t, DIMS).expect("frame")
        },
    }
}

pub(crate) fn bench_demo_frame_cost(c: &mut Criterion) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("benches/fixtures/amx");

    let mut renderer =
        OffscreenRenderer::new().expect("offscreen renderer initializes (GPU required)");

    let mut group = c.benchmark_group("demo_frame");
    group.sampling_mode(SamplingMode::Linear);
    group.sample_size(15);

    for relative in WORKLOADS {
        let path = root.join(relative);
        let name = Path::new(relative).with_extension("").to_string_lossy().to_string();
        let target = build_target(&path);
        // Warm the vello/effect pipelines once so samples measure steady state.
        let _ = render_frame(&target, &mut renderer, 0.2);
        group.bench_function(name.clone(), |b| {
            b.iter(|| std::hint::black_box(render_frame(&target, &mut renderer, FRAME_TIME_S)))
        });
        // Env-gated stage breakdown for this demo (one warm frame).
        if std::env::var_os("ANIMATIX_STAGE_REPORT").is_some() {
            let _ = animatix::perf::take_measurements();
            let _ = render_frame(&target, &mut renderer, FRAME_TIME_S);
            let mut stages: Vec<(String, std::time::Duration)> =
                animatix::perf::take_measurements();
            stages.sort_by_key(|b| std::cmp::Reverse(b.1));
            for (stage, dur) in stages {
                eprintln!("[stage] {name} {stage} = {:.3} ms", dur.as_secs_f64() * 1000.0);
            }
        }
    }
    group.finish();
}
