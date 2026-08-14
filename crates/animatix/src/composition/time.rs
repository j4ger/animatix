use super::{Composition, TransitionBlend};

impl Composition {
    /// Evaluate the composition at a given global time.
    ///
    /// Returns the active scene name, local time within that scene, and
    /// optional transition blend info if currently in a transition period.
    pub fn evaluate(&self, global_time_s: f64) -> (String, f64, Option<TransitionBlend>) {
        let t = global_time_s.max(0.0).min(self.global_duration_s.max(0.001));

        // Find all scenes whose [start, start+duration) range contains t.
        // During a transition, exactly two scenes will be active.
        let mut active: Vec<(&String, f64, f64)> = Vec::new();

        for (name, scene) in &self.scenes {
            let start = self.scene_start_times.get(name).copied().unwrap_or(0.0);
            let end = start + scene.duration_s;

            if t >= start && t < end + 0.001 {
                // Small epsilon to handle boundary cases
                active.push((name, start, end));
            }
        }

        if active.len() == 2 {
            // Sort active scenes by start time so active[0] is the outgoing (earlier) scene
            active.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

            // We're in a transition period
            let (from_name, from_start, _from_end) = active[0];
            let (to_name, to_start, _) = active[1];

            let from_local = t - from_start;
            let to_local = t - to_start;

            let edge = self.edges.get(from_name);
            let id = edge.map(|e| e.transition.id.clone()).unwrap_or_else(|| "cut".into());
            let transition_duration_s =
                edge.map(|e| e.transition.duration_ms as f64 / 1000.0).unwrap_or(0.0);
            let easing = edge.map(|e| e.transition.easing).unwrap_or(crate::easing::Easing::Linear);

            let progress = if transition_duration_s > 0.0 {
                ((t - to_start) / transition_duration_s).clamp(0.0, 1.0)
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
        } else if let Some((name, start, _end)) = active.first() {
            // Single active scene — no transition
            let local = t - start;
            (name.to_string(), local, None)
        } else {
            // t is at or beyond the end — return last scene at final frame
            if let Some(last_name) = self.declaration_order.last() {
                if let Some(last_scene) = self.scenes.get(last_name) {
                    return (last_name.clone(), last_scene.duration_s, None);
                }
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
        let end = start + scene.duration_s;

        // Consider transition overlap: a scene may be active slightly beyond its
        // nominal end due to transition blending.
        let edge = self.edges.get(scene_name);
        let transition_overlap =
            edge.map(|e| e.transition.duration_ms as f64 / 1000.0).unwrap_or(0.0);

        if global_time_s >= *start && global_time_s < end + transition_overlap {
            Some(global_time_s - start)
        } else {
            None
        }
    }
}
