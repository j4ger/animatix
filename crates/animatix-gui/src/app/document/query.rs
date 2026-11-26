//! Session queries: active timeline/scene resolution and keyframe lookup.

use animatix::timeline::Timeline;

use super::DocumentSession;

impl DocumentSession {
    /// Find the 0-indexed source line of the keyframe whose absolute time
    /// is closest to and ≤ `time_s`. Returns `None` if no keyframe exists.
    pub fn find_keyframe_line_at(&self, time_s: f64) -> Option<usize> {
        let scene = self.composition.as_ref().and_then(|c| super::review_scene_at_time(c, time_s));
        self.source_timing.playing_line(time_s, scene.as_deref())
    }

    pub fn is_composition(&self) -> bool {
        self.composition.is_some()
    }

    /// Interactive pause points across the document in seconds.
    pub fn pause_points_s(&self) -> Vec<f64> {
        if let Some(comp) = &self.composition {
            comp.pause_points_s().to_vec()
        } else if let Some(tl) = &self.timeline {
            tl.pause_points().iter().map(|&ms| ms as f64 / 1000.0).collect()
        } else {
            Vec::new()
        }
    }

    pub fn active_timeline(&self) -> Option<&Timeline> {
        self.active_timeline_ref().map(|r| r.timeline)
    }

    /// Returns an `ActiveTimelineRef` with full scene context.
    /// For single-scene documents, returns a reference directly.
    /// For compositions, resolves the active scene (or falls back to
    /// declaration order / entry scene).
    pub fn active_timeline_ref(
        &self,
    ) -> Option<crate::app::document::active_timeline::ActiveTimelineRef<'_>> {
        use crate::app::document::active_timeline::{ActiveSceneId, ActiveTimelineRef};

        if let Some(timeline) = self.timeline.as_ref() {
            return Some(ActiveTimelineRef {
                id: ActiveSceneId::SingleScene,
                timeline,
                composition: None,
                scene_name: None,
                duration_s: self.duration_s,
                dimensions: self.scene_dimensions,
            });
        }

        let composition = self.composition.as_ref()?;
        let (scene_name, scene) = self
            .active_scene
            .as_deref()
            .and_then(|name| composition.scenes.get(name).map(|s| (name, s)))
            .or_else(|| {
                composition
                    .declaration_order
                    .first()
                    .and_then(|name| composition.scenes.get(name).map(|s| (name.as_str(), s)))
            })
            .or_else(|| composition.scenes.iter().next().map(|(name, s)| (name.as_str(), s)))?;

        Some(ActiveTimelineRef {
            id: ActiveSceneId::Scene(scene_name.to_string()),
            timeline: &scene.timeline,
            composition: Some(composition),
            scene_name: Some(scene_name),
            duration_s: scene.duration_s.max(0.1),
            dimensions: self.scene_dimensions,
        })
    }

    /// Resolve an export target for the given scope.
    /// For single-scene documents, always returns the timeline.
    /// For compositions, returns the whole composition or active scene timeline.
    pub fn export_target(
        &self,
        scope: crate::app::document::export_target::ExportScope,
    ) -> Option<crate::app::document::export_target::ExportTargetRef<'_>> {
        use crate::app::document::export_target::{ExportScope, ExportTargetRef};

        match scope {
            ExportScope::ActiveScene | ExportScope::Scene(_) => {
                let tr = self.active_timeline_ref()?;
                Some(ExportTargetRef::Timeline {
                    timeline: tr.timeline,
                    duration_s: tr.duration_s,
                    dimensions: tr.dimensions,
                })
            },
            ExportScope::WholeComposition => {
                let composition = self.composition.as_ref()?;
                Some(ExportTargetRef::Composition {
                    composition,
                    duration_s: composition.global_duration_s.max(0.1),
                    dimensions: self.scene_dimensions,
                })
            },
        }
    }

    pub fn scene_names(&self) -> &[String] {
        self.composition
            .as_ref()
            .map(|composition| composition.declaration_order.as_slice())
            .unwrap_or(&[])
    }
}
