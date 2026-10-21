use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix::timeline::DebugRenderOptions;
use animatix_render::encode as export;
use tracing::{error, info};

use crate::build::{configured_resolution, load_and_build};
use crate::extensions::CliExtensions;

pub fn run(
    input: &Path,
    set: &[(String, String)],
    cols: Option<usize>,
    tile_width: Option<u32>,
    tile_height: Option<u32>,
    times: Option<Vec<f64>>,
    max_frames: usize,
    debug_bounds: bool,
    output: Option<PathBuf>,
    extensions: &CliExtensions,
) {
    info!("Generating storyboard for {}", input.display());
    let (target, _) = load_and_build(input, extensions, set);
    let configured_resolution = configured_resolution(&target);
    let (default_tw, default_th) = configured_resolution
        .map(|(w, h)| ((w / 2).clamp(320, 640), (h / 2).clamp(180, 360)))
        .unwrap_or((480, 270));
    let tile_width = tile_width.unwrap_or(default_tw);
    let tile_height = tile_height.unwrap_or(default_th);
    let render_width = configured_resolution.map(|(w, _)| w);
    let render_height = configured_resolution.map(|(_, h)| h);

    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("storyboard");
    let output_file = output.unwrap_or_else(|| PathBuf::from(format!("{}_storyboard.png", stem)));

    let sample_times = match times {
        Some(ts) => ts,
        None => compute_auto_storyboard_times(&target, max_frames),
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

    match result {
        Ok(frame_count) => {
            println!("Storyboard saved to {} ({} frames)", output_file.display(), frame_count);
        },
        Err(e) => {
            error!("Error: {e}");
            std::process::exit(1);
        },
    }
}

/// Automatically select a diverse set of keyframe beat timestamps and the resting frame.
pub fn compute_auto_storyboard_times(target: &BuildTarget, max_frames: usize) -> Vec<f64> {
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
