use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix::timeline::DebugRenderOptions;
use animatix_render::encode as export;
use animatix_render::offscreen::OffscreenRenderer;
use tracing::{error, info};

use crate::build::{configured_resolution, load_and_build};
use crate::extensions::CliExtensions;
use crate::frame_source::{
    DEFAULT_TOLERANCE, FrameView, frame_ink_fraction, render_frame_observable,
};

pub fn run(
    input: &Path,
    set: &[(String, String)],
    cols: Option<usize>,
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
) {
    match render_storyboard(
        input,
        set,
        cols,
        tile_width,
        tile_height,
        max_width,
        min_ink,
        fast,
        times,
        max_frames,
        debug_bounds,
        output,
        extensions,
    ) {
        Ok((output_file, frame_count)) => {
            println!("Storyboard saved to {} ({} frames)", output_file.display(), frame_count);
        },
        Err(e) => {
            error!("Error: {e}");
            std::process::exit(1);
        },
    }
}

/// Render a storyboard without printing or exiting, so `review --storyboard`
/// can compose it into its own report flow. Output bytes and semantics are
/// exactly those of [`run`].
#[allow(clippy::too_many_arguments)] // mirrors the CLI surface above
pub(crate) fn render_storyboard(
    input: &Path,
    set: &[(String, String)],
    cols: Option<usize>,
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
    info!("Generating storyboard for {}", input.display());
    let (target, _) = load_and_build(input, extensions, set);
    let configured_resolution = configured_resolution(&target);
    let (default_tw, default_th) = configured_resolution
        .map(|(w, h)| ((w / 2).clamp(320, 640), (h / 2).clamp(180, 360)))
        .unwrap_or((480, 270));
    let mut tile_width = tile_width.unwrap_or(default_tw);
    let height_was_explicit = tile_height.is_some();
    let mut tile_height = tile_height.unwrap_or(default_th);
    let render_width = configured_resolution.map(|(w, _)| w);
    let render_height = configured_resolution.map(|(_, h)| h);

    // Cap the contact sheet to a vision-model-friendly long edge. `cols` is
    // auto-detected by the exporter (up to 4), so assume the maximum when it
    // was not given explicitly.
    let effective_cols = cols.unwrap_or(4).max(1) as u32;
    let margin = 12_u32;
    let gutter = 8_u32;
    let available_width = max_width
        .saturating_sub(2 * margin)
        .saturating_sub((effective_cols - 1) * gutter);
    let max_tile_width = (available_width / effective_cols).max(1);
    if tile_width > max_tile_width {
        tile_width = max_tile_width;
    }
    if !height_was_explicit {
        let aspect = default_th as f64 / default_tw.max(1) as f64;
        tile_height = ((tile_width as f64 * aspect).round() as u32).max(1);
    }

    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("storyboard");
    let output_file = output.unwrap_or_else(|| PathBuf::from(format!("{}_storyboard.png", stem)));

    let sample_times = match times {
        Some(ts) => ts,
        None => select_storyboard_times(&target, max_frames, min_ink, fast),
    };

    let settings = export::StoryboardSettings {
        cols,
        tile_width,
        tile_height,
        render_width,
        render_height,
        times: sample_times,
        gutter: 8,
        margin: 12,
        header_height: 24,
    };

    let debug_options = DebugRenderOptions {
        compute_hit_regions: false,
        draw_bounds: debug_bounds,
        ..Default::default()
    };

    let result = match &target {
        BuildTarget::SingleScene(timeline) => export::render_storyboard_timeline(
            timeline.clone(),
            &settings,
            &output_file,
            debug_options,
        ),
        BuildTarget::MultiScene(comp) => {
            export::render_storyboard_composition(comp, &settings, &output_file, debug_options)
        },
    };

    result.map(|frame_count| (output_file, frame_count)).map_err(|e| e.to_string())
}

/// Automatically select a diverse set of keyframe beat timestamps and the resting frame.
pub(crate) fn compute_auto_storyboard_times(target: &BuildTarget, max_frames: usize) -> Vec<f64> {
    let max_frames = max_frames.max(2);
    let (mut key_times, duration_s) = match target {
        BuildTarget::SingleScene(timeline) => {
            let kt: Vec<f64> = timeline.keyframe_times_s().into_iter().collect();
            (kt, timeline.playback_duration_seconds())
        },
        BuildTarget::MultiScene(comp) => {
            let mut kt = Vec::new();
            for (name, scene) in &comp.scenes {
                let start_s = comp.scene_start_times.get(name).copied().unwrap_or(0.0);
                kt.push(start_s);
                for t in scene.timeline.keyframe_times_s() {
                    kt.push(start_s + t);
                }
            }
            (kt, comp.global_duration_s)
        },
    };

    if duration_s <= 0.001 {
        return vec![0.0];
    }

    key_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    key_times.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    if key_times.is_empty() {
        let step = duration_s / (max_frames - 1) as f64;
        return (0..max_frames).map(|i| i as f64 * step).collect();
    }

    let step = duration_s / (max_frames - 1) as f64;
    let mut selected = Vec::with_capacity(max_frames);
    selected.push(0.0);

    let mut used_keys = std::collections::HashSet::new();

    for i in 1..(max_frames - 1) {
        let ideal_t = i as f64 * step;
        let mut best_k_idx = None;
        let mut best_key = None;
        let mut best_dist = step * 0.40;

        for (k_idx, &kt) in key_times.iter().enumerate() {
            if kt <= 0.05 || kt >= duration_s - 0.05 || used_keys.contains(&k_idx) {
                continue;
            }
            let dist = (kt - ideal_t).abs();
            if dist < best_dist {
                best_dist = dist;
                best_key = Some(kt);
                best_k_idx = Some(k_idx);
            }
        }

        if let Some(idx) = best_k_idx {
            used_keys.insert(idx);
        }

        selected.push(best_key.unwrap_or(ideal_t));
    }

    selected.push(duration_s);
    selected.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    selected.dedup_by(|a, b| (*a - *b).abs() < 0.02);

    while selected.len() < max_frames {
        let mut max_gap = 0.0;
        let mut insert_idx = 0;
        let mut midpoint = 0.0;
        for i in 0..(selected.len() - 1) {
            let gap = selected[i + 1] - selected[i];
            if gap > max_gap {
                max_gap = gap;
                insert_idx = i + 1;
                midpoint = selected[i] + gap / 2.0;
            }
        }
        if max_gap <= 0.05 {
            break;
        }
        selected.insert(insert_idx, midpoint);
    }

    selected
}

/// Select storyboard times from keyframe intervals.
///
/// Keyframe boundaries are usually empty frames (an actor has just entered or
/// is about to leave), so candidates come from inside each interval. In `fast`
/// mode the midpoint is used directly; otherwise each candidate is rendered and
/// the frame with the most ink wins (Manim MCP's "richest frame"), and intervals
/// below `min_ink` are dropped. The result is capped at `max_frames`.
pub(crate) fn select_storyboard_times(
    target: &BuildTarget,
    max_frames: usize,
    min_ink: f64,
    fast: bool,
) -> Vec<f64> {
    let max_frames = max_frames.max(2);
    let (mut key_times, duration_s) = match target {
        BuildTarget::SingleScene(timeline) => {
            let kt: Vec<f64> = timeline.keyframe_times_s().into_iter().collect();
            (kt, timeline.playback_duration_seconds())
        },
        BuildTarget::MultiScene(comp) => {
            let mut kt = Vec::new();
            for (name, scene) in &comp.scenes {
                let start_s = comp.scene_start_times.get(name).copied().unwrap_or(0.0);
                kt.push(start_s);
                for t in scene.timeline.keyframe_times_s() {
                    kt.push(start_s + t);
                }
            }
            (kt, comp.global_duration_s)
        },
    };

    if duration_s <= 0.001 {
        return vec![0.0];
    }

    key_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    key_times.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    let mut boundaries = vec![0.0];
    boundaries.extend(key_times);
    boundaries.push(duration_s);
    boundaries.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    let mut intervals: Vec<(f64, f64)> = boundaries
        .windows(2)
        .filter_map(|pair| {
            let gap = pair[1] - pair[0];
            (gap >= 0.01).then_some((pair[0], pair[1]))
        })
        .collect();
    if intervals.is_empty() {
        return vec![duration_s * 0.5];
    }

    // At most one tile per interval, evenly spread when there are too many.
    if intervals.len() > max_frames {
        let total = intervals.len();
        intervals =
            (0..max_frames).map(|i| intervals[i * (total - 1) / (max_frames - 1)]).collect();
    }

    let mut selected = Vec::new();
    if fast {
        for (start, end) in &intervals {
            selected.push(start + (end - start) * 0.5);
        }
    } else {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(renderer) => renderer,
            // No GPU: fall back to midpoints rather than failing the command.
            Err(_) => return select_storyboard_times(target, max_frames, min_ink, true),
        };
        let dimensions = configured_resolution(target)
            .map(|(width, height)| animatix::timeline::SceneDimensions { width, height })
            .unwrap_or(animatix::timeline::SceneDimensions {
                width: 1280,
                height: 720,
            });
        for (start, end) in &intervals {
            let gap = end - start;
            let mut best_t = start + gap * 0.5;
            let mut best_ink = 0.0_f32;
            for fraction in [0.25_f64, 0.5, 0.75] {
                let t = start + gap * fraction;
                if let Ok(observable) =
                    render_frame_observable(&mut renderer, target, dimensions, t)
                    && let Some(view) = FrameView::new(
                        observable.frame.width,
                        observable.frame.height,
                        &observable.frame.rgba,
                    )
                {
                    let ink = frame_ink_fraction(&view, DEFAULT_TOLERANCE);
                    if ink > best_ink {
                        best_ink = ink;
                        best_t = t;
                    }
                }
            }
            if best_ink >= min_ink as f32 {
                selected.push(best_t);
            }
        }
        if selected.is_empty() {
            selected.push(
                intervals
                    .iter()
                    .map(|(start, end)| start + (end - start) * 0.5)
                    .fold(0.0_f64, f64::max),
            );
        }
    }

    // Backfill to the requested frame count by splitting the largest gaps.
    while selected.len() < max_frames {
        selected.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut split_at = 0;
        let mut max_gap = 0.0;
        for i in 0..selected.len().saturating_sub(1) {
            let gap = selected[i + 1] - selected[i];
            if gap > max_gap {
                max_gap = gap;
                split_at = i + 1;
            }
        }
        if max_gap <= 0.05 {
            break;
        }
        let midpoint = selected[split_at - 1] + max_gap * 0.5;
        selected.insert(split_at, midpoint);
    }

    selected.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    selected.dedup_by(|a, b| (*a - *b).abs() < 0.02);
    selected
}
