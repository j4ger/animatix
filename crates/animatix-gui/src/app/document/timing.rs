//! Timeline duration and keyframe-time helpers shared across the GUI.

use animatix::composition::Composition;
use animatix::timeline::Timeline;

/// Test-only: `DocumentSession::rebuild` computes `duration_s` from the built
/// program directly; this helper exists for the duration tests.
#[cfg(test)]
pub fn timeline_duration_seconds(
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
) -> f64 {
    if let Some(timeline) = timeline {
        timeline
            .tracks()
            .iter()
            .map(|(_, id)| timeline.world().max_keyframe_time(id).unwrap_or(0))
            .max()
            .unwrap_or(0) as f64
            / 1000.0
    } else {
        composition.map(|c| c.global_duration_s).unwrap_or(0.0)
    }
}

pub fn timeline_keyframe_times_s(
    timeline: Option<&Timeline>,
    composition: Option<&Composition>,
    _active_scene: Option<&str>,
) -> Vec<f64> {
    if let Some(timeline) = timeline {
        timeline.keyframe_times_s()
    } else if let Some(composition) = composition {
        // Collect keyframes from all scenes, converting local times to global times
        let mut all_keyframes: Vec<f64> = Vec::new();
        for (scene_name, scene) in &composition.scenes {
            let start_s = composition.scene_start_times.get(scene_name).copied().unwrap_or(0.0);
            for local_time in scene.timeline.keyframe_times_s() {
                all_keyframes.push(start_s + local_time);
            }
        }
        all_keyframes.sort_by(|a, b| a.total_cmp(b));
        all_keyframes.dedup_by(|a, b| (*a - *b).abs() < 0.001);
        all_keyframes
    } else {
        Vec::new()
    }
}

/// Match the scene actually displayed at a completed transition / instant cut.
/// `Composition::evaluate` retains the outgoing scene during its boundary epsilon.
pub(crate) fn review_scene_at_time(composition: &Composition, time_s: f64) -> Option<String> {
    let (scene, _, blend) = composition.evaluate(time_s);
    let scene = match blend {
        Some(blend) if blend.progress >= 1.0 => blend.to_scene,
        _ => scene,
    };
    (!scene.is_empty()).then_some(scene)
}

#[cfg(test)]
mod review_tests {
    use super::*;
    #[test]
    fn exact_cut_boundary_inspects_the_visible_incoming_scene() {
        let mut document = crate::app::document::DocumentSession::from_source(
            "cut.amx".into(),
            "# Intro\nconfig { duration: 3 }\n#0s\na: Rect\n# Main\nconfig { duration: 6 }\n#0s\nb: Rect".into(),
        ).unwrap();
        document.rebuild().unwrap();
        let composition = document.composition.as_ref().unwrap();
        assert_eq!(review_scene_at_time(composition, 2.9).as_deref(), Some("Intro"));
        assert_eq!(review_scene_at_time(composition, 3.0).as_deref(), Some("Main"));
    }
}
