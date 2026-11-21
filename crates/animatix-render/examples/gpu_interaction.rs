//! Native GPU interaction benchmark for the steady-state filter path.
//!
//! Runs one and eight independent Blur, MotionBlur, or Bloom scopes over 64 px regions on a 720p
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
};
use animatix_render::frame_gpu::FrameGpu;

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;
const DEFAULT_ROI_SIZE: u32 = 64;
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
    let effect = args.next().unwrap_or_else(|| "blur".to_string());
    let roi_size = args
        .next()
        .map(|value| value.parse::<u32>())
        .transpose()?
        .unwrap_or(DEFAULT_ROI_SIZE);
    if !(1..=120).contains(&frames)
        || !matches!(effect.as_str(), "blur" | "blur-alpha" | "blur-wide" | "motion" | "bloom")
        || !matches!(roi_size, 64 | 512)
        || args.next().is_some()
    {
        return Err(boxed_message(
            "usage: gpu_interaction [output.json] [frames: 1..=120] [blur|blur-alpha|blur-wide|motion|bloom] [roi: 64|512]",
        ));
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
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut renderer = FrameGpu::new(&device, &queue);

    let main_scene = make_main_scene();
    let chain = make_effect_chain(&effect);
    let mut measurements = Vec::with_capacity(2);
    let scope_counts: &[usize] = if roi_size == 512 { &[1] } else { &[1, 8] };
    for &scopes in scope_counts {
        let mut scene = main_scene.clone();
        for (content, region) in make_scopes(scopes, &effect, roi_size) {
            scene.push_filter(
                content,
                Some(region),
                chain.clone(),
                1.0,
                animatix::scene::CompositePlacement::AfterScene,
            );
        }

        for _ in 0..WARMUP_FRAMES {
            renderer
                .render_frame_into(&scene, &output_view, kurbo::Affine::IDENTITY)
                .map_err(boxed_message)?;
            wait_for_submitted_work(&device, &queue)?;
        }

        let mut measurement = Measurement {
            scopes,
            encode_cpu_ms: Vec::with_capacity(frames),
            sync_wait_ms: Vec::with_capacity(frames),
            total_ms: Vec::with_capacity(frames),
        };
        for _ in 0..frames {
            let frame_start = Instant::now();
            renderer
                .render_frame_into(&scene, &output_view, kurbo::Affine::IDENTITY)
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

    let json = format_json(&adapter_info, frames, &measurements, &effect, roi_size);
    fs::write(output_path, json)?;
    Ok(())
}

fn make_effect_chain(effect: &str) -> EffectChain {
    let (name, values) = match effect {
        "bloom" => (
            "Bloom",
            vec![
                EffectParamValue::F32(0.75),
                EffectParamValue::F32(1.0),
                EffectParamValue::F32(0.0),
            ],
        ),
        "motion" => ("MotionBlur", vec![EffectParamValue::F32(24.0), EffectParamValue::F32(27.0)]),
        "blur-wide" => ("Blur", vec![EffectParamValue::F32(44.0)]),
        _ => ("Blur", vec![EffectParamValue::F32(BLUR_RADIUS)]), // The validated blur presets.
    };
    EffectChain {
        instances: vec![EffectInstance {
            id: EffectId::new(name),
            enabled: true,
            params: EffectParams { values },
        }],
        time_ms: 0.0,
        ..Default::default()
    }
}

fn make_scopes(
    count: usize,
    effect: &str,
    roi_size: u32,
) -> Vec<(animatix::scene::Scene, EffectRegion)> {
    (0..count)
        .map(|index| {
            let column = index % 10;
            let row = index / 10;
            let origin = [32.0 + (column * 96) as f32, 32.0 + (row * 96) as f32];
            let mut scene = animatix::scene::Scene::new();
            let rect = kurbo::Rect::new(
                f64::from(origin[0] + roi_size as f32 * 0.125),
                f64::from(origin[1] + roi_size as f32 * 0.125),
                f64::from(origin[0] + roi_size as f32 * 0.875),
                f64::from(origin[1] + roi_size as f32 * 0.875),
            );
            let path = kurbo::Shape::to_path(&rect, 1e-3);
            scene.fill(
                peniko::Fill::NonZero,
                kurbo::Affine::IDENTITY,
                peniko::Color::from_rgba8(80, 180, 255, source_alpha(effect)),
                None,
                &path,
            );
            (
                scene,
                EffectRegion {
                    origin,
                    size: SceneDimensions {
                        width: roi_size,
                        height: roi_size,
                    },
                },
            )
        })
        .collect()
}

fn make_main_scene() -> animatix::scene::Scene {
    let mut scene = animatix::scene::Scene::new();
    let rect = kurbo::Rect::new(0.0, 0.0, f64::from(WIDTH), f64::from(HEIGHT));
    let path = kurbo::Shape::to_path(&rect, 1e-3);
    scene.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::from_rgba8(14, 18, 28, 255),
        None,
        &path,
    );
    scene
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

fn format_json(
    info: &wgpu::AdapterInfo,
    frames: usize,
    measurements: &[Measurement],
    effect: &str,
    roi_size: u32,
) -> String {
    let runs = measurements
        .iter()
        .map(|measurement| {
            let encode = summarize(&measurement.encode_cpu_ms);
            let wait = summarize(&measurement.sync_wait_ms);
            let total = summarize(&measurement.total_ms);
            let legacy_blur_fields = if effect == "blur" {
                format!(
                    "\"blur_scopes\":{},\"blur_radius_px\":{},",
                    measurement.scopes, BLUR_RADIUS
                )
            } else {
                String::new()
            };
            format!(
                "    {{{}\"effect_scopes\":{},\"roi_px\":{},\
                 \"frames\":{},\"warmup_frames\":{},\
                 \"encode_cpu_ms\":{{\"mean\":{:.6},\"median\":{:.6},\"p95\":{:.6}}},\
                 \"sync_wait_ms\":{{\"mean\":{:.6},\"median\":{:.6},\"p95\":{:.6}}},\
                 \"total_ms\":{{\"mean\":{:.6},\"median\":{:.6},\"p95\":{:.6}}}}}",
                legacy_blur_fields,
                measurement.scopes,
                roi_size,
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
        "{{\n  \"benchmark\":\"animatix-gpu-interaction-v2\",\n  \
         \"effect\":{},\n  \"effect_parameters\":{},\n  \"source_alpha_u8\":{},\n  \
         \"adapter\":{{\"name\":{},\"backend\":{},\"device_type\":{},\
         \"vendor\":{},\"device\":{},\"driver\":{},\"driver_info\":{}}},\n  \
         \"target_px\":[{},{}],\n  \"warmup_frames\":{},\n  \"runs\":[\n{}\n  ]\n}}\n",
        json_string(effect),
        match effect {
            "bloom" => r#"{"intensity":0.75,"keep":1,"threshold":0}"#,
            "motion" => r#"{"length":24,"angle":27}"#,
            "blur-wide" => r#"{"radius":44}"#,
            _ => r#"{"radius":8}"#, // The validated dense blur presets.
        },
        source_alpha(effect),
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

fn source_alpha(effect: &str) -> u8 {
    if effect == "blur" { 255 } else { 128 }
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
