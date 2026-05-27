
pub mod behavior;
pub mod inspector;
pub mod timeline_panel;

pub mod sidebar;
pub mod editor;
pub mod inspector_panel;
pub mod timeline;
pub mod preview_panel;

pub use crate::app::commands::{CommandQueue, PropertyEdit, PropertyValue};
use crate::app::preview::{self, selection, DragState};
use crate::app::{FileTreeEntry, PreviewPaneState};
use crate::app::panels::inspector::{PropertyViewMode, KeyframeViewMode};
use crate::editor::EditorBuffer;
use animatix::diagnostics::Diagnostic;
use animatix::primitives;
use animatix::timeline::{SceneDimensions, Timeline};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SidebarTab {
    Explorer,
    Layers,
}

/// Returns the canonical default actor type: the first non-advanced Shape actor.
pub(crate) fn default_actor_type() -> &'static str {
    primitives::actor_kind_registry()
        .iter()
        .find(|meta| {
            meta.category == animatix::timeline::ActorCategory::Shape && !meta.advanced
        })
        .map(|meta| meta.type_name)
        .unwrap_or("Rect")
}

pub(crate) struct WorkspaceViewer<'a> {
    #[allow(dead_code)]
    pub(super) scene_names: Vec<String>,
    #[allow(dead_code)]
    pub(super) import_aliases: Vec<String>,
    pub(super) active_scene: Option<String>,
    pub(super) is_composition: bool,
    pub(super) composition: Option<&'a animatix::composition::Composition>,
    pub(super) current_file: &'a Path,
    pub(super) expanded_dirs: &'a mut HashSet<PathBuf>,
    pub(super) file_tree: &'a [FileTreeEntry],
    pub(super) editor: &'a mut EditorBuffer,
    pub(super) preview: &'a mut PreviewPaneState,
    #[allow(dead_code)]
    pub(super) panel_state: &'a mut crate::app::PanelState,
    pub(super) diagnostics: &'a [Diagnostic],
    pub(super) preview_texture_id: Option<egui::TextureId>,
    pub(super) commands: &'a mut CommandQueue,
    pub(super) source_dirty: &'a mut String,
    pub(super) scene_dimensions: SceneDimensions,
    pub(super) timeline: Option<&'a Timeline>,
    pub(super) selected_actors: &'a mut HashSet<String>,
    pub(super) hit_regions: &'a [(String, kurbo::Rect)],
    pub(super) drag_state: &'a mut DragState,
    pub(super) selection: &'a mut selection::SelectionState,
    /// When true, property edits should create keyframes instead of overwriting defaults.
    pub(super) keyframe_mode: bool,
    /// Actor labels that the user have explicitly collapsed in the layer tree.
    pub(super) collapsed_actors: &'a mut HashSet<String>,
    /// Per-actor pivot offsets in object-local space (relative to actor centre).
    pub(super) pivot_offsets: &'a mut HashMap<String, [f32; 2]>,
    /// Active tool mode for preview interactions.
    pub(super) tool_mode: &'a mut preview::ToolMode,
    /// Rotation snap increment in degrees (Shift+rotate).
    pub(super) rotation_snap_degrees: f32,
    pub(super) sidebar_tab: &'a mut SidebarTab,
    pub(super) property_view_mode: &'a mut PropertyViewMode,
    pub(super) keyframe_view_mode: &'a mut KeyframeViewMode,
}

/// Compute a "nice" tick interval for ruler marks.
/// Produces round numbers (1, 2, 5, 10, 20, 50, 100, ...).
pub(super) fn nice_tick_interval(visible_range: f32, target_ticks: f32) -> f32 {
    let raw = (visible_range / target_ticks).abs();
    if raw <= 0.0 {
        return 1.0;
    }
    let magnitude = 10.0_f32.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let nice_mul = if normalized < 1.5 {
        1.0
    } else if normalized < 3.5 {
        2.0
    } else if normalized < 7.5 {
        5.0
    } else {
        10.0
    };
    nice_mul * magnitude
}

pub(super) const RULER_SIZE: f32 = 20.0;

/// Uniform panel frame: 8 px padding, transparent fill.
pub(super) fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::same(8))
}

impl WorkspaceViewer<'_> {
    pub(super) fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        let mut ctx = sidebar::SidebarViewer {
            active_scene: self.active_scene.as_deref(),
            is_composition: self.is_composition,
            composition: self.composition,
            current_file: self.current_file,
            expanded_dirs: self.expanded_dirs,
            file_tree: self.file_tree,
            preview: self.preview,
            commands: self.commands,
            scene_dimensions: self.scene_dimensions,
            timeline: self.timeline,
            selected_actors: self.selected_actors,
            collapsed_actors: self.collapsed_actors,
            sidebar_tab: self.sidebar_tab,
        };
        sidebar::sidebar_ui(&mut ctx, ui);
    }

    pub(super) fn editor_ui(&mut self, ui: &mut egui::Ui) {
        let mut ctx = editor::EditorViewer {
            editor: self.editor,
            diagnostics: self.diagnostics,
            source_dirty: self.source_dirty,
            commands: self.commands,
            is_playing: self.preview.playback.is_playing,
        };
        editor::editor_ui(&mut ctx, ui);
    }

    pub(super) fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        let mut ctx = inspector_panel::InspectorViewer {
            preview: self.preview,
            timeline: self.timeline,
            composition: self.composition,
            active_scene: self.active_scene.as_deref(),
            selected_actors: self.selected_actors,
            commands: self.commands,
            keyframe_mode: self.keyframe_mode,
            scene_dimensions: self.scene_dimensions,
            pivot_offsets: self.pivot_offsets,
            property_view_mode: self.property_view_mode,
            keyframe_view_mode: self.keyframe_view_mode,
        };
        inspector_panel::inspector_ui(&mut ctx, ui);
    }

    pub(super) fn timeline_ui(&mut self, ui: &mut egui::Ui) {
        let mut ctx = timeline::TimelineViewer {
            preview: self.preview,
            timeline: self.timeline,
            composition: self.composition,
            active_scene: self.active_scene.as_deref(),
            commands: self.commands,
            collapsed_actors: self.collapsed_actors,
        };
        timeline::timeline_ui(&mut ctx, ui);
    }

    pub(super) fn preview_ui(&mut self, ui: &mut egui::Ui) {
        let mut ctx = preview_panel::PreviewViewer {
            scene_dimensions: self.scene_dimensions,
            preview: self.preview,
            preview_texture_id: self.preview_texture_id,
            commands: self.commands,
            drag_state: self.drag_state,
            selection: self.selection,
            selected_actors: self.selected_actors,
            hit_regions: self.hit_regions,
            timeline: self.timeline,
            pivot_offsets: self.pivot_offsets,
            tool_mode: self.tool_mode,
            rotation_snap_degrees: self.rotation_snap_degrees,
            composition: self.composition,
            active_scene: self.active_scene.as_deref(),
            keyframe_mode: self.keyframe_mode,
        };
        preview_panel::preview_ui(&mut ctx, ui);
    }
}

#[cfg(test)]
mod tests {
    use super::nice_tick_interval;

    #[test]
    fn nice_tick_interval_normal_range() {
        // visible_range=100.0, target_ticks=10 → raw=10 → magnitude=10 → normalized=1 → nice_mul=1 → 10.0
        let interval = nice_tick_interval(100.0, 10.0);
        assert!((interval - 10.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_rounds_to_two() {
        // visible_range=50.0, target_ticks=10 → raw=5 → magnitude=1 → normalized=5 → nice_mul=5 → 5.0
        let interval = nice_tick_interval(50.0, 10.0);
        assert!((interval - 5.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_small_values() {
        // visible_range=0.5, target_ticks=10 → raw=0.05 → magnitude=0.01 → normalized=5 → nice_mul=5 → 0.05
        let interval = nice_tick_interval(0.5, 10.0);
        assert!(interval > 0.0);
        assert!((interval / 0.01 - 5.0).abs() < 0.001 || (interval / 0.05 - 1.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_zero_range() {
        // raw=0.0 → early return 1.0
        assert_eq!(nice_tick_interval(0.0, 10.0), 1.0);
    }

    #[test]
    fn nice_tick_interval_negative_range() {
        // abs(visible_range) used
        assert_eq!(nice_tick_interval(-100.0, 10.0), 10.0);
    }

    #[test]
    fn nice_tick_interval_large_range_gives_round_numbers() {
        // visible_range=10000.0, target_ticks=10 → raw=1000 → magnitude=100 → normalized=10 → nice_mul=10 → 1000.0
        let interval = nice_tick_interval(10000.0, 10.0);
        assert!((interval - 1000.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_always_positive() {
        for &range in &[0.1, 1.0, 10.0, 100.0, 1000.0] {
            let interval = nice_tick_interval(range, 10.0);
            assert!(interval > 0.0, "interval must be positive for range={}", range);
        }
    }

    #[test]
    fn nice_tick_interval_boundary_near_one_point_five() {
        // raw just below 1.5 → nice_mul=1
        let interval = nice_tick_interval(14.9, 10.0);
        assert!((interval - 1.0).abs() < 0.001);

        // raw just above 1.5 → nice_mul=2
        let interval = nice_tick_interval(15.1, 10.0);
        // raw=1.51 → magnitude=1 → normalized=1.51 → nice_mul=2 → 2.0
        assert!((interval - 2.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_boundary_near_three_point_five() {
        // raw just below 3.5 → nice_mul=2
        let interval = nice_tick_interval(34.9, 10.0);
        assert!((interval - 2.0).abs() < 0.001);

        // raw just above 3.5 → nice_mul=5
        let interval = nice_tick_interval(35.1, 10.0);
        assert!((interval - 5.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_boundary_near_seven_point_five() {
        // raw just below 7.5 → nice_mul=5
        let interval = nice_tick_interval(74.9, 10.0);
        assert!((interval - 5.0).abs() < 0.001);

        // raw just above 7.5 → nice_mul=10
        let interval = nice_tick_interval(75.1, 10.0);
        assert!((interval - 10.0).abs() < 0.001);
    }
}
