//! Sample-time selection for `animatix review`.
//!
//! `auto` reuses the storyboard's beat picker (start, keyframe beats near an
//! even grid, rest frame); `keyframes` takes the complete keyframe set (thinned
//! to `--max-frames` when necessary); a numeric value is an even grid.

use animatix::composition::BuildTarget;

use super::types::SampleSpec;

/// Duration used by review: declared duration when set, otherwise the
/// keyframe-inferred length. Multi-scene files use the composition total.
pub(crate) fn target_duration_s(target: &BuildTarget) -> f64 {
    match target {
        BuildTarget::SingleScene(timeline) => timeline.playback_duration_seconds(),
        BuildTarget::MultiScene(composition) => composition.global_duration_s,
    }
}

/// All keyframe beat times in global seconds.
pub(crate) fn all_keyframe_times(target: &BuildTarget) -> Vec<f64> {
    let mut times = match target {
        BuildTarget::SingleScene(timeline) => timeline.keyframe_times_s(),
        BuildTarget::MultiScene(composition) => {
            let mut times = Vec::new();
            for (name, scene) in &composition.scenes {
                let start_s = composition.scene_start_times.get(name).copied().unwrap_or(0.0);
                times.push(start_s);
                for t in scene.timeline.keyframe_times_s() {
                    times.push(start_s + t);
                }
            }
            times
        },
    };
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    times
}

/// Compute the base sample grid for `--samples`.
pub(crate) fn compute_sample_times(
    target: &BuildTarget,
    spec: &SampleSpec,
    max_frames: usize,
) -> Vec<f64> {
    let duration_s = target_duration_s(target);
    let cap = max_frames.max(2);
    if duration_s <= 0.001 {
        return vec![0.0];
    }

    let mut times = match spec {
        SampleSpec::Auto => crate::commands::storyboard::compute_auto_storyboard_times(target, cap),
        SampleSpec::Keyframes => {
            let mut keyframes = all_keyframe_times(target);
            keyframes.insert(0, 0.0);
            keyframes.push(duration_s);
            keyframes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            keyframes.dedup_by(|a, b| (*a - *b).abs() < 0.001);
            thin_to_cap(keyframes, cap)
        },
        SampleSpec::Count(count) => {
            let n = (*count).clamp(2, cap);
            (0..n).map(|i| duration_s * i as f64 / (n - 1) as f64).collect()
        },
    };

    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    times.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    times
}

/// Keep the first/last sample and thin the interior evenly.
fn thin_to_cap(times: Vec<f64>, cap: usize) -> Vec<f64> {
    if times.len() <= cap || cap < 2 {
        return times;
    }
    let last_index = times.len() - 1;
    (0..cap)
        .map(|i| {
            let index = (i * last_index).div_ceil(cap - 1).min(last_index);
            times[index]
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_target(source: &str) -> BuildTarget {
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let report = animatix::timeline::Timeline::build_with_diagnostics(
            &ast.expect("ast"),
            &std::collections::HashMap::new(),
        );
        BuildTarget::SingleScene(report.output)
    }

    #[test]
    fn numeric_sampling_is_even_and_includes_endpoints() {
        let target =
            single_target("#0s\nbox: Rect, at: (0, 0), size: (10, 10)\n#2s\nfade-in box [1s]\n");
        let times = compute_sample_times(&target, &SampleSpec::Count(5), 16);
        assert_eq!(times.len(), 5);
        assert!(times[0].abs() < f64::EPSILON);
        assert!((times.last().copied().unwrap() - target_duration_s(&target)).abs() < 0.001);
    }

    #[test]
    fn keyframe_sampling_includes_start_and_rest_and_respects_cap() {
        let target = single_target(
            "#0s\nbox: Rect, at: (0, 0), size: (10, 10)\n#1s\nfade-in box [500ms]\n#3s\nfade-out box [500ms]\n",
        );
        let times = compute_sample_times(&target, &SampleSpec::Keyframes, 4);
        assert!(times.len() <= 4);
        assert_eq!(times[0], 0.0);
        assert!((times.last().copied().unwrap() - target_duration_s(&target)).abs() < 0.001);
    }

    #[test]
    fn zero_duration_is_a_single_start_sample() {
        let target = single_target("#0s\nbox: Rect, at: (0, 0), size: (10, 10)\n");
        for spec in [
            SampleSpec::Auto,
            SampleSpec::Keyframes,
            SampleSpec::Count(8),
        ] {
            assert_eq!(compute_sample_times(&target, &spec, 8), vec![0.0]);
        }
    }
}
