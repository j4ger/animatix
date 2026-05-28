//! Sidebar panel: file explorer tree and layer tree.
//!
//! Focused context struct borrows only the fields the sidebar needs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use egui::{RichText, Vec2};

use crate::app::commands::{Command, CommandQueue};
use crate::app::components;
use crate::app::design_tokens::*;
use crate::app::icons::actor_icon_str;
use crate::app::panels::SidebarTab;
use crate::app::{FileTreeEntry, PreviewPaneState};
use animatix::timeline::{Timeline, SceneDimensions, TrackAccessor};

/// Id used to persist the explorer filter string in egui's data store.
const EXPLORER_FILTER_ID: &str = "explorer_filter";

pub(crate) struct SidebarViewer<'a> {
    pub active_scene: Option<&'a str>,
    pub is_composition: bool,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub current_file: &'a Path,
    pub expanded_dirs: &'a mut HashSet<PathBuf>,
    pub file_tree: &'a [FileTreeEntry],
    pub preview: &'a mut PreviewPaneState,
    pub commands: &'a mut CommandQueue,
    pub scene_dimensions: SceneDimensions,
    pub timeline: Option<&'a Timeline>,
    pub selected_actors: &'a mut HashSet<String>,
    pub collapsed_actors: &'a mut HashSet<String>,
    pub sidebar_tab: &'a mut SidebarTab,
}

/// Uniform panel frame: 8 px padding, transparent fill.
fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::same(8))
}

pub(crate) fn sidebar_ui(ctx: &mut SidebarViewer<'_>, ui: &mut egui::Ui) {
    panel_frame().show(ui, |ui| {
        let mut active_tab = *ctx.sidebar_tab;
        let prev_tab = *ctx.sidebar_tab;

        render_sidebar_tab_bar(ui, &mut active_tab);
        ui.add_space(SPACE_M);

        // Slide-in animation on tab switch
        let content_offset_id = ui.id().with("sidebar_slide");
        if prev_tab != active_tab {
            ui.ctx().animate_value_with_time(content_offset_id, 6.0, 0.0);
            // Clear explorer filter when switching away from the Explorer tab
            if active_tab == SidebarTab::Layers {
                ui.data_mut(|d| d.remove::<String>(egui::Id::new(EXPLORER_FILTER_ID)));
            }
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
                    SidebarTab::Explorer => explorer_content_ui(ctx, ui),
                    SidebarTab::Layers => layers_content_ui(ctx, ui),
                }
            },
        );

        *ctx.sidebar_tab = active_tab;
    });
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

fn explorer_content_ui(ctx: &mut SidebarViewer<'_>, ui: &mut egui::Ui) {
    // ── Filter input ────────────────────────────────────────────────────────
    let filter_id = egui::Id::new(EXPLORER_FILTER_ID);
    let mut filter = ui.data(|d| d.get_temp::<String>(filter_id)).unwrap_or_default();

    ui.horizontal(|ui| {
        ui.add_space(SPACE_S);
        let response = ui.add(
            egui::TextEdit::singleline(&mut filter)
                .hint_text("Filter files…")
                .desired_width(f32::INFINITY),
        );
        if response.changed() {
            ui.data_mut(|d| d.insert_temp(filter_id, filter.clone()));
        }
        // If the field was cleared interactively, persist the empty string so
        // the stored value stays in sync (clearing is distinct from never-set).
        if filter.is_empty() && response.lost_focus() {
            ui.data_mut(|d| d.insert_temp(filter_id, String::new()));
        }
    });
    ui.add_space(SPACE_S);

    let filter_lower = filter.to_lowercase();
    let has_filter = !filter_lower.is_empty();

    // ── Pre-compute visibility ──────────────────────────────────────────────
    // When a filter is active we build a `show` mask.  The rules are:
    //   1. An entry is visible if its name contains the filter (case-insensitive).
    //   2. If a directory is visible (by name), all its descendants are visible.
    //   3. If any descendant is visible, the ancestor directory is also visible.
    // When no filter is active every entry passes.
    let show = if has_filter {
        let len = ctx.file_tree.len();
        let mut show = vec![false; len];

        // --- Pass 1: direct name match ---
        for (i, entry) in ctx.file_tree.iter().enumerate() {
            show[i] = entry.name.to_lowercase().contains(&filter_lower);
        }

        // --- Pass 2 (forward): expand matching directories to all children ---
        for i in 0..len {
            if show[i] && ctx.file_tree[i].is_dir {
                let parent_depth = ctx.file_tree[i].depth;
                for s in show[(i + 1)..].iter_mut().zip(ctx.file_tree[(i + 1)..].iter()) {
                    if s.1.depth <= parent_depth {
                        break;
                    }
                    *s.0 = true;
                }
            }
        }

        // --- Pass 3 (backward): show ancestors of any visible entry ---
        for i in (0..len).rev() {
            if ctx.file_tree[i].is_dir {
                let parent_depth = ctx.file_tree[i].depth;
                for s in show[(i + 1)..].iter().zip(ctx.file_tree[(i + 1)..].iter()) {
                    if s.1.depth <= parent_depth {
                        break;
                    }
                    if *s.0 {
                        show[i] = true;
                        break;
                    }
                }
            }
        }

        show
    } else {
        vec![true; ctx.file_tree.len()]
    };

    // ── Render visible tree entries ─────────────────────────────────────────
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
        for (i, entry) in ctx.file_tree.iter().enumerate() {
            if !show[i] {
                continue;
            }

            let is_selected = !entry.is_dir && entry.path == ctx.current_file;
            let is_expanded = entry.is_dir && ctx.expanded_dirs.contains(&entry.path);
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
                ctx.commands.push_back(Command::ToggleExpandDir(path.clone()));
            }
            if response.row_clicked {
                if is_dir {
                    ctx.commands.push_back(Command::ToggleExpandDir(path));
                } else {
                    ctx.commands.push_back(Command::OpenFile(path));
                }
            }
        }
    });
}

fn layers_content_ui(ctx: &mut SidebarViewer<'_>, ui: &mut egui::Ui) {
    // For compositions, use the active scene's timeline
    let timeline = ctx.timeline.or_else(|| {
        let comp = ctx.composition?;
        let scene_name = ctx.active_scene?;
        comp.scenes.get(scene_name).map(|s| &s.timeline)
    });
    let Some(timeline) = timeline else {
        components::empty_state(ui, egui_phosphor::regular::FILM_STRIP, "No timeline loaded", "");
        return;
    };

    // Show which scene's actors are being displayed
    if ctx.is_composition {
        if let Some(scene_name) = ctx.active_scene.as_ref() {
            ui.horizontal(|ui| {
                ui.add_space(SPACE_L);
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} {}", egui_phosphor::regular::FILM_STRIP, scene_name))
                            .size(FONT_SIZE_S)
                            .color(TEXT_MUTED),
                    )
                    .selectable(false),
                );
            });
            ui.add_space(SPACE_S);
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
            ui.add_space(SPACE_XL);
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
                    ctx.scene_dimensions.width as f32 / 2.0,
                    ctx.scene_dimensions.height as f32 / 2.0,
                ];
                ctx.commands.push_back(Command::CreateActor { ty: super::default_actor_type().into(), label, position: pos });
            }
        });
        return;
    }

    let time_ms = (ctx.preview.playback.current_time_s * 1000.0) as u64;
    egui::ScrollArea::vertical()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            for root_label in root_nodes {
                render_actor_tree(
                    ui,
                    timeline,
                    root_label,
                    ctx.selected_actors,
                    ctx.collapsed_actors,
                    ctx.commands,
                    time_ms,
                    0,
                );
            }
        });
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
    use crate::app::commands::{PropertyEdit, PropertyValue};

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

    if response.drag_started && !is_anonymous {
        ui.data_mut(|d| d.insert_temp(drag_data_id, label.to_string()));
    }

    let is_dragging = ui.data(|d| d.get_temp::<String>(drag_data_id)).is_some();
    let is_drop_target = is_dragging && response.hovered && !is_anonymous;
    if is_drop_target {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if dragged != label {
            ui.painter().rect_stroke(
                response.row_rect.expand(1.0),
                2,
                egui::Stroke::new(1.5, ACCENT_BLUE),
                egui::StrokeKind::Outside,
            );
        }
    }

    if is_dragging && ui.input(|i| i.pointer.any_released()) {
        let dragged = ui.data(|d| d.get_temp::<String>(drag_data_id)).unwrap_or_default();
        if !dragged.is_empty() && dragged != label {
            let pointer_pos = ui.input(|i| i.pointer.latest_pos());
            let over_this_row = pointer_pos.is_some_and(|p| response.row_rect.contains(p));
            if over_this_row && is_drop_target {
                commands.push_back(Command::ReparentActor { actor: dragged.clone(), new_parent: Some(label.to_string()) });
            } else if !over_this_row && depth == 0 {
                let over_any_root = pointer_pos.is_some_and(|p| {
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