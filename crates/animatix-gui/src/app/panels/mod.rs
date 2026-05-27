
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
