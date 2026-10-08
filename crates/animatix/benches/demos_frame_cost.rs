//! Steady-state per-frame cost of every example demo, GPU-rendered offscreen.
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
//! `plugin_pulse.amx` needs a native plugin library and is skipped.

use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix::timeline::SceneDimensions;
use animatix_render::offscreen::{OffscreenRenderer, RenderedFrame};
use animatix_syntax::module::ModuleGraph;
use criterion::{Criterion, SamplingMode, criterion_group, criterion_main};

const DIMS: SceneDimensions = SceneDimensions {
    width: 1280,
    height: 720,
};
const FRAME_TIME_S: f64 = 1.5;

/// Walk `examples/` the same way check_examples/render_smoke do: every `.amx`
/// outside `lib/` and `scenes/`; multi-file projects via their `main.amx`.
fn collect_demos(root: &Path) -> Vec<PathBuf> {
    let mut demos = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("examples dir")
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .collect();
        entries.sort();
        for path in entries {
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_some_and(|e| e == "amx")
                && !path.components().any(|c| c.as_os_str() == "lib")
                && !path.components().any(|c| c.as_os_str() == "scenes")
            {
                let parent_has_main = path.parent().is_some_and(|p| p.join("main.amx").is_file());
                if parent_has_main && path.file_name().is_some_and(|f| f != "main.amx") {
                    continue;
                }
                // plugin_pulse needs a native plugin library; render cost
                // without it is not representative.
                if path.file_name().is_some_and(|f| f == "plugin_pulse.amx") {
                    continue;
                }
                demos.push(path);
            }
        }
    }
    demos
}

/// Build a demo through the CLI's own load path (modules + typecheck +
/// component expansion + scene grouping).
fn build_target(path: &Path) -> BuildTarget {
    let mut graph = ModuleGraph::new();
    let mut program = graph.load_program(path).expect("demo loads");
    program.typecheck();
    let mut expansion_errors = Vec::new();
    let ast = program.expand_components(&mut expansion_errors);
    assert!(
        expansion_errors.is_empty(),
        "{}: expansion errors {expansion_errors:?}",
        path.display()
    );
    let report = BuildTarget::from_ast(&ast, &program.namespaces, Some(path));
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

fn bench_demo_frame_cost(c: &mut Criterion) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let demos = collect_demos(&root);
    assert!(demos.len() > 30, "expected the full example corpus");

    let mut renderer =
        OffscreenRenderer::new().expect("offscreen renderer initializes (GPU required)");

    let mut group = c.benchmark_group("demo_frame");
    group.sampling_mode(SamplingMode::Linear);
    group.sample_size(15);

    for path in &demos {
        let name = path
            .strip_prefix(&root)
            .expect("under examples")
            .with_extension("")
            .to_string_lossy()
            .to_string();
        let target = build_target(path);
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

criterion_group!(benches, bench_demo_frame_cost);
criterion_main!(benches);
