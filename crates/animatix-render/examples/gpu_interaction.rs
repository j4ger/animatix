//! Native GPU interaction benchmark for the steady-state filter path.
//!
//! Runs one and eight independent Blur scopes over 64 px regions on a 720p
//! target. `encode_cpu_ms` covers scene/filter encoding and queue submission;
//! `sync_wait_ms` is the explicit wait for all queued GPU work; `total_ms` is
//! their sum. Pipeline creation and scratch/pending-texture allocation are
//! kept in the warmup phase.

use std::error::Error;
use std::fs;
use std::sync::mpsc;
use std::time::Instant;

use animatix::timeline::SceneDimensions;
use animatix::timeline::effects::{
    EffectChain, EffectId, EffectInstance, EffectParamValue, EffectParams, EffectRegion,
    FilterBackend,
};
use animatix_render::core::RendererCore;
use animatix_render::filter_backend::GpuFilterBackend;
use animatix_render::offscreen::drain_pending_layers;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const ROI_SIZE: u32 = 64;
const BLUR_RADIUS: f32 = 8.0;
const WARMUP_FRAMES: usize = 10;

struct Measurement {
    scopes: usize,
    encode_cpu_ms: Vec<f64>,
    sync_wait_ms: Vec<f64>,
    total_ms: Vec<f64>,
}

#[derive(Clone, Copy)]
struct Summary {
    mean: f64,
    median: f64,
    p95: f64,
}

#[derive(Debug)]
struct MessageError(String);

impl std::fmt::Display for MessageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for MessageError {}

fn boxed_message(message: impl ToString) -> Box<dyn Error> {
    Box::new(MessageError(message.to_string()))
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let output_path = args.next().unwrap_or_else(|| "gpu_interaction.json".to_string());
    let frames = args.next().map(|value| value.parse::<usize>()).transpose()?.unwrap_or(60);
    if !(1..=120).contains(&frames) || args.next().is_some() {
        return Err(boxed_message("usage: gpu_interaction [output.json] [frames: 1..=120]"));
    }

    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::default(),
        compatible_surface: None,
        force_fallback_adapter: false,
        apply_limit_buckets: false,
    }))?;
    let adapter_info = adapter.get_info();
    let required_limits = wgpu::Limits::default().using_resolution(adapter.limits());
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("Animatix GPU Interaction Benchmark"),
        required_features: wgpu::Features::empty(),
        required_limits,
        memory_hints: Default::default(),
        ..Default::default()
    }))?;

    let dimensions = SceneDimensions {
        width: WIDTH,
        height: HEIGHT,
    };
    let output_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Animatix GPU Interaction Output"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut core = RendererCore::new(&device, &queue).map_err(boxed_message)?;

    let main_scene = make_main_scene();
    let blur_chain = make_blur_chain();
    let mut measurements = Vec::with_capacity(2);
    for scopes in [1, 8] {
        let scopes = make_scopes(scopes);
        let mut backend = GpuFilterBackend::new(device.clone(), queue.clone(), dimensions)
            .map_err(boxed_message)?;

        for _ in 0..WARMUP_FRAMES {
            encode_frame(
                &mut core,
                &mut backend,
                &device,
                &queue,
                dimensions,
                &main_scene,
                &scopes,
                &blur_chain,
                &output_texture,
                &output_view,
            )
            .map_err(boxed_message)?;
            wait_for_submitted_work(&device, &queue)?;
        }

        let mut measurement = Measurement {
            scopes: scopes.len(),
            encode_cpu_ms: Vec::with_capacity(frames),
            sync_wait_ms: Vec::with_capacity(frames),
            total_ms: Vec::with_capacity(frames),
        };
        for _ in 0..frames {
            let frame_start = Instant::now();
            encode_frame(
                &mut core,
                &mut backend,
                &device,
                &queue,
                dimensions,
                &main_scene,
                &scopes,
                &blur_chain,
                &output_texture,
                &output_view,
            )
            .map_err(boxed_message)?;
            let encoded_at = Instant::now();
            wait_for_submitted_work(&device, &queue)?;
            let completed_at = Instant::now();

            measurement
                .encode_cpu_ms
                .push(encoded_at.duration_since(frame_start).as_secs_f64() * 1000.0);
            measurement
                .sync_wait_ms
                .push(completed_at.duration_since(encoded_at).as_secs_f64() * 1000.0);
            measurement
                .total_ms
                .push(completed_at.duration_since(frame_start).as_secs_f64() * 1000.0);
        }
        measurements.push(measurement);
    }

    let json = format_json(&adapter_info, frames, &measurements);
    fs::write(output_path, json)?;
    Ok(())
}

fn make_blur_chain() -> EffectChain {
    EffectChain {
        instances: vec![EffectInstance {
            id: EffectId::new("Blur"),
            enabled: true,
            params: EffectParams {
                values: vec![EffectParamValue::F32(BLUR_RADIUS)],
            },
        }],
        time_ms: 0.0,
    }
}

fn make_scopes(count: usize) -> Vec<(vello::Scene, EffectRegion)> {
    (0..count)
        .map(|index| {
            let column = index % 10;
            let row = index / 10;
            let origin = [32.0 + (column * 96) as f32, 32.0 + (row * 96) as f32];
            let mut scene = vello::Scene::new();
            let rect = kurbo::Rect::new(
                f64::from(origin[0] + 8.0),
                f64::from(origin[1] + 8.0),
                f64::from(origin[0] + 56.0),
                f64::from(origin[1] + 56.0),
            );
            let path = kurbo::Shape::to_path(&rect, 1e-3);
            scene.fill(
                vello::peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                vello::peniko::Color::from_rgba8(80, 180, 255, 255),
                None,
                &path,
            );
            (
                scene,
                EffectRegion {
                    origin,
                    size: SceneDimensions {
                        width: ROI_SIZE,
                        height: ROI_SIZE,
                    },
                },
            )
        })
        .collect()
}

fn make_main_scene() -> vello::Scene {
    let mut scene = vello::Scene::new();
    let rect = kurbo::Rect::new(0.0, 0.0, f64::from(WIDTH), f64::from(HEIGHT));
    let path = kurbo::Shape::to_path(&rect, 1e-3);
    scene.fill(
        vello::peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        vello::peniko::Color::from_rgba8(14, 18, 28, 255),
        None,
        &path,
    );
    scene
}

#[allow(clippy::too_many_arguments)]
fn encode_frame(
    core: &mut RendererCore,
    backend: &mut GpuFilterBackend,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    dimensions: SceneDimensions,
    main_scene: &vello::Scene,
    scopes: &[(vello::Scene, EffectRegion)],
    chain: &EffectChain,
    output_texture: &wgpu::Texture,
    output_view: &wgpu::TextureView,
) -> Result<(), String> {
    let filter_backend: &mut dyn FilterBackend = backend;
    for (scene, region) in scopes {
        filter_backend.render_scene_to_pending_composite(
            scene,
            dimensions,
            Some(*region),
            chain,
            1.0,
        )?;
    }

    core.render_vello_scene_with_background(
        device,
        queue,
        output_view,
        dimensions.width,
        dimensions.height,
        main_scene,
        vello::peniko::Color::TRANSPARENT,
    )
    .map_err(|error| error.to_string())?;

    let layers = filter_backend.take_pending_composites();
    drain_pending_layers(
        core,
        device,
        queue,
        filter_backend,
        layers,
        Some(output_texture),
        output_view,
        1.0,
    );
    Ok(())
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

fn format_json(info: &wgpu::AdapterInfo, frames: usize, measurements: &[Measurement]) -> String {
    let runs = measurements
        .iter()
        .map(|measurement| {
            let encode = summarize(&measurement.encode_cpu_ms);
            let wait = summarize(&measurement.sync_wait_ms);
            let total = summarize(&measurement.total_ms);
            format!(
                "    {{\"blur_scopes\":{},\"roi_px\":{},\"blur_radius_px\":{},\
                 \"frames\":{},\"warmup_frames\":{},\
                 \"encode_cpu_ms\":{{\"mean\":{:.6},\"median\":{:.6},\"p95\":{:.6}}},\
                 \"sync_wait_ms\":{{\"mean\":{:.6},\"median\":{:.6},\"p95\":{:.6}}},\
                 \"total_ms\":{{\"mean\":{:.6},\"median\":{:.6},\"p95\":{:.6}}}}}",
                measurement.scopes,
                ROI_SIZE,
                BLUR_RADIUS,
                frames,
                WARMUP_FRAMES,
                encode.mean,
                encode.median,
                encode.p95,
                wait.mean,
                wait.median,
                wait.p95,
                total.mean,
                total.median,
                total.p95,
            )
        })
        .collect::<Vec<_>>()
        .join(",\n");

    format!(
        "{{\n  \"benchmark\":\"animatix-gpu-interaction-v1\",\n  \
         \"adapter\":{{\"name\":{},\"backend\":{},\"device_type\":{},\
         \"vendor\":{},\"device\":{},\"driver\":{},\"driver_info\":{}}},\n  \
         \"target_px\":[{},{}],\n  \"warmup_frames\":{},\n  \"runs\":[\n{}\n  ]\n}}\n",
        json_string(&info.name),
        json_string(&format!("{:?}", info.backend)),
        json_string(&format!("{:?}", info.device_type)),
        info.vendor,
        info.device,
        json_string(&info.driver),
        json_string(&info.driver_info),
        WIDTH,
        HEIGHT,
        WARMUP_FRAMES,
        runs,
    )
}

fn summarize(values: &[f64]) -> Summary {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mean = sorted.iter().sum::<f64>() / sorted.len() as f64;
    let middle = sorted.len() / 2;
    let median = if sorted.len().is_multiple_of(2) {
        (sorted[middle - 1] + sorted[middle]) * 0.5
    } else {
        sorted[middle]
    };
    let p95_index = sorted.len().saturating_mul(95).div_ceil(100).saturating_sub(1);
    Summary {
        mean,
        median,
        p95: sorted[p95_index],
    }
}

fn json_string(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    for character in value.chars() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            '\n' => encoded.push_str("\\n"),
            '\r' => encoded.push_str("\\r"),
            '\t' => encoded.push_str("\\t"),
            character if character < ' ' => {
                encoded.push_str(&format!("\\u{:04x}", character as u32));
            },
            character => encoded.push(character),
        }
    }
    encoded.push('"');
    encoded
}
