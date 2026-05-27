
pub mod behavior;
pub mod preview_canvas;
pub mod inspector;
pub mod timeline_panel;

fn default_actor_type() -> &'static str {
    animatix::primitives::actor_kind_registry()
        .iter()
        .find(|meta| {
            meta.category == animatix::timeline::ActorCategory::Shape && !meta.advanced
        })
        .map(|meta| meta.type_name)
        .unwrap_or("Rect")
}

#[allow(dead_code)]
fn transition_type_label(id: &str) -> &'static str {
    animatix::transition_registry::display_name(id)
}

/// Compute a "nice" tick interval for ruler marks.
/// Produces round numbers (1, 2, 5, 10, 20, 50, 100, ...).
fn nice_tick_interval(visible_range: f32, target_ticks: f32) -> f32 {
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

const RULER_SIZE: f32 = 20.0;

pub use crate::app::commands::{Command, CommandQueue, PropertyEdit, PropertyValue};
use crate::app::components;

use crate::app::icons::actor_icon_str;
use crate::app::design_tokens::*;
use crate::app::preview::{self, selection, ActorProps, DragState, fit_preview};
use crate::app::{FileTreeEntry, PreviewPaneState};
use crate::editor::EditorBuffer;
use animatix::diagnostics::Diagnostic;
use animatix::timeline::{PositionBinding, SceneDimensions, Timeline, TrackAccessor};
use egui::{Pos2, RichText, Stroke, Vec2};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SidebarTab {
    Explorer,
    Layers,
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

/// Uniform panel frame: 8 px padding, transparent fill.
fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::same(8))
}

fn render_sidebar_tab_bar(ui: &mut egui::Ui, active_tab: &mut SidebarTab) {
    let tabs = [
        (SidebarTab::Explorer, egui_phosphor::regular::FOLDER, "Explorer"),
        (SidebarTab::Layers, egui_phosphor::regular::STACK, "Layers"),
    ];
    if let Some(new_tab) = components::pill_tab_bar(ui, *active_tab, &tabs) {
        *active_tab = new_tab;
    }
}

impl WorkspaceViewer<'_> {
    fn get_actor_props(&self, actor: &str) -> Option<ActorProps> {
        let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
        self.get_actor_props_at_time(actor, time_ms)
    }

    fn get_actor_props_at_time(&self, actor: &str, time_ms: u64) -> Option<ActorProps> {
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene.as_ref()?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        })?;
        let track = timeline.get_track(actor)?;
        let half = track.size.as_ref().map(|pt| pt.evaluate(time_ms))?;
        let local_size = [half[0] * 2.0, half[1] * 2.0];
        let world_affine = timeline.actor_world_affine(actor, time_ms, self.scene_dimensions)?;
        let coeffs = world_affine.as_coeffs();
        let position = [coeffs[4] as f32, coeffs[5] as f32];
        let rotation = (coeffs[1] as f32).atan2(coeffs[0] as f32);
        let scale = ((coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt()) as f32;
        let size = [local_size[0] * scale, local_size[1] * scale];
        let pivot_offset = self.pivot_offsets.get(actor).copied().unwrap_or([0.0, 0.0]);
        Some(ActorProps { position, size, rotation, pivot_offset })
    }

    fn is_layout_managed(&self, actor: &str) -> bool {
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene.as_ref()?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        });
        let Some(timeline) = timeline else { return false; };
        let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
        preview::is_layout_managed(actor, timeline, time_ms)
    }

    fn find_layout_container(&self, actor: &str) -> Option<(String, animatix::timeline::LayoutType, usize)> {
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene.as_ref()?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        })?;
        let container = timeline
            .tracks()
            .iter()
            .find(|(_, track)| track.children.iter().any(|child| child == actor))?
            .0
            .clone();
        let metadata = timeline.container_metadata().get(&container)?;
        let source_index = timeline
            .get_track(&container)?
            .children
            .iter()
            .position(|child| child == actor)?;
        Some((container, metadata.layout_type, source_index))
    }

    pub(super) fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            let tab_id = ui.id().with("sidebar_tab");
            let mut active_tab = ui
                .data(|d| d.get_temp::<SidebarTab>(tab_id))
                .unwrap_or(SidebarTab::Explorer);

            let prev_tab_id = ui.id().with("sidebar_prev_tab");
            let prev_tab: Option<SidebarTab> = ui.data(|d| d.get_temp(prev_tab_id));

            render_sidebar_tab_bar(ui, &mut active_tab);
            ui.add_space(6.0);

            // Slide-in animation on tab switch
            let content_offset_id = ui.id().with("sidebar_slide");
            if prev_tab != Some(active_tab) {
                // Tab just switched — reset animation to start from an offset
                ui.ctx().animate_value_with_time(content_offset_id, 6.0, 0.0);
                ui.data_mut(|d| d.insert_temp(prev_tab_id, active_tab));
            }
            let offset = ui.ctx().animate_value_with_time(content_offset_id, 0.0, 0.12);
            if offset > 0.01 {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
            }

            ui.allocate_ui_with_layout(
                ui.available_size(),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.add_space(offset);
                    match active_tab {
                        SidebarTab::Explorer => self.explorer_content_ui(ui),
                        SidebarTab::Layers => self.layers_content_ui(ui),
                    }
                },
            );

            ui.data_mut(|d| d.insert_temp(tab_id, active_tab));
        });
    }

    fn explorer_content_ui(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
            for entry in self.file_tree {
                let is_selected = !entry.is_dir && entry.path == self.current_file;
                let is_expanded = entry.is_dir && self.expanded_dirs.contains(&entry.path);
                let has_children = entry.is_dir;

                let (icon, label_color) = if entry.is_dir {
                    let folder_icon = if is_expanded {
                        egui_phosphor::regular::FOLDER_OPEN
                    } else {
                        egui_phosphor::regular::FOLDER
                    };
                    (Some(folder_icon), None)
                } else {
                    let is_amx = entry.path.extension().and_then(|e| e.to_str()) == Some("amx");
                    let file_icon = if is_amx {
                        egui_phosphor::regular::FILM_STRIP
                    } else {
                        egui_phosphor::regular::FILE
                    };
                    let color = if is_amx { Some(ACCENT_BLUE) } else { None };
                    (Some(file_icon), color)
                };

                let row_id = ui.id().with(entry.path.display().to_string());
                let path = entry.path.clone();
                let is_dir = entry.is_dir;
                let response = components::Row::new(&entry.name)
                    .indent(entry.depth as f32 * 14.0)
                    .selected(is_selected)
                    .icon(icon)
                    .label_color(label_color.unwrap_or(TEXT_SECONDARY))
                    .has_children(has_children)
                    .expanded(is_expanded)
                    .show(ui, row_id);

                if response.chevron_clicked {
                    self.commands.push_back(Command::ToggleExpandDir(path.clone()));
                }
                if response.row_clicked {
                    if is_dir {
                        self.commands.push_back(Command::ToggleExpandDir(path));
                    } else {
                        self.commands.push_back(Command::OpenFile(path));
                    }
                }
            }
        });
    }

    fn layers_content_ui(&mut self, ui: &mut egui::Ui) {
        // For compositions, use the active scene's timeline
        let timeline = self.timeline.or_else(|| {
            let comp = self.composition?;
            let scene_name = self.active_scene.as_ref()?;
            comp.scenes.get(scene_name).map(|s| &s.timeline)
        });
        let Some(timeline) = timeline else {
            components::empty_state(ui, egui_phosphor::regular::FILM_STRIP, "No timeline loaded", "");
            return;
        };

        // Show which scene's actors are being displayed
        if self.is_composition {
            if let Some(scene_name) = self.active_scene.as_ref() {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("{} {}", egui_phosphor::regular::FILM_STRIP, scene_name))
                                .size(FONT_SIZE_S)
                                .color(TEXT_MUTED),
                        )
                        .selectable(false),
                    );
                });
                ui.add_space(4.0);
            }
        }

        let root_nodes = timeline.root_actor_labels();
        if root_nodes.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(SPACE_XL * 3.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(egui_phosphor::regular::FILM_STRIP)
                            .size(ROW_L)
                            .color(TEXT_MUTED),
                    )
                    .selectable(false),
                );
                ui.add_space(SPACE_M);
                ui.add(
                    egui::Label::new(
                        RichText::new("No actors in scene")
                            .size(FONT_SIZE_L)
                            .color(TEXT_SECONDARY),
                    )
                    .selectable(false),
                );
                ui.add_space(12.0);
                if ui
                    .button(
                        RichText::new(format!("{} Add Actor", egui_phosphor::regular::PLUS))
                            .size(FONT_SIZE_L)
                            .color(ACCENT_BLUE),
                    )
                    .clicked()
                {
                    let label = "rect1".to_string();
                    let pos = [
                        self.scene_dimensions.width as f32 / 2.0,
                        self.scene_dimensions.height as f32 / 2.0,
                    ];
                    self.commands.push_back(Command::CreateActor { ty: default_actor_type().into(), label, position: pos });
                }
            });
            return;
        }

        let time_ms = (self.preview.playback.current_time_s * 1000.0) as u64;
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for root_label in root_nodes {
                    render_actor_tree(
                        ui,
                        timeline,
                        root_label,
self.selected_actors,
                        self.collapsed_actors,
                        self.commands,
                        time_ms,
                        0,
                    );
                }
            });
    }

    pub(super) fn editor_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            self.editor.set_diagnostics(self.diagnostics);
            let response = self.editor.show(ui);
            if response.changed() || self.editor.text() != self.source_dirty.as_str() {
                *self.source_dirty = self.editor.text().to_string();
                self.commands.push_back(Command::EditorChanged);
            }
            if let Some(time_s) = self.editor.pending_scrub_to_time.take() {
                self.commands.push_back(Command::ScrubTo(time_s));
                if !self.preview.playback.is_playing {
                    self.commands.push_back(Command::TogglePlayback);
                }
            }
            // ScrollToLine commands are handled by the shell in app/mod.rs
        });
    }

    pub(super) fn inspector_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            let current_time_s = self.preview.playback.current_time_s;
            // For compositions, use the active scene's timeline.
            // During a transition, if the selected actor is in the other scene,
            // use that scene's timeline so the inspector shows correct properties.
            let timeline = self.timeline.or_else(|| {
                let comp = self.composition?;
                let active_scene = self.active_scene.as_ref()?;
                let active_has_actor = self.selected_actors.iter().next().is_some_and(|sel| {
                    comp.scenes.get(active_scene.as_str()).is_some_and(|s| s.timeline.has_actor(sel))
                });
                if !active_has_actor {
                    let (_, _, transition) = comp.evaluate(current_time_s);
                    if transition.is_some() {
                        for (name, scene) in &comp.scenes {
                            if name != active_scene
                                && self.selected_actors.iter().any(|sel| scene.timeline.has_actor(sel)) {
                                    return Some(&scene.timeline);
                                }
                        }
                    }
                }
                comp.scenes.get(active_scene.as_str()).map(|s| &s.timeline)
            });
            inspector::inspector_ui(ui, timeline, self.selected_actors, current_time_s, self.commands, self.keyframe_mode, self.scene_dimensions, self.pivot_offsets);
        });
    }

    pub(super) fn timeline_ui(&mut self, ui: &mut egui::Ui) {
        panel_frame().show(ui, |ui| {
            // For compositions, use the active scene's timeline
            let timeline = self.timeline.or_else(|| {
                let comp = self.composition?;
                let scene_name = self.active_scene.as_ref()?;
                comp.scenes.get(scene_name).map(|s| &s.timeline)
            });
            timeline_panel::timeline_panel_ui(
                ui,
                self.preview,
                timeline,
                self.composition,
                self.active_scene.as_deref(),
                self.commands,
            );
        });
    }
}

// ─── Layer Tree ─────────────────────────────────────────────────────────────

fn render_actor_tree(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    label: &str,
    selected_actors: &mut HashSet<String>,
    collapsed_actors: &mut HashSet<String>,
    commands: &mut CommandQueue,
    time_ms: u64,
    depth: usize,
) {
    let Some(track) = timeline.get_track(label) else {
        return;
    };

    let is_selected = selected_actors.contains(label);
    let is_anonymous = label.starts_with("__anon");
    let has_children = !track.children.is_empty();
    let is_expanded = has_children && !collapsed_actors.contains(label);
    let opacity = track.opacity.get(time_ms, 1.0);
    let is_visible = opacity > 0.001;

    let (icon, display_label, label_color) = if is_anonymous {
        (
            Some(egui_phosphor::regular::GHOST),
            "anon",
            Some(TEXT_MUTED),
        )
    } else {
        let icon = Some(actor_icon_str(track.kind));
        (icon, label, None)
    };

    // Use the unique actor label (not display_label) for the row id
    // so that multiple anonymous actors don't share the same id.
    let row_id = ui.id().with(label);
    let label_owned = label.to_string();

    // Visibility toggle (eye icon) on the right
    let eye_icon = if is_visible {
        egui_phosphor::regular::EYE
    } else {
        egui_phosphor::regular::EYE_CLOSED
    };
    let eye_color = if is_visible { TEXT_SECONDARY } else { TEXT_DISABLED };

    let response = components::Row::new(display_label)
        .indent(depth as f32 * 14.0)
        .selected(is_selected)
        .icon(icon)
        .label_color(label_color.unwrap_or(if is_visible { TEXT_SECONDARY } else { TEXT_DISABLED }))
        .has_children(has_children)
        .expanded(is_expanded)
        .sense(egui::Sense::click_and_drag())
        .right(|ui| {
            let eye_btn = components::icon_button_colored(
                ui,
                eye_icon,
                if is_visible { "Hide layer" } else { "Show layer" },
                eye_color,
                TEXT_PRIMARY,
            );
            if eye_btn.clicked() {
                let new_opacity = if is_visible { 0.0 } else { 1.0 };
                commands.push_back(Command::PropertyEdit(PropertyEdit {
                    actor: label.to_string(),
                    property: "opacity".into(),
                    value: PropertyValue::Float(new_opacity),
                    create_keyframe: false,
                }));
            }
        })
        .show(ui, row_id);

    // ── Drag-and-drop reparenting ──
    let drag_id = ui.id().with("layer_drag");
    let drag_data_id = drag_id.with("data");

    // Detect drag start on this row
    if response.drag_started && !is_anonymous {
        ui.data_mut(|d| d.insert_temp(drag_data_id, label.to_string()));
    }

    // Detect drop target
    let is_dragging = ui.data(|d| d.get_temp::<String>(drag_data_id)).is_some();
    let is_drop_target = is_dragging && response.hovered && !is_anonymous;
    if is_drop_target {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if dragged != label {
            // Highlight as drop target
            ui.painter().rect_stroke(
                response.row_rect.expand(1.0),
                2,
                Stroke::new(1.5, ACCENT_BLUE),
                egui::StrokeKind::Outside,
            );
        }
    }

    // Handle drop (pointer released while dragging)
    if is_dragging && ui.input(|i| i.pointer.any_released()) {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if !dragged.is_empty() && dragged != label {
            let pointer_pos = ui.input(|i| i.pointer.latest_pos());
            let over_this_row = pointer_pos.is_some_and(|p| response.row_rect.contains(p));
            if over_this_row && is_drop_target {
                // Dropped on this row — reparent under this actor
                commands.push_back(Command::ReparentActor { actor: dragged.clone(), new_parent: Some(label.to_string()) });
            } else if !over_this_row && depth == 0 {
                // Dropped outside any row at root level — reparent to top-level
                // Only the root-level rows handle this to avoid duplicates
                let over_any_root = pointer_pos.is_some_and(|p| {
                    // Check if pointer is within the scroll area at all
                    response.row_rect.expand(100.0).contains(p)
                });
                if over_any_root {
                    commands.push_back(Command::ReparentActor { actor: dragged.clone(), new_parent: None });
                }
            }
        }
        ui.data_mut(|d| d.remove::<String>(drag_data_id));
    }

    if response.chevron_clicked {
        if collapsed_actors.contains(&label_owned) {
            collapsed_actors.remove(&label_owned);
        } else {
            collapsed_actors.insert(label_owned.clone());
        }
    }

    if response.row_clicked {
        let modifiers = ui.ctx().input(|i| i.modifiers);
        let multi = modifiers.shift || modifiers.ctrl || modifiers.command;
        if multi {
            if selected_actors.contains(label) {
                selected_actors.remove(label);
            } else {
                selected_actors.insert(label.to_string());
            }
        } else {
            selected_actors.clear();
            selected_actors.insert(label.to_string());
        }
    }

    // Children
    if is_expanded {
        for child_label in &track.children {
            render_actor_tree(
                ui,
                timeline,
                child_label,
                selected_actors,
                collapsed_actors,
                commands,
                time_ms,
                depth + 1,
            );
        }
    }
}
