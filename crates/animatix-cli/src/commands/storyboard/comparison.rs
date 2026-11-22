use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix::timeline::{DebugRenderOptions, SceneDimensions};
use animatix_render::encode::{StoryboardSettings, render_storyboard_comparison};

use super::select_storyboard_times;
use crate::build::{configured_resolution, try_load_and_build};
use crate::extensions::CliExtensions;
use crate::review::samples::target_duration_s;

#[allow(clippy::too_many_arguments)] // Mirrors the storyboard CLI options.
pub(super) fn render(
    before: &Path,
    after: &Path,
    scene: Option<&str>,
    set: &[(String, String)],
    tile_width: Option<u32>,
    tile_height: Option<u32>,
    max_width: u32,
    min_ink: f64,
    fast: bool,
    times: Option<Vec<f64>>,
    max_frames: usize,
    debug_bounds: bool,
    output: Option<PathBuf>,
    extensions: &CliExtensions,
) -> Result<(PathBuf, usize), String> {
    let (before, before_dims) = comparison_target(before, scene, set, extensions)?;
    let (after_target, after_dims) = comparison_target(after, scene, set, extensions)?;
    let common_end = target_duration_s(&before).min(target_duration_s(&after_target));
    let times = if let Some(times) = times {
        if times.iter().any(|time| !time.is_finite() || !(0.0..=common_end).contains(time)) {
            return Err(format!("Comparison times must be in the shared range 0–{common_end:.3}s"));
        }
        times
    } else {
        let mut shared = select_storyboard_times(&after_target, max_frames, min_ink, fast);
        for time in &mut shared {
            *time = time.min(common_end);
        }
        shared.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        if shared.is_empty() {
            shared.push(0.0);
        }
        shared
    };

    let mut settings = StoryboardSettings::default();
    let available_width = max_width.saturating_sub(2 * settings.margin + settings.gutter);
    settings.tile_width = tile_width
        .unwrap_or((after_dims.width / 2).clamp(320, 640))
        .min((available_width / 2).max(1));
    settings.tile_height = tile_height.unwrap_or_else(|| {
        (f64::from(settings.tile_width) * f64::from(after_dims.height)
            / f64::from(after_dims.width))
        .round()
        .max(1.0) as u32
    });
    settings.times = times;
    let output = output.unwrap_or_else(|| {
        let stem = after.file_stem().and_then(|name| name.to_str()).unwrap_or("storyboard");
        PathBuf::from(format!("{stem}_comparison.png"))
    });
    let debug = DebugRenderOptions {
        compute_hit_regions: false,
        draw_bounds: debug_bounds,
        ..Default::default()
    };
    render_storyboard_comparison(
        (&before, before_dims),
        (&after_target, after_dims),
        &settings,
        &output,
        debug,
    )
    .map(|count| (output, count))
    .map_err(|error| error.to_string())
}

fn comparison_target(
    path: &Path,
    scene: Option<&str>,
    set: &[(String, String)],
    extensions: &CliExtensions,
) -> Result<(BuildTarget, SceneDimensions), String> {
    let (target, diagnostics) = try_load_and_build(path, extensions, set)?;
    if let Some(error) = diagnostics.iter().find(|diagnostic| diagnostic.is_error()) {
        return Err(format!("{}: {}", path.display(), error.message));
    }
    let (width, height) = configured_resolution(&target).unwrap_or((1280, 720));
    let selected = if let Some(name) = scene {
        let BuildTarget::MultiScene(mut composition) = target else {
            return Err(format!("{} has no named scene '{name}'", path.display()));
        };
        let scene = composition
            .scenes
            .remove(name)
            .ok_or_else(|| format!("{} has no scene '{name}'", path.display()))?;
        BuildTarget::SingleScene(scene.timeline)
    } else {
        target
    };
    Ok((selected, SceneDimensions { width, height }))
}
