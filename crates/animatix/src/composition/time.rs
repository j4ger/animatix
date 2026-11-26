use super::{Composition, TransitionBlend};

use crate::time::{add_seconds, scene_local_seconds};

impl Composition {
    /// Evaluate the composition at a given global time.
    ///
    /// Returns the active scene name, local time within that scene, and
    /// optional transition blend info if currently in a transition period.
    pub fn evaluate(&self, global_time_s: f64) -> (String, f64, Option<TransitionBlend>) {
        let t = global_time_s.max(0.0).min(self.global_duration_s);

        // Half-open intervals: at an exact cut or blend end, the outgoing
        // scene is done. Playback order also breaks ties between equal starts.
        let mut active = self.playback_order().iter().rev().filter_map(|name| {
            let start = *self.scene_start_times.get(name)?;
            let end = add_seconds(start, self.scenes.get(name)?.duration_s);
            (t >= start && t < end).then_some((name, start))
        });
        let incoming = active.next();
        let outgoing = active.next();
        if let (Some((to_name, to_start)), Some((from_name, from_start))) = (incoming, outgoing)
            && let Some(edge) = self.edges.get(from_name).filter(|edge| &edge.to_scene == to_name)
        {
            let from_local = scene_local_seconds(t, from_start);
            let to_local = scene_local_seconds(t, to_start);

            let id = edge.transition.id.clone();
            let transition_duration_s = edge.transition.duration_ms as f64 / 1000.0;
            let easing = edge.transition.easing;

            let progress = if transition_duration_s > 0.0 {
                (to_local / transition_duration_s).clamp(0.0, 1.0)
            } else {
                1.0 // Instant cut
            };

            let eased_progress = crate::easing::apply_easing(progress as f32, easing) as f64;

            (
                from_name.clone(),
                from_local,
                Some(TransitionBlend {
                    from_scene: from_name.clone(),
                    to_scene: to_name.clone(),
                    from_local,
                    to_local,
                    progress,
                    eased_progress,
                    id,
                    easing,
                }),
            )
        } else if let Some((name, start)) = incoming {
            // Single active scene — no transition
            let local = scene_local_seconds(t, start);
            (name.to_string(), local, None)
        } else {
            // t is at or beyond the end — return last scene at final frame
            if let Some(last_name) = self.playback_order().last()
                && let Some(last_scene) = self.scenes.get(last_name)
            {
                return (last_name.clone(), last_scene.duration_s, None);
            }
            ("".to_string(), 0.0, None)
        }
    }

    /// Get local time within a specific scene from global time.
    ///
    /// Returns `None` if the global time falls outside the scene's active period.
    pub fn local_time_for_scene(&self, scene_name: &str, global_time_s: f64) -> Option<f64> {
        let start = self.scene_start_times.get(scene_name)?;
        let scene = self.scenes.get(scene_name)?;
        let end = add_seconds(*start, scene.duration_s);

        // The outgoing transition occupies the tail of this scene's interval.
        if global_time_s >= *start && global_time_s < end {
            Some(scene_local_seconds(global_time_s, *start))
        } else {
            None
        }
    }
}
