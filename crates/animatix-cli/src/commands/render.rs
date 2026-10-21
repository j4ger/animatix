use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix::timeline::DebugRenderOptions;
use animatix_render::encode as export;
use tracing::{error, info};

use crate::build::{configured_resolution, load_and_build};
use crate::extensions::CliExtensions;

pub fn run_image(
    input: &Path,
    width: Option<u32>,
    height: Option<u32>,
    time: f32,
    output: Option<PathBuf>,
    debug_bounds: bool,
    set: &[(String, String)],
    extensions: &CliExtensions,
) {
    info!("Rendering Animatix image: {}", input.display());
    let (target, _) = load_and_build(input, extensions, set);
    let configured_resolution = configured_resolution(&target);
    let width = width.or(configured_resolution.map(|(w, _)| w)).unwrap_or(1280);
    let height = height.or(configured_resolution.map(|(_, h)| h)).unwrap_or(720);
    let output_file = output.unwrap_or_else(|| PathBuf::from(format!("animatix_{}s.png", time)));
    info!("Output image: {}x{} at {}s -> {}", width, height, time, output_file.display());
    let result = match &target {
        BuildTarget::MultiScene(comp) => {
            export::render_image_composition(comp, width, height, time, &output_file)
        },
        BuildTarget::SingleScene(timeline) => export::render_image_timeline_with_debug(
            timeline.clone(),
            width,
            height,
            time,
            &output_file,
            DebugRenderOptions {
                compute_hit_regions: false,
                draw_bounds: debug_bounds,
                ..Default::default()
            },
        ),
    };
    if let Err(e) = result {
        error!("Error: {e}");
        std::process::exit(1);
    }
}

#[cfg(feature = "video")]
pub fn run_video(
    input: &Path,
    width: Option<u32>,
    height: Option<u32>,
    fps: u32,
    duration: Option<f32>,
    hold: f32,
    output: Option<PathBuf>,
    debug_bounds: bool,
    threads: export::MaxRenderThreads,
    codec: export::VideoCodec,
    preset: export::H264Preset,
    export_preset: Option<&str>,
    set: &[(String, String)],
    extensions: &CliExtensions,
) {
    info!("Rendering Animatix video: {}", input.display());
    let (mut width, mut height, mut fps, mut codec, mut preset) =
        (width, height, fps, codec, preset);
    if let Some(name) = export_preset {
        let Some(preset_values) = export::ExportPreset::by_name(name) else {
            error!("Unknown export preset '{name}'");
            std::process::exit(2);
        };
        width = Some(preset_values.width);
        height = Some(preset_values.height);
        fps = preset_values.fps;
        codec = preset_values.video_codec;
        preset = preset_values.h264_preset;
    }
    let (target, _) = load_and_build(input, extensions, set);
    if export_preset.is_none() {
        let configured = match &target {
            BuildTarget::SingleScene(timeline) => timeline.export_preset(),
            BuildTarget::MultiScene(composition) => composition
                .scenes
                .values()
                .next()
                .and_then(|scene| scene.timeline.export_preset()),
        };
        if let Some(name) = configured {
            let Some(preset_values) = export::ExportPreset::by_name(name) else {
                error!("Unknown export preset '{name}' from config");
                std::process::exit(2);
            };
            width = Some(preset_values.width);
            height = Some(preset_values.height);
            fps = preset_values.fps;
            codec = preset_values.video_codec;
            preset = preset_values.h264_preset;
        }
    }
    let configured_resolution = configured_resolution(&target);
    let width = width.or(configured_resolution.map(|(w, _)| w)).unwrap_or(1280);
    let height = height.or(configured_resolution.map(|(_, h)| h)).unwrap_or(720);
    let effective_duration = resolve_duration(duration, &target, hold, 0.5);
    let output_file = output.unwrap_or_else(|| default_output_file("mp4"));
    info!(
        "Output configuration: {}x{} at {} FPS for {:.2}s -> {}",
        width,
        height,
        fps,
        effective_duration,
        output_file.display()
    );
    let result = match &target {
        BuildTarget::MultiScene(comp) => export::render_video_composition_with_settings(
            comp,
            width,
            height,
            fps,
            effective_duration,
            &output_file,
            DebugRenderOptions {
                compute_hit_regions: false,
                draw_bounds: debug_bounds,
                ..Default::default()
            },
            export::ExportSettings {
                max_render_threads: threads,
                video_codec: codec,
                h264_preset: preset,
            },
        ),
        BuildTarget::SingleScene(timeline) => export::render_video_timeline_with_settings(
            timeline.clone(),
            width,
            height,
            fps,
            effective_duration,
            &output_file,
            DebugRenderOptions {
                compute_hit_regions: false,
                draw_bounds: debug_bounds,
                ..Default::default()
            },
            export::ExportSettings {
                max_render_threads: threads,
                video_codec: codec,
                h264_preset: preset,
            },
        ),
    };
    if let Err(e) = result {
        error!("Error: {e}");
        std::process::exit(1);
    }
}

#[cfg(feature = "video")]
pub fn run_gif(
    input: &Path,
    width: Option<u32>,
    height: Option<u32>,
    fps: u32,
    duration: Option<f32>,
    hold: f32,
    output: Option<PathBuf>,
    debug_bounds: bool,
    threads: export::MaxRenderThreads,
    export_preset: Option<&str>,
    set: &[(String, String)],
    extensions: &CliExtensions,
) {
    info!("Rendering Animatix GIF: {}", input.display());
    let (mut width, mut height, mut fps) = (width, height, fps);
    if let Some(name) = export_preset {
        let Some(preset_values) = export::ExportPreset::by_name(name) else {
            error!("Unknown export preset '{name}'");
            std::process::exit(2);
        };
        width = Some(preset_values.width);
        height = Some(preset_values.height);
        fps = preset_values.fps;
    }
    let (target, _) = load_and_build(input, extensions, set);
    if export_preset.is_none() {
        let configured = match &target {
            BuildTarget::SingleScene(timeline) => timeline.export_preset(),
            BuildTarget::MultiScene(composition) => composition
                .scenes
                .values()
                .next()
                .and_then(|scene| scene.timeline.export_preset()),
        };
        if let Some(name) = configured {
            let Some(preset_values) = export::ExportPreset::by_name(name) else {
                error!("Unknown export preset '{name}' from config");
                std::process::exit(2);
            };
            width = Some(preset_values.width);
            height = Some(preset_values.height);
            fps = preset_values.fps;
        }
    }
    let configured_resolution = configured_resolution(&target);
    let width = width.or(configured_resolution.map(|(w, _)| w)).unwrap_or(640);
    let height = height.or(configured_resolution.map(|(_, h)| h)).unwrap_or(360);
    let effective_duration = resolve_duration(duration, &target, hold, 0.5);
    let output_file = output.unwrap_or_else(|| default_output_file("gif"));
    info!(
        "Output configuration: {}x{} at {} FPS for {:.2}s -> {}",
        width,
        height,
        fps,
        effective_duration,
        output_file.display()
    );
    let result = match &target {
        BuildTarget::MultiScene(comp) => export::render_gif_composition_with_settings(
            comp,
            width,
            height,
            fps,
            effective_duration,
            &output_file,
            DebugRenderOptions {
                compute_hit_regions: false,
                draw_bounds: debug_bounds,
                ..Default::default()
            },
            export::ExportSettings {
                max_render_threads: threads,
                ..Default::default()
            },
        ),
        BuildTarget::SingleScene(timeline) => export::render_gif_timeline_with_settings(
            timeline.clone(),
            width,
            height,
            fps,
            effective_duration,
            &output_file,
            DebugRenderOptions {
                compute_hit_regions: false,
                draw_bounds: debug_bounds,
                ..Default::default()
            },
            export::ExportSettings {
                max_render_threads: threads,
                ..Default::default()
            },
        ),
    };
    if let Err(e) = result {
        error!("Error: {e}");
        std::process::exit(1);
    }
}

/// Resolves export duration for a `BuildTarget` (single or multi-scene).
#[cfg(feature = "video")]
fn resolve_duration(
    duration: Option<f32>,
    target: &BuildTarget,
    hold: f32,
    min_duration: f32,
) -> f32 {
    duration.unwrap_or_else(|| {
        let d = target.duration_s() as f32 + hold.max(0.0);
        d.max(min_duration)
    })
}

/// Generates a timestamped default filename when `--output` is omitted.
#[cfg(feature = "video")]
fn default_output_file(ext: &str) -> PathBuf {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Simple unix timestamp: animatix_1234567890.png
    PathBuf::from(format!("animatix_{}.{}", secs, ext))
}
