//! Session queries: active timeline/scene resolution and keyframe lookup.

use animatix::timeline::{Timeline, TimelineIndex};
use animatix_syntax::ast::Stmt;

use super::DocumentSession;
use super::timing::timeline_keyframe_times_s;

impl DocumentSession {
    pub fn raw_program_statements(&self) -> Option<&[Stmt]> {
        self.raw_statements.as_deref()
    }

    /// Find the 0-indexed source line of the keyframe whose absolute time
    /// is closest to and ≤ `time_s`. Returns `None` if no keyframe exists.
    pub fn find_keyframe_line_at(&self, time_s: f64) -> Option<usize> {
        self.active_timeline()?;
        self.timeline_index.line_for_time((time_s * 1000.0) as u64)
    }

    /// Find the absolute time of the keyframe immediately before `time_s`.
    pub fn prev_keyframe_time(&self, time_s: f64) -> f64 {
        let scene = self.active_scene.as_deref();
        let keyframes = if let Some(composition) = self.composition.as_ref() {
            let Some(scene_name) = scene else {
                return 0.0;
            };
            let Some(scene) = composition.scenes.get(scene_name) else {
                return 0.0;
            };
            scene.timeline.keyframe_times_s()
        } else {
            timeline_keyframe_times_s(self.active_timeline(), None, scene)
        };
        if !keyframes.is_empty() {
            return keyframes.into_iter().rev().find(|t| *t <= time_s).unwrap_or(0.0);
        }

        self.timeline_index
            .prev_keyframe_time((time_s * 1000.0) as u64)
            .map(|ms| ms as f64 / 1000.0)
            .unwrap_or(0.0)
    }

    /// Rescan source text for keyframes and update cached maps.
    /// Call this after direct source text modifications (e.g. keyframe insertion)
    /// so the editor decorations stay in sync before the next rebuild.
    pub fn rescan_keyframe_lines(&mut self) {
        self.timeline_index = TimelineIndex::build(&self.source_text);
        self.keyframe_lines = self.timeline_index.keyframes.iter().map(|(_, line)| *line).collect();
    }

    pub fn is_composition(&self) -> bool {
        self.composition.is_some()
    }

    /// Interactive pause points across the document in seconds.
    pub fn pause_points_s(&self) -> Vec<f64> {
        if let Some(comp) = &self.composition {
            comp.pause_points().iter().map(|&ms| ms as f64 / 1000.0).collect()
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

    /// Returns a mutable `ActiveTimelineMut` for editing the active scene.
    /// For single-scene documents, returns the sole timeline mutably.
    /// For compositions, resolves the active scene similarly.
    pub fn active_timeline_mut(
        &mut self,
    ) -> Option<crate::app::document::active_timeline::ActiveTimelineMut<'_>> {
        use crate::app::document::active_timeline::{ActiveSceneId, ActiveTimelineMut};

        if let Some(timeline) = self.timeline.as_mut() {
            return Some(ActiveTimelineMut {
                id: ActiveSceneId::SingleScene,
                timeline,
                scene_name: None,
            });
        }

        let composition = self.composition.as_mut()?;
        let scene_name = self
            .active_scene
            .clone()
            .or_else(|| composition.declaration_order.first().cloned())
            .or_else(|| composition.scenes.keys().next().cloned())?;

        if !composition.scenes.contains_key(&scene_name) {
            return None;
        }
        let scene = composition.scenes.get_mut(&scene_name)?;
        Some(ActiveTimelineMut {
            id: ActiveSceneId::Scene(scene_name.clone()),
            timeline: &mut scene.timeline,
            scene_name: Some(scene_name),
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

    pub fn import_aliases(&self) -> Vec<String> {
        let mut aliases = self.namespaces.keys().cloned().collect::<Vec<_>>();
        aliases.sort();
        aliases
    }
}
