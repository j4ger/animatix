//! Optional render tier for `animatix review`.
//!
//! Rendering is CPU-reviewed first and GPU-checked second: structural facts are
//! always available, while this tier adds whole-frame ink, per-actor ink, and
//! beat-change measurements when an `OffscreenRenderer` adapter can be created.

use std::collections::BTreeMap;
use std::path::Path;

use animatix::composition::BuildTarget;
use animatix::timeline::SceneDimensions;

use crate::frame_source::{
    DEFAULT_MIN_INK_PIXELS, DEFAULT_TOLERANCE, FrameView, actor_visibility, differing_fraction,
    frame_ink_fraction, render_frame_observable, verify_time_key,
};
use crate::review::facts::ReviewFacts;

use super::types::{
    Policy, RenderFacts, RenderMode, RenderSampleFacts, RenderStatus, SiteRestFacts,
};

/// Run the requested render tier. Structural results survive an auto-mode
/// adapter/frame failure; `--render always` turns either into a hard error.
pub(crate) fn run_render_tier(
    target: &BuildTarget,
    dimensions: SceneDimensions,
    sample_times: &[f64],
    policy: Policy,
    mode: RenderMode,
    frames_dir: Option<&Path>,
    facts: &ReviewFacts,
) -> Result<RenderFacts, String> {
    if mode == RenderMode::Never {
        return Ok(RenderFacts {
            status: RenderStatus::Skipped,
            reason: "--render never".to_string(),
            samples: Vec::new(),
            site_rest: None,
        });
    }

    let site_times = if policy == Policy::Site {
        site_rest_probe_times(target)
    } else {
        None
    };
    let mut all_times = sample_times.to_vec();
    if let Some((_, times)) = &site_times {
        all_times.extend(times.iter().copied());
    }
    all_times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    all_times.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    let mut renderer = match animatix_render::offscreen::OffscreenRenderer::new() {
        Ok(renderer) => renderer,
        Err(error) => {
            if mode == RenderMode::Always {
                return Err(format!("cannot initialize offscreen renderer: {error}"));
            }
            return Ok(RenderFacts {
                status: RenderStatus::Skipped,
                reason: format!("no render adapter: {error}"),
                samples: Vec::new(),
                site_rest: None,
            });
        },
    };

    if let Some(dir) = frames_dir {
        std::fs::create_dir_all(dir).map_err(|error| {
            format!("cannot create frames directory {}: {error}", dir.display())
        })?;
    }

    let mut samples = Vec::new();
    let mut previous: Option<(f64, animatix_render::offscreen::RenderedFrame)> = None;
    for (index, &time_s) in all_times.iter().enumerate() {
        let frame = facts
            .frame_at(time_s)
            .ok_or_else(|| format!("missing fact for sample time {}", time_s))?;
        let draw_order: Vec<crate::review::probe::ProbeActor> = frame
            .draw_order
            .iter()
            .filter_map(|(scene, label)| {
                let actor = facts.actor(scene, label)?;
                let frame_actor = facts.actor_frame(frame, scene, label)?;
                let bounds = frame_actor.bounds?;
                Some(crate::review::probe::ProbeActor {
                    scene: scene.clone(),
                    label: label.clone(),
                    bounds,
                    opacity: frame_actor.opacity.unwrap_or(0.0),
                    is_text: actor.is_text,
                })
            })
            .collect();
        let rendered = match render_frame_observable(&mut renderer, target, dimensions, time_s) {
            Ok(rendered) => rendered,
            Err(error) => {
                // ... (rest of the error handling)
                let site_rest = site_times
                    .as_ref()
                    .and_then(|(held, times)| site_rest_from_samples(&samples, *held, times));
                return Ok(RenderFacts {
                    status: RenderStatus::Failed,
                    reason: format!("render failed at {time_s:.3}s: {error}"),
                    samples,
                    site_rest,
                });
            },
        };
        let view =
            FrameView::new(rendered.frame.width, rendered.frame.height, &rendered.frame.rgba)
                .ok_or_else(|| "rendered frame buffer has an unexpected size".to_string())?;
        let reference = view.modal_color();
        let ink_fraction = frame_ink_fraction(&view, DEFAULT_TOLERANCE);
        let occlusions =
            crate::review::probe::probe_occlusion(&view, &draw_order, reference, DEFAULT_TOLERANCE);

        let mut actor_ink = BTreeMap::new();
        let mut changed_from_prev = None;
        if let Some((_, prev_frame)) = &previous {
            let prev_view =
                FrameView::new(prev_frame.width, prev_frame.height, &prev_frame.rgba)
                    .ok_or_else(|| "previous frame buffer has an unexpected size".to_string())?;
            changed_from_prev = Some(differing_fraction(&prev_view, &view, DEFAULT_TOLERANCE));
        }

        for actor in &draw_order {
            let frame_actor =
                facts.actor_frame(frame, &actor.scene, &actor.label).ok_or_else(|| {
                    format!("missing frame actor for {}/{}", actor.scene, actor.label)
                })?;
            if let Some(bounds) = frame_actor.bounds {
                let visibility = actor_visibility(
                    &view,
                    Some(bounds),
                    reference,
                    DEFAULT_TOLERANCE,
                    DEFAULT_MIN_INK_PIXELS,
                );
                actor_ink
                    .insert((actor.scene.clone(), actor.label.clone()), visibility.ink_fraction);
            }
        }

        if let Some(dir) = frames_dir {
            let path = dir.join(format!("review_{index:04}_{time_s:.2}s.png"));
            save_png(&rendered.frame, &path)?;
        }

        samples.push(RenderSampleFacts {
            time_s,
            ink_fraction,
            changed_from_prev,
            actor_ink,
            transition: rendered.transition,
            occlusions, // Add occlusions field here!
        });
        previous = Some((time_s, rendered.frame));
    }

    let site_rest = site_times
        .as_ref()
        .and_then(|(held, times)| site_rest_from_samples(&samples, *held, times));
    Ok(RenderFacts {
        status: RenderStatus::Rendered,
        reason: format!("rendered {} sample(s)", samples.len()),
        samples,
        site_rest,
    })
}

fn save_png(frame: &animatix_render::offscreen::RenderedFrame, path: &Path) -> Result<(), String> {
    let image = image::RgbaImage::from_raw(frame.width, frame.height, frame.rgba.to_vec())
        .ok_or_else(|| "rendered frame buffer has an unexpected size".to_string())?;
    image
        .save(path)
        .map_err(|error| format!("cannot save {}: {error}", path.display()))
}

fn site_rest_from_samples(
    samples: &[RenderSampleFacts],
    held_time_s: f64,
    site_times: &[f64],
) -> Option<SiteRestFacts> {
    let in_site_probe = |time_s: f64| site_times.iter().any(|probe| (probe - time_s).abs() < 0.01);
    let held = samples
        .iter()
        .find(|sample| (sample.time_s - held_time_s).abs() < 0.01)
        .or_else(|| samples.last())?;
    let peak = samples
        .iter()
        .filter(|sample| in_site_probe(sample.time_s))
        .max_by(|a, b| a.ink_fraction.total_cmp(&b.ink_fraction))?;
    Some(SiteRestFacts {
        held_time_s: held.time_s,
        held_ink_fraction: held.ink_fraction,
        peak_ink_fraction: peak.ink_fraction,
        peak_time_s: peak.time_s,
    })
}

/// Probe times used by the site resting-composition policy. Mirrors
/// `scripts/site-rest-check.py`: single scenes sample the whole arc; a
/// multi-scene plate samples only its last scene, after the transition into it.
pub(crate) fn site_rest_probe_times(target: &BuildTarget) -> Option<(f64, Vec<f64>)> {
    let duration_s = super::samples::target_duration_s(target);
    if duration_s <= 0.001 {
        return None;
    }
    let held = (duration_s - 0.05).max(0.0);
    let mut times = Vec::new();
    match target {
        BuildTarget::SingleScene(timeline) => {
            times.push(0.0);
            times.extend(
                timeline.keyframe_times_s().into_iter().filter(|t| *t <= duration_s + 0.01),
            );
            times.push(duration_s / 2.0);
            times.push(held);
        },
        BuildTarget::MultiScene(composition) => {
            let mut by_start: Vec<(&String, f64)> = composition
                .scene_start_times
                .iter()
                .map(|(name, start)| (name, *start))
                .collect();
            by_start.sort_by(|a, b| a.1.total_cmp(&b.1));
            let (last_name, start) = *by_start.last()?;
            let previous_name = by_start.get(by_start.len().checked_sub(2)?).map(|(name, _)| *name);
            let transition_s = previous_name
                .and_then(|name| composition.edges.get(name))
                .map(|edge| edge.transition.duration_ms as f64 / 1000.0)
                .unwrap_or(0.0);
            let scene = composition.scenes.get(last_name)?;
            let content_start = (start + transition_s).min(held);
            let end = (start + scene.duration_s).min(duration_s);
            let steps = ((end - content_start) / 0.5).floor().max(0.0) as usize;
            for i in 0..=steps {
                times.push(round_time(content_start + i as f64 * 0.5));
            }
            times.push(round_time(content_start));
            times.push(held);
        },
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|a, b| verify_time_key(*a) == verify_time_key(*b));
    Some((held, times))
}

fn round_time(time_s: f64) -> f64 {
    (time_s * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_probe_for_single_scene_includes_rest_and_midpoint() {
        let (ast, errors) = animatix_syntax::parser::parse_source(
            "#0s\nbox: Rect, at: (0, 0), size: (10, 10)\n#3s\nfade-out box [500ms]\n",
        );
        assert!(errors.is_empty());
        let report = animatix::timeline::Timeline::build_with_diagnostics(
            &ast.expect("ast"),
            &std::collections::HashMap::new(),
        );
        let target = BuildTarget::SingleScene(report.output);
        let duration = super::super::samples::target_duration_s(&target);
        let (held, times) = site_rest_probe_times(&target).expect("site probes");
        assert!(times.contains(&held));
        assert!(times.contains(&0.0));
        assert!(times.iter().any(|time| (time - duration / 2.0).abs() < 0.01));
    }
}
