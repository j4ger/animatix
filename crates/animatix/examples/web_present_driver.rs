//! Native A/B driver for the web player's render plus canvas-blit path.
//!
//! The separate arm uses the current render submit followed by a surface-format
//! blit submit. The merged arm uses RendererCore::render_scene_and_present,
//! which records that same blit in the render submission. Both
//! arms consume the same pre-evaluated real .amx scenes and render dimensions.
//!
//! This is a headless native analogue: it measures command encoding/submission
//! and explicit GPU completion, not browser scheduling, swapchain presentation,
//! or browser WebGPU overhead. The submit count is source-proven at the two call
//! sites, not sampled from an instrumentation counter.
//!
//! Usage:
//!
//!     cargo run --release -p animatix --example web_present_driver -- [output.json] [frames]
//!
//! The default fixtures are attention.amx (vectors and text) and 01_shapes.amx
//! (vectors, text, SVG, and a raster image). Pixel verification also alternates
//! between these image-free and image-bearing frames while resizing both render
//! and presentation targets.

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Instant;

use animatix::composition::BuildTarget;
use animatix::timeline::effects::FilterBackend;
use animatix::timeline::{BuildOptions, SceneDimensions};
use animatix_render::core::RendererCore;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticCode};
use animatix_syntax::module::ModuleGraph;

#[path = "web_present_driver/verification.rs"]
mod verification;

use verification::verify_resized_image_sequence;

const DEFAULT_FRAMES: usize = 90;
const PAIRED_ROUNDS: usize = 3;
const WARMUP_FRAMES: usize = 12;
const OUTPUT_PATH: &str = "/tmp/animatix-web-present.json";
const FIXTURE_TIMES: [f64; 3] = [0.8, 2.4, 5.6];
const PRESENTATION_SIZES: [(u32, u32); 3] = [(1052, 592), (640, 360), (1280, 720)];

#[derive(Debug)]
struct DriverError(String);

impl std::fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for DriverError {}

fn error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(DriverError(message.into()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Mode {
    Separate,
    Merged,
}

impl Mode {
    fn label(self) -> &'static str {
        match self {
            Self::Separate => "separate",
            Self::Merged => "merged",
        }
    }

    fn source_proven_submits(self) -> u32 {
        match self {
            // RendererCore::render_scene submits once; the immediate blit submits once.
            Self::Separate => 2,
            // RendererCore submits once after encoding the final blit.
            Self::Merged => 1,
        }
    }
}

struct FrameSample {
    time_s: f64,
    scene: animatix::scene::Scene,
}

struct Fixture {
    name: String,
    path: PathBuf,
    dims: SceneDimensions,
    samples: Vec<FrameSample>,
    diagnostics: Vec<String>,
}

#[derive(Default)]
struct Timings {
    cpu_encode_submit_ms: Vec<f64>,
    completion_wait_ms: Vec<f64>,
    completed_wall_ms: Vec<f64>,
}

struct Summary {
    mean: f64,
    p50: f64,
    p90: f64,
    p95: f64,
    max: f64,
}

struct TargetPair {
    _render_texture: wgpu::Texture,
    render_view: wgpu::TextureView,
    present_texture: wgpu::Texture,
    present_view: wgpu::TextureView,
    present_size: [u32; 2],
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let output_path = args.next().map(PathBuf::from).unwrap_or_else(|| PathBuf::from(OUTPUT_PATH));
    let frames = args
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(DEFAULT_FRAMES);
    if !(PAIRED_ROUNDS..=600).contains(&frames) || args.next().is_some() {
        return Err(error("usage: web_present_driver [output.json] [frames: 3..=600]"));
    }

    let fixtures = load_fixtures()?;
    let (device, queue, adapter_info, present_format) = create_device()?;

    let mut cases = Vec::new();
    for fixture in &fixtures {
        for present_size in PRESENTATION_SIZES {
            cases.push(measure_case(
                fixture,
                present_size,
                present_format,
                frames,
                &device,
                &queue,
            )?);
        }
    }

    let verification = verify_resized_image_sequence(&fixtures, present_format, &device, &queue)?;
    let pixel_artifact_path = output_path.with_extension("pixels.bin");
    let pixel_artifact_error = fs::write(&pixel_artifact_path, &verification.raw_pixels)
        .err()
        .map(|cause| cause.to_string());
    let report = serde_json::json!({
        "kind": "animatix-web-present-native-ab",
        "scope": "headless native render and presentation-blit analogue; not browser FPS",
        "attention_input_identity": serde_json::from_str::<serde_json::Value>(include_str!(
            "web_present_driver/fixtures/attention-identity.json"
        ))?,
        "adapter": {
            "name": adapter_info.name,
            "backend": format!("{:?}", adapter_info.backend),
            "device_type": format!("{:?}", adapter_info.device_type),
            "vendor": adapter_info.vendor,
            "device": adapter_info.device,
            "driver": adapter_info.driver,
            "driver_info": adapter_info.driver_info,
        },
        "present_format": format!("{present_format:?}"),
        "frames_per_mode": frames,
        "paired_rounds": PAIRED_ROUNDS,
        "warmup_frames_per_arm_per_round": WARMUP_FRAMES,
        "submit_count_basis": "static source call-site count; no production queue counter",
        "submit_count_evidence": {
            "separate": "RendererCore::render_scene submits render work, then blit_texture_to_format submits the surface blit.",
            "merged": "RendererCore::render_scene_and_present encodes the surface blit in the render submission and submits once.",
        },
        "cases": cases,
        "pixel_verification": {
            "passed": verification.passed,
            "checks": &verification.checks,
            "raw_dump": {
                "path": pixel_artifact_path.display().to_string(),
                "byte_order": format!("{present_format:?}"),
                "byte_length": verification.raw_pixels.len(),
                "layout": &verification.raw_layout,
                "write_error": &pixel_artifact_error,
            },
        },
    });
    fs::write(&output_path, serde_json::to_vec_pretty(&report)?)?;
    tracing::info!(path = %output_path.display(), "wrote native presentation benchmark");
    if !verification.passed {
        return Err(error(format!(
            "presentation parity failed; timings and per-frame comparisons are in {}, raw pixels are in {}{}",
            output_path.display(),
            pixel_artifact_path.display(),
            pixel_artifact_error
                .as_deref()
                .map(|cause| format!(" (raw pixel write failed: {cause})"))
                .unwrap_or_default()
        )));
    }
    Ok(())
}

fn load_fixtures() -> Result<Vec<Fixture>, Box<dyn Error>> {
    let inputs = [
        (
            "attention-vectors-text",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples/web_present_driver/fixtures/attention.amx"),
        ),
        (
            "basic-shapes-image-svg",
            PathBuf::from("crates/animatix/benches/fixtures/amx/basics/01_shapes.amx"),
        ),
    ];
    inputs.into_iter().map(|(name, path)| load_fixture(name, path)).collect()
}

fn load_fixture(name: &str, path: PathBuf) -> Result<Fixture, Box<dyn Error>> {
    let mut graph = ModuleGraph::new();
    let mut program = graph.load_program(&path).map_err(|cause| {
        error(format!("fixture {} failed to parse or load: {cause}", path.display()))
    })?;
    let type_diagnostics = program.typecheck();
    let mut diagnostics = Vec::new();
    collect_diagnostics(&mut diagnostics, "typecheck", type_diagnostics, &path)?;
    let expanded = program.expand_components()?;
    let context = Arc::new(animatix::ExtensionRegistry::new());
    let report = BuildTarget::build_with_options(
        &expanded.statements,
        &program.namespaces,
        Some(&path),
        BuildOptions::default()
            .with_context(context)
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    collect_diagnostics(&mut diagnostics, "build", report.diagnostics, &path)?;
    let timeline = match report.output {
        BuildTarget::SingleScene(timeline) => timeline,
        BuildTarget::MultiScene(_) => {
            return Err(error(format!("fixture {} must remain a single scene", path.display())));
        },
    };
    if timeline.needs_filter_backend() {
        return Err(error(format!(
            "fixture {} uses Filter or Glass; this benchmark intentionally covers the plain-frame path",
            path.display()
        )));
    }
    let (width, height) = timeline.resolution().unwrap_or((1280, 720));
    let dims = SceneDimensions { width, height };
    let mut samples = Vec::with_capacity(FIXTURE_TIMES.len());
    for time_s in FIXTURE_TIMES {
        let mut backend: Option<&mut dyn FilterBackend> = None;
        let scene = timeline.evaluate_for_presentation(time_s, dims, &mut backend);
        collect_runtime_diagnostics(&mut diagnostics, &timeline, time_s, &path)?;
        samples.push(FrameSample { time_s, scene });
    }
    Ok(Fixture {
        name: name.to_string(),
        path,
        dims,
        samples,
        diagnostics,
    })
}

fn collect_diagnostics(
    output: &mut Vec<String>,
    phase: &str,
    diagnostics: Vec<Diagnostic>,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    for diagnostic in diagnostics {
        if diagnostic.is_error() {
            return Err(error(format!(
                "fixture {} has {phase} error: {}",
                path.display(),
                diagnostic.message
            )));
        }
        output.push(format!(
            "{phase} [{:?}/{:?}]: {}",
            diagnostic.severity, diagnostic.code, diagnostic.message
        ));
    }
    Ok(())
}

fn collect_runtime_diagnostics(
    output: &mut Vec<String>,
    timeline: &animatix::timeline::Timeline,
    time_s: f64,
    path: &Path,
) -> Result<(), Box<dyn Error>> {
    for diagnostic in timeline.runtime_diagnostics() {
        if diagnostic.is_error() || diagnostic.code == DiagnosticCode::RenderFailure {
            return Err(error(format!(
                "fixture {} failed during render at {time_s:.3}s: {}",
                path.display(),
                diagnostic.message
            )));
        }
        output.push(format!(
            "render @ {time_s:.3}s [{:?}/{:?}]: {}",
            diagnostic.severity, diagnostic.code, diagnostic.message
        ));
    }
    Ok(())
}

fn create_device()
-> Result<(wgpu::Device, wgpu::Queue, wgpu::AdapterInfo, wgpu::TextureFormat), Box<dyn Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))?;
    let adapter_info = adapter.get_info();
    let present_format = choose_present_format(&adapter);
    let required_limits = wgpu::Limits::default().using_resolution(adapter.limits());
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Animatix Web Present A/B"),
        required_features: wgpu::Features::empty(),
        required_limits,
        memory_hints: Default::default(),
        ..Default::default()
    }))?;
    Ok((device, queue, adapter_info, present_format))
}

fn choose_present_format(adapter: &wgpu::Adapter) -> wgpu::TextureFormat {
    let bgra = wgpu::TextureFormat::Bgra8Unorm;
    let features = adapter.get_texture_format_features(bgra);
    if features
        .allowed_usages
        .contains(wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC)
    {
        bgra
    } else {
        wgpu::TextureFormat::Rgba8Unorm
    }
}

fn measure_case(
    fixture: &Fixture,
    present_size: (u32, u32),
    present_format: wgpu::TextureFormat,
    total_frames: usize,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<serde_json::Value, Box<dyn Error>> {
    let mut timings = HashMap::<Mode, Timings>::new();
    let frames_per_round = round_sizes(total_frames);
    for (round, frame_count) in frames_per_round.into_iter().enumerate() {
        let modes = if round % 2 == 0 {
            [Mode::Separate, Mode::Merged]
        } else {
            [Mode::Merged, Mode::Separate]
        };
        for mode in modes {
            let run = measure_arm(
                fixture,
                present_size,
                present_format,
                frame_count,
                mode,
                device,
                queue,
            )?;
            let accumulated = timings.entry(mode).or_default();
            accumulated.cpu_encode_submit_ms.extend(run.cpu_encode_submit_ms);
            accumulated.completion_wait_ms.extend(run.completion_wait_ms);
            accumulated.completed_wall_ms.extend(run.completed_wall_ms);
        }
    }

    let scale = 0.85f32;
    let (raster_width, raster_height) = raster_dims(fixture.dims, scale);
    let raster = [raster_width, raster_height];
    let summaries = |mode: Mode| -> Result<serde_json::Value, Box<dyn Error>> {
        let samples = timings
            .get(&mode)
            .ok_or_else(|| error(format!("missing {} timings", mode.label())))?;
        Ok(serde_json::json!({
            "submit_calls_per_frame_source_proven": mode.source_proven_submits(),
            "cpu_encode_submit_ms": summarize(&samples.cpu_encode_submit_ms)?,
            "completion_wait_ms": summarize(&samples.completion_wait_ms)?,
            "completed_wall_ms": summarize(&samples.completed_wall_ms)?,
        }))
    };
    Ok(serde_json::json!({
        "fixture": fixture.name,
        "source": fixture.path.display().to_string(),
        "scene": [fixture.dims.width, fixture.dims.height],
        "raster_scale": scale,
        "raster": raster,
        "presentation": [present_size.0, present_size.1],
        "format": format!("{present_format:?}"),
        "fixture_diagnostics": fixture.diagnostics,
        "runs": {
            "separate": summaries(Mode::Separate)?,
            "merged": summaries(Mode::Merged)?,
        }
    }))
}

fn round_sizes(total_frames: usize) -> [usize; PAIRED_ROUNDS] {
    let base = total_frames / PAIRED_ROUNDS;
    let remainder = total_frames % PAIRED_ROUNDS;
    std::array::from_fn(|round| base + if round < remainder { 1 } else { 0 })
}

fn measure_arm(
    fixture: &Fixture,
    present_size: (u32, u32),
    present_format: wgpu::TextureFormat,
    frames: usize,
    mode: Mode,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<Timings, Box<dyn Error>> {
    let scale = 0.85f32;
    let raster = raster_dims(fixture.dims, scale);
    let targets = create_target_pair(device, raster, present_size, present_format);
    let mut core = RendererCore::new(device, queue)?;
    let mut scratch = animatix::scene::Scene::new();
    for index in 0..WARMUP_FRAMES {
        let sample = &fixture.samples[index % fixture.samples.len()];
        draw_frame(
            &mut core,
            &mut scratch,
            sample,
            scale,
            raster,
            present_size,
            present_format,
            &targets,
            mode,
            device,
            queue,
        )?;
        wait_for_submitted_work(device, queue)?;
    }

    let mut timings = Timings {
        cpu_encode_submit_ms: Vec::with_capacity(frames),
        completion_wait_ms: Vec::with_capacity(frames),
        completed_wall_ms: Vec::with_capacity(frames),
    };
    for index in 0..frames {
        let sample = &fixture.samples[index % fixture.samples.len()];
        let frame_start = Instant::now();
        draw_frame(
            &mut core,
            &mut scratch,
            sample,
            scale,
            raster,
            present_size,
            present_format,
            &targets,
            mode,
            device,
            queue,
        )?;
        let encoded_at = Instant::now();
        wait_for_submitted_work(device, queue)?;
        let completed_at = Instant::now();
        timings
            .cpu_encode_submit_ms
            .push(encoded_at.duration_since(frame_start).as_secs_f64() * 1000.0);
        timings
            .completion_wait_ms
            .push(completed_at.duration_since(encoded_at).as_secs_f64() * 1000.0);
        timings
            .completed_wall_ms
            .push(completed_at.duration_since(frame_start).as_secs_f64() * 1000.0);
    }
    Ok(timings)
}

fn draw_frame(
    core: &mut RendererCore,
    scratch: &mut animatix::scene::Scene,
    sample: &FrameSample,
    scale: f32,
    raster_size: (u32, u32),
    present_size: (u32, u32),
    present_format: wgpu::TextureFormat,
    targets: &TargetPair,
    mode: Mode,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(), Box<dyn Error>> {
    let _time_s = std::hint::black_box(sample.time_s);
    scratch.reset();
    scratch.append(&sample.scene, Some(kurbo::Affine::scale(f64::from(scale))));
    let encoded_scene = std::hint::black_box(&*scratch);
    let raster_dims = [raster_size.0, raster_size.1];
    let output_dims = [present_size.0, present_size.1];
    match mode {
        Mode::Separate => {
            core.render_scene(
                device,
                queue,
                &targets.render_view,
                raster_dims[0],
                raster_dims[1],
                encoded_scene,
            )?;
            core.blit_texture_to_format(
                device,
                queue,
                &targets.render_view,
                &targets.present_view,
                output_dims[0],
                output_dims[1],
                1.0,
                present_format,
            );
        },
        Mode::Merged => {
            core.render_scene_and_present(
                device,
                queue,
                &targets.render_view,
                raster_dims,
                encoded_scene,
                &targets.present_view,
                output_dims,
                present_format,
            )?;
        },
    }
    Ok(())
}

fn raster_dims(dims: SceneDimensions, scale: f32) -> (u32, u32) {
    (
        ((dims.width as f32 * scale).round() as u32).max(1),
        ((dims.height as f32 * scale).round() as u32).max(1),
    )
}

fn create_target_pair(
    device: &wgpu::Device,
    raster_size: (u32, u32),
    present_size: (u32, u32),
    present_format: wgpu::TextureFormat,
) -> TargetPair {
    let render_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Animatix Web Present Render Target"),
        size: wgpu::Extent3d {
            width: raster_size.0,
            height: raster_size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let render_view = render_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let present_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Animatix Web Present Target"),
        size: wgpu::Extent3d {
            width: present_size.0,
            height: present_size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: present_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let present_view = present_texture.create_view(&wgpu::TextureViewDescriptor::default());
    TargetPair {
        _render_texture: render_texture,
        render_view,
        present_texture,
        present_view,
        present_size: [present_size.0, present_size.1],
    }
}

fn wait_for_submitted_work(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> Result<(), Box<dyn Error>> {
    let (sender, receiver) = mpsc::channel();
    queue.on_submitted_work_done(move || {
        let _ = sender.send(());
    });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    })?;
    receiver.recv()?;
    Ok(())
}

fn summarize(values: &[f64]) -> Result<serde_json::Value, Box<dyn Error>> {
    if values.is_empty() {
        return Err(error("cannot summarize an empty timing set"));
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let pick = |percentile: f64| -> f64 {
        let index = ((sorted.len() - 1) as f64 * percentile).round() as usize;
        sorted[index.min(sorted.len() - 1)]
    };
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let summary = Summary {
        mean,
        p50: pick(0.50),
        p90: pick(0.90),
        p95: pick(0.95),
        max: pick(1.0),
    };
    Ok(serde_json::json!({
        "frames": values.len(),
        "mean": summary.mean,
        "p50": summary.p50,
        "p90": summary.p90,
        "p95": summary.p95,
        "max": summary.max,
    }))
}
