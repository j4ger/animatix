use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix::timeline::DebugRenderOptions;
use animatix_render::encode as export;
use tracing::{error, info};

use crate::build::load_and_build;
use crate::extensions::CliExtensions;

pub fn run_image(
    input: &Path,
    width: Option<u32>,
    height: Option<u32>,
    time: f64,
    output: Option<PathBuf>,
    debug_bounds: bool,
    set: &[(String, String)],
    extensions: &CliExtensions,
) {
    info!("Rendering Animatix image: {}", input.display());
    let (target, _) = load_and_build(input, extensions, set);
    let (width, height) = output_dimensions(&target, width, height, (1280, 720));
    let output_file = output.unwrap_or_else(|| PathBuf::from(format!("animatix_{}s.png", time)));
    info!("Output image: {}x{} at {}s -> {}", width, height, time, output_file.display());
    let result = match &target {
        BuildTarget::MultiScene(comp) => {
            export::render_image_composition(comp, width, height, time, &output_file)
        },
        BuildTarget::SingleScene(timeline) => export::render_image_timeline_with_options(
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
            export::ExportOptions::default(),
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
    scene: Option<&str>,
    no_audio: bool,
    width: Option<u32>,
    height: Option<u32>,
    fps: u32,
    duration: Option<f64>,
    hold: f64,
    output: Option<PathBuf>,
    debug_bounds: bool,
    settings: export::ExportSettings,
    export_preset: Option<&str>,
    set: &[(String, String)],
    extensions: &CliExtensions,
) {
    info!("Rendering Animatix video: {}", input.display());
    let (target, diagnostics) = load_and_build(input, extensions, set);
    if scene.is_some() && diagnostics.iter().any(|diagnostic| diagnostic.is_error()) {
        std::process::exit(1);
    }
    let (width, height, fps, settings) =
        match effective_video_settings(&target, width, height, fps, settings, export_preset) {
            Ok(settings) => settings,
            Err(message) => {
                error!("{message}");
                std::process::exit(2);
            },
        };
    let effective_duration = resolve_duration(duration, &target, hold, 0.5);
    let output_file = output.unwrap_or_else(|| default_output_file("mp4"));
    info!(
        "Exporting {}x{} at {} FPS with {:.2}s full duration to {}",
        width,
        height,
        fps,
        effective_duration,
        output_file.display()
    );
    let debug_options = DebugRenderOptions {
        compute_hit_regions: false,
        draw_bounds: debug_bounds,
        ..Default::default()
    };
    let options = export::ExportOptions {
        settings,
        skip_audio: no_audio,
        ..Default::default()
    };
    let result = match (&target, scene) {
        (BuildTarget::MultiScene(comp), Some(scene)) => export::render_video_chapter_with_options(
            comp,
            scene,
            width,
            height,
            fps,
            effective_duration,
            &output_file,
            debug_options,
            options,
        ),
        (BuildTarget::MultiScene(comp), None) => export::render_video_composition_with_options(
            comp,
            width,
            height,
            fps,
            effective_duration,
            &output_file,
            debug_options,
            options,
        ),
        (BuildTarget::SingleScene(timeline), None) => export::render_video_timeline_with_options(
            timeline.clone(),
            width,
            height,
            fps,
            effective_duration,
            &output_file,
            debug_options,
            options,
        ),
        (BuildTarget::SingleScene(_), Some(_)) => Err(export::ExportError::VideoEncode(
            "--scene requires a named scene in the composition".into(),
        )),
    };
    if let Err(e) = result {
        error!("Error: {e}");
        std::process::exit(1);
    }
}

#[cfg(feature = "video")]
pub(super) fn effective_video_settings(
    target: &BuildTarget,
    width: Option<u32>,
    height: Option<u32>,
    fps: u32,
    settings: export::ExportSettings,
    export_preset: Option<&str>,
) -> Result<(u32, u32, u32, export::ExportSettings), String> {
    if let Some(preset) = resolve_preset(target, export_preset)? {
        let (width, height) =
            video_output_dimensions(target, width, height, (preset.width, preset.height));
        return Ok((
            width,
            height,
            preset.fps,
            export::ExportSettings {
                video_codec: preset.video_codec,
                h264_preset: preset.h264_preset,
                ..settings
            },
        ));
    }
    let fallback = target.configured_canvas_size().unwrap_or((1280, 720));
    let (width, height) = video_output_dimensions(target, width, height, fallback);
    Ok((width, height, fps, settings))
}

#[cfg(feature = "video")]
pub fn run_gif(
    input: &Path,
    width: Option<u32>,
    height: Option<u32>,
    fps: u32,
    duration: Option<f64>,
    hold: f64,
    output: Option<PathBuf>,
    debug_bounds: bool,
    threads: export::MaxRenderThreads,
    export_preset: Option<&str>,
    set: &[(String, String)],
    extensions: &CliExtensions,
) {
    info!("Rendering Animatix GIF: {}", input.display());
    let (target, _) = load_and_build(input, extensions, set);
    let (width, height, fps) = match resolve_preset(&target, export_preset) {
        Ok(Some(preset)) => {
            let (w, h) = export::output_dimensions(
                target.canvas_dimensions(),
                width,
                height,
                (preset.width, preset.height),
            );
            (w, h, preset.fps)
        },
        Ok(None) => {
            let (w, h) = output_dimensions(&target, width, height, (640, 360));
            (w, h, fps)
        },
        Err(message) => {
            error!("{message}");
            std::process::exit(2);
        },
    };
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
    let debug_options = DebugRenderOptions {
        compute_hit_regions: false,
        draw_bounds: debug_bounds,
        ..Default::default()
    };
    let options = export::ExportOptions::with_settings(export::ExportSettings {
        max_render_threads: threads,
        ..Default::default()
    });
    let result = match &target {
        BuildTarget::MultiScene(comp) => export::render_gif_composition_with_options(
            comp,
            width,
            height,
            fps,
            effective_duration,
            &output_file,
            debug_options,
            options,
        ),
        BuildTarget::SingleScene(timeline) => export::render_gif_timeline_with_options(
            timeline.clone(),
            width,
            height,
            fps,
            effective_duration,
            &output_file,
            debug_options,
            options,
        ),
    };
    if let Err(e) = result {
        error!("Error: {e}");
        std::process::exit(1);
    }
}

fn output_dimensions(
    target: &BuildTarget,
    width: Option<u32>,
    height: Option<u32>,
    fallback: (u32, u32),
) -> (u32, u32) {
    export::output_dimensions(
        target.canvas_dimensions(),
        width,
        height,
        target.configured_canvas_size().unwrap_or(fallback),
    )
}

#[cfg(feature = "video")]
fn video_output_dimensions(
    target: &BuildTarget,
    width: Option<u32>,
    height: Option<u32>,
    fallback: (u32, u32),
) -> (u32, u32) {
    let (w, h) = export::output_dimensions(target.canvas_dimensions(), width, height, fallback);
    // Explicit and preset sizes are validated by the encoder. An inferred
    // dimension can be rounded to an even pixel count for YUV 4:2:0.
    match (width, height) {
        (Some(_), None) => (w, h.saturating_add(h % 2)),
        (None, Some(_)) => (w.saturating_add(w % 2), h),
        _ => (w, h),
    }
}

#[cfg(feature = "video")]
fn resolve_preset(
    target: &BuildTarget,
    explicit: Option<&str>,
) -> Result<Option<export::ExportPreset>, String> {
    let configured = match target {
        BuildTarget::SingleScene(timeline) => timeline.export_preset(),
        BuildTarget::MultiScene(composition) => composition
            .scenes
            .values()
            .next()
            .and_then(|scene| scene.timeline.export_preset()),
    };
    explicit
        .or(configured)
        .map(|name| {
            export::ExportPreset::by_name(name).copied().ok_or_else(|| {
                let source = if explicit.is_some() {
                    ""
                } else {
                    " from config"
                };
                format!("Unknown export preset '{name}'{source}")
            })
        })
        .transpose()
}

/// Resolves export duration for a `BuildTarget` (single or multi-scene).
#[cfg(feature = "video")]
pub(super) fn resolve_duration(
    duration: Option<f64>,
    target: &BuildTarget,
    hold: f64,
    min_duration: f64,
) -> f64 {
    duration.unwrap_or_else(|| {
        let d = animatix::time::add_seconds(target.duration_s(), hold.max(0.0));
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
