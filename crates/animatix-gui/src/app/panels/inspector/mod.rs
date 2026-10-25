use std::collections::{HashMap, HashSet};

use animatix::timeline::{EntityId, Timeline, collect_all_keyframe_times};
use egui::{Color32, Pos2, RichText, ScrollArea, Vec2};
use eparts::widget::Select;
use eparts::{Button, NumberField, TextField};

use crate::app::PreviewPaneState;
use crate::app::commands::{
    ActionQueue, ActorCommand, DocumentCommand, PlaybackCommand, PropertyEdit,
    PropertyValue as GuiPropertyValue, SceneCommand,
};
use crate::app::components::easing_curve_editor::EasingCurveState;
use crate::app::components::{Badge, TabBar, easing_curve_editor, layout, text_tooltip, timeline};
use crate::app::design_tokens::spatial::inspector::INPUT_WIDTH_FLOAT as INSPECTOR_INPUT_WIDTH_FLOAT;
use crate::app::design_tokens::spatial::{RADIUS_M, RADIUS_S, STROKE_WIDTH, spatial};
use crate::app::design_tokens::typography::TextRole;
use crate::app::panels::panel_frame;
use crate::app::utils::text as app_text;

pub(crate) mod graph_editor;
pub(crate) mod keyframe_table;
pub(crate) mod property_groups;
mod property_groups_legend;
pub(crate) mod spreadsheet;

use self::keyframe_table::{count_keyframes, render_dope_sheet};
use self::property_groups::*;
use self::spreadsheet::render_property_spreadsheet;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PropertyViewMode {
    Semantic,
    Intensity,
    Spreadsheet,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum KeyframeViewMode {
    List,
    Curve,
}

pub(crate) struct InspectorContext<'a> {
    pub preview: &'a mut PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub selected_actors: &'a mut HashSet<String>,
    pub commands: &'a mut ActionQueue,
    pub keyframe_mode: bool,
    pub scene_dimensions: animatix::timeline::SceneDimensions,
    pub pivot_offsets: &'a mut HashMap<String, [f32; 2]>,
    pub property_view_mode: &'a mut PropertyViewMode,
    pub keyframe_view_mode: &'a mut KeyframeViewMode,
}

/// Renders the unified actor inspector panel (with frame).
pub(crate) fn inspector_panel_ui(ctx: &mut InspectorContext<'_>, ui: &mut egui::Ui) {
    panel_frame().show(ui, |ui| {
        // Scene-level inspector: when no actor is selected in a composition
        if ctx.selected_actors.is_empty() {
            if let (Some(comp), Some(active_scene)) = (ctx.composition, ctx.active_scene) {
                render_scene_inspector(ui, comp, active_scene, ctx.commands, ctx.preview);
                return;
            }
        }

        let current_time_s = ctx.preview.playback.current_time_s();
        let timeline = ctx.timeline;
        inspector_ui(
            ui,
            timeline,
            ctx.selected_actors,
            current_time_s,
            ctx.commands,
            ctx.keyframe_mode,
            ctx.scene_dimensions,
            ctx.pivot_offsets,
            ctx.property_view_mode,
            ctx.keyframe_view_mode,
            ctx.active_scene,
        );
    });
}

/// Render scene-level inspector when no actor is selected.
fn render_property_stream(
    ui: &mut egui::Ui,
    groups: &[PropertyGroup],
    _actor_label: &str,
    _commands: &mut ActionQueue,
    _keyframe_mode: bool,
    _current_time_s: f64,
    property_view_mode: &mut PropertyViewMode,
) {
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
    // Flatten all entries and sort by keyframe count descending
    let mut all_entries: Vec<(&PropertyGroup, &PropertyEntry)> = Vec::new();
    for group in groups {
        for entry in &group.properties {
            all_entries.push((group, entry));
        }
    }
    all_entries.sort_by(|a, b| b.1.keyframe_count.cmp(&a.1.keyframe_count));

    // Find max keyframe count for bar scaling
    let max_kf = all_entries.iter().map(|(_, e)| e.keyframe_count).max().unwrap_or(1).max(1);

    ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_1);
    for (group, entry) in &all_entries {
        let row_height = sp.inspector.row_height;
        let available = ui.available_width();
        let (row_rect, row_response) =
            ui.allocate_exact_size(Vec2::new(available, row_height), egui::Sense::hover());

        if row_response.hovered() {
            ui.painter().rect_filled(row_rect, 0.0, theme.palette.surface.hover);
        }

        let baseline_y = row_rect.center().y;

        // Intensity bar (left side)
        let bar_max_w = 60.0;
        let bar_w = if entry.keyframe_count > 0 {
            (entry.keyframe_count as f32 / max_kf as f32 * bar_max_w).max(4.0)
        } else {
            0.0
        };
        if bar_w > 0.0 {
            let bar_rect = egui::Rect::from_min_max(
                egui::pos2(row_rect.min.x + sp.base.space_2, baseline_y - 3.0),
                egui::pos2(row_rect.min.x + sp.base.space_2 + bar_w, baseline_y + 3.0),
            );
            let bar_color = if entry.keyframe_count >= max_kf / 2 {
                theme.palette.status.warning
            } else {
                theme.palette.text.muted
            };
            ui.painter().rect_filled(bar_rect, RADIUS_S, bar_color);
        }

        // Icon + property name
        let name_x = row_rect.min.x + sp.base.space_2 + bar_max_w + sp.base.space_2;
        ui.painter().text(
            egui::pos2(name_x, baseline_y),
            egui::Align2::LEFT_CENTER,
            format!("{} {}", group.icon, entry.name),
            TextRole::BodyS.font_id(),
            theme.palette.text.secondary,
        );

        // Current value (middle area)
        let value_text = format_property_value(&entry.kind);
        let value_x = name_x + 100.0;
        if !value_text.is_empty() {
            ui.painter().text(
                egui::pos2(value_x, baseline_y),
                egui::Align2::LEFT_CENTER,
                &value_text,
                egui::FontId::monospace(TextRole::Micro.size()),
                theme.palette.text.muted,
            );
        }

        // Keyframe count badge (right)
        if entry.keyframe_count > 0 {
            let count_rect = egui::Rect::from_center_size(
                egui::pos2(row_rect.max.x - sp.base.space_2, baseline_y),
                Vec2::new(40.0, row_height),
            );
            ui.put(
                count_rect,
                Badge::new(format!("{} {}", egui_phosphor::regular::DIAMOND, entry.keyframe_count)),
            );
        }

        // Click to jump to the property in semantic view
        if row_response.clicked() {
            *property_view_mode = PropertyViewMode::Semantic;
        }
    }
    ui.spacing_mut().item_spacing = Vec2::new(0.0, sp.base.space_2);

    // Divider between animated and non-animated
    let animated_count = all_entries.iter().filter(|(_, e)| e.keyframe_count > 0).count();
    if animated_count > 0 && animated_count < all_entries.len() {
        ui.add_space(sp.base.space_2);
        let divider_rect = ui.available_rect_before_wrap();
        if divider_rect.width() > 0.0 {
            ui.painter().line_segment(
                [
                    egui::pos2(divider_rect.min.x + sp.base.space_2, divider_rect.min.y + 4.0),
                    egui::pos2(divider_rect.max.x - sp.base.space_2, divider_rect.min.y + 4.0),
                ],
                egui::Stroke::new(STROKE_WIDTH, theme.palette.border.default),
            );
        }
        ui.add_space(sp.base.space_2);
    }
}

// ─── Actor Header ─────────────────────────────────────────────────────────

fn render_actor_header(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    id: EntityId,
    current_time_s: f64,
    commands: &mut ActionQueue,
) {
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
    let current_time_ms = (current_time_s * 1000.0) as u64;
    let actor_label = timeline.world().label(id).unwrap_or("");
    let available = ui.available_width();
    let row_h = sp.base.row_l;
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(available, row_h), egui::Sense::hover());

    // ── Left side: icon + name ──
    let left_rect =
        egui::Rect::from_min_max(row_rect.min, egui::pos2(row_rect.center().x, row_rect.max.y));
    ui.scope_builder(egui::UiBuilder::new().max_rect(left_rect), |ui| {
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
            |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(crate::app::icons::actor_icon_for_track(id, timeline))
                            .size(TextRole::Heading.size())
                            .color(theme.palette.status.warning),
                    )
                    .selectable(false),
                );
                ui.add_space(sp.base.space_2);

                // Actor label (click to rename)
                let edit_id = ui.id().with("actor_name_edit");
                let is_editing: bool = ui.data(|d| d.get_temp(edit_id)).unwrap_or(false);
                let mut edit_buffer: String = ui
                    .data(|d| d.get_temp(edit_id.with("buf")))
                    .unwrap_or_else(|| actor_label.to_string());

                if is_editing {
                    let response = TextField::new(&mut edit_buffer).desired_width(120.0).show(ui);
                    response.request_focus();
                    if response.lost_focus() {
                        ui.data_mut(|d| d.insert_temp(edit_id, false));
                        if edit_buffer != actor_label && !edit_buffer.is_empty() {
                            commands.push_back(
                                ActorCommand::RenameActor {
                                    old_label: actor_label.to_string(),
                                    new_label: edit_buffer.clone(),
                                }
                                .into(),
                            );
                        }
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        ui.data_mut(|d| d.insert_temp(edit_id, false));
                        edit_buffer = actor_label.to_string();
                    }
                    ui.data_mut(|d| d.insert_temp(edit_id.with("buf"), edit_buffer));
                } else {
                    let label_response = ui.add(
                        egui::Label::new(
                            RichText::new(actor_label)
                                .size(TextRole::Heading.size())
                                .color(theme.palette.text.primary),
                        )
                        .selectable(false)
                        .sense(egui::Sense::click()),
                    );
                    if label_response.double_clicked() {
                        ui.data_mut(|d| {
                            d.insert_temp(edit_id, true);
                            d.insert_temp(edit_id.with("buf"), actor_label.to_string());
                        });
                    }
                }
            },
        );
    });

    // ── Right side: shape type + first seen time ──
    let right_rect =
        egui::Rect::from_min_max(egui::pos2(row_rect.center().x, row_rect.min.y), row_rect.max);
    ui.scope_builder(egui::UiBuilder::new().max_rect(right_rect), |ui| {
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(false),
            |ui| {
                let first_seen_ms = timeline.world().meta(id).first_seen_ms;
                if first_seen_ms > 0 && first_seen_ms != u64::MAX {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("t = {:.2}s", first_seen_ms as f64 / 1000.0))
                                .size(TextRole::Micro.size())
                                .color(theme.palette.text.muted),
                        )
                        .selectable(false),
                    );
                    ui.add_space(sp.base.space_3);
                }

                if let Some(shape_pt) = &timeline.world().shape(id).shape_type {
                    let shape = shape_pt.evaluate(current_time_ms);
                    ui.add(
                        egui::Label::new(
                            RichText::new(shape.to_string())
                                .size(TextRole::BodyS.size())
                                .color(theme.palette.text.muted),
                        )
                        .selectable(false),
                    );
                }
            },
        );
    });
}

// ─── Parent Card ──────────────────────────────────────────────────────────

fn render_parent_card(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    actor: &str,
    commands: &mut ActionQueue,
) {
    use crate::app::components::layout;

    // Find current parent from the authoritative hierarchy component.
    let current_parent = timeline
        .get_entity_id(actor)
        .and_then(|id| timeline.world().hierarchy(id).parent)
        .and_then(|parent_id| timeline.get_entity_label(parent_id))
        .map(str::to_string);

    layout::group_box(ui, format!("{} Hierarchy", egui_phosphor::regular::TREE_STRUCTURE), |ui| {
        layout::labeled_row(ui, "Parent", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
            let all_labels: Vec<String> =
                timeline.tracks().keys().filter(|&label| label != actor).cloned().collect();

            let mut options = vec!["None (root)".to_string()];
            options.extend(all_labels.clone());
            let mut sel_idx = match current_parent.as_ref() {
                Some(p) => all_labels.iter().position(|l| l == p).map(|i| i + 1),
                None => Some(0),
            };

            ui.add(
                Select::new(ui.id().with("parent_dropdown"), &mut sel_idx, &options)
                    .placeholder("Select parent…"),
            );
            if let Some(idx) = sel_idx {
                let chosen = if idx == 0 {
                    None
                } else {
                    Some(all_labels[idx - 1].clone())
                };
                if chosen != current_parent {
                    commands.push_back(
                        ActorCommand::ReparentActor {
                            actor: actor.to_string(),
                            new_parent: chosen,
                        }
                        .into(),
                    );
                }
            }
        });
    });
}

// ─── Container Children Reorder ───────────────────────────────────────────

fn render_container_children(
    ui: &mut egui::Ui,
    container: &str,
    order: &[String],
    commands: &mut ActionQueue,
    keyframe_mode: bool,
) {
    let sp = spatial(ui);
    let theme = eparts::theme(ui);
    ui.spacing_mut().item_spacing = Vec2::new(sp.base.space_2, sp.base.space_2);
    for (i, label) in order.iter().enumerate() {
        let row_id = ui.id().with(format!("child_{}", i));
        let available = ui.available_width();
        let (row_rect, _) =
            ui.allocate_exact_size(Vec2::new(available, sp.base.row_m), egui::Sense::hover());

        // Background
        let bg = if ui.rect_contains_pointer(row_rect) {
            theme.palette.surface.hover
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(row_rect, RADIUS_M, bg);
        }

        let baseline_y = row_rect.center().y;
        let mut cursor_x = row_rect.min.x + sp.base.space_3;

        // Index badge
        let badge_rect = egui::Rect::from_min_size(
            egui::pos2(cursor_x, baseline_y - 9.0),
            Vec2::new(sp.base.row_s, sp.base.row_s),
        );
        ui.put(badge_rect, Badge::new((i + 1).to_string()));
        cursor_x += sp.base.row_s + sp.base.space_2;

        // Label
        ui.painter().text(
            egui::pos2(cursor_x, baseline_y),
            egui::Align2::LEFT_CENTER,
            label,
            TextRole::BodyS.font_id(),
            theme.palette.text.secondary,
        );

        // Up / Down buttons (right-aligned)
        let btn_size = Vec2::new(sp.base.row_s, sp.base.row_s);
        let btn_y = row_rect.min.y + (row_rect.height() - btn_size.y) * 0.5;
        let mut btn_x = row_rect.max.x - sp.base.space_2 - btn_size.x;

        // Down button
        let down_rect = egui::Rect::from_min_size(egui::pos2(btn_x, btn_y), btn_size);
        let down_resp = ui.interact(down_rect, row_id.with("down"), egui::Sense::click());
        text_tooltip(ui, down_resp.id.with("tooltip"), &down_resp, "Move down");
        let down_color = if i + 1 >= order.len() {
            theme.palette.text.disabled
        } else if down_resp.hovered() {
            theme.palette.text.primary
        } else {
            theme.palette.text.secondary
        };
        ui.painter().text(
            down_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::CARET_DOWN,
            TextRole::BodyS.font_id(),
            down_color,
        );
        btn_x -= btn_size.x + sp.base.space_1;

        // Up button
        let up_rect = egui::Rect::from_min_size(egui::pos2(btn_x, btn_y), btn_size);
        let up_resp = ui.interact(up_rect, row_id.with("up"), egui::Sense::click());
        text_tooltip(ui, up_resp.id.with("tooltip"), &up_resp, "Move up");
        let up_color = if i == 0 {
            theme.palette.text.disabled
        } else if up_resp.hovered() {
            theme.palette.text.primary
        } else {
            theme.palette.text.secondary
        };
        ui.painter().text(
            up_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::CARET_UP,
            TextRole::BodyS.font_id(),
            up_color,
        );

        // Emit reorder on click
        if up_resp.clicked() && i > 0 {
            let mut new_order = order.to_vec();
            new_order.swap(i, i - 1);
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: container.to_string(),
                    property: "child_order".into(),
                    value: GuiPropertyValue::StringList(new_order),
                    create_keyframe: keyframe_mode,
                })
                .into(),
            );
        }
        if down_resp.clicked() && i + 1 < order.len() {
            let mut new_order = order.to_vec();
            new_order.swap(i, i + 1);
            commands.push_back(
                DocumentCommand::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: container.to_string(),
                    property: "child_order".into(),
                    value: GuiPropertyValue::StringList(new_order),
                    create_keyframe: keyframe_mode,
                })
                .into(),
            );
        }
    }
}

/// Format a property value for display in the intensity stream view.
fn format_property_value(kind: &PropertyKind) -> String {
    match kind {
        PropertyKind::Vec2 { x, y } => format!("({:.1}, {:.1})", x, y),
        PropertyKind::Float(v) => format!("{:.2}", v),
        PropertyKind::U32(v) => format!("{}", v),
        PropertyKind::Bool(v) => v.to_string(),
        PropertyKind::Color(rgba) => {
            let r = (rgba[0] * 255.0).round() as u8;
            let g = (rgba[1] * 255.0).round() as u8;
            let b = (rgba[2] * 255.0).round() as u8;
            if rgba[3] >= 0.999 {
                format!("#{:02x}{:02x}{:02x}", r, g, b)
            } else {
                format!("#{:02x}{:02x}{:02x}{:02x}", r, g, b, (rgba[3] * 255.0) as u8)
            }
        },
        PropertyKind::Text(s) => {
            if s.chars().count() > 16 {
                format!("{}…", app_text::truncate_chars(s, 15))
            } else {
                s.clone()
            }
        },
        PropertyKind::Enum { value, .. } => value.clone(),
        PropertyKind::EnumOwned { value, .. } => value.clone(),
        PropertyKind::Sum { value, .. } => match value {
            animatix::timeline::PropertyValue::Variant { name, value } => {
                let payload = match value.as_ref() {
                    animatix::timeline::PropertyValue::Bool(v) => v.to_string(),
                    animatix::timeline::PropertyValue::String(v) => v.clone(),
                    animatix::timeline::PropertyValue::F32(v) => format!("{v:.2}"),
                    other => format!("{other:?}"),
                };
                format!("{name}: {payload}")
            },
            other => format!("{other:?}"),
        },
        PropertyKind::Union { value, .. } => match value {
            animatix::timeline::PropertyValue::Bool(v) => v.to_string(),
            animatix::timeline::PropertyValue::String(v) => v.clone(),
            animatix::timeline::PropertyValue::F32(v) => format!("{v:.2}"),
            animatix::timeline::PropertyValue::U32(v) => v.to_string(),
            animatix::timeline::PropertyValue::Vec2(v) => format!("({:.1}, {:.1})", v[0], v[1]),
            animatix::timeline::PropertyValue::Color(v)
            | animatix::timeline::PropertyValue::Vec4(v) => {
                let r = (v[0] * 255.0).round() as u8;
                let g = (v[1] * 255.0).round() as u8;
                let b = (v[2] * 255.0).round() as u8;
                if v[3] >= 0.999 {
                    format!("#{r:02x}{g:02x}{b:02x}")
                } else {
                    format!("#{r:02x}{g:02x}{b:02x}{:02x}", (v[3] * 255.0) as u8)
                }
            },
            other => format!("{other:?}"),
        },
    }
}

mod scene;

use scene::{inspector_ui, render_scene_inspector};
