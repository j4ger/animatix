use animatix::timeline::{AnimationTrack, ShapeType, Timeline};
use egui::{Color32, Id, RichText, ScrollArea, Vec2};

use crate::app::components;
use crate::app::icons::actor_icon_str;
use crate::app::theme::*;
use crate::app::panels::{PropertyEdit, PropertyValue as GuiPropertyValue, UiActions};

mod property_groups;
mod keyframe_table;

use self::property_groups::*;
use self::keyframe_table::{render_dope_sheet, collect_all_keyframe_times, count_keyframes};

// ─── Main Entry Point ─────────────────────────────────────────────────────

/// Renders the unified actor inspector panel.
///
/// Layout (single scrollable panel, no tabs):
///   1. Actor Header
///   2. Active Properties (editable, native inputs)
///   3. Mini Timeline
///   4. Keyframes
pub(super) fn inspector_ui(
    ui: &mut egui::Ui,
    timeline: Option<&Timeline>,
    selected_actor: &mut Option<String>,
    current_time_s: f64,
    actions: &mut UiActions,
    keyframe_mode: bool,
    scene_dimensions: animatix::timeline::SceneDimensions,
) {
    let should_reset = selected_actor
        .as_ref()
        .is_some_and(|sel| timeline.is_some_and(|t| !t.has_actor(sel)));
    if should_reset {
        *selected_actor = None;
    }

    let Some(timeline) = timeline else {
        components::empty_state(
            ui,
            egui_phosphor::regular::FILM_STRIP,
            "No timeline loaded",
            "Open or create a scene to begin",
        );
        return;
    };

    let root_nodes = timeline.root_actor_labels();
    if root_nodes.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(36.0);
            ui.add(
                egui::Label::new(
                    RichText::new(egui_phosphor::regular::FILM_STRIP)
                        .size(28.0)
                        .color(Color32::from_rgb(90, 96, 110)),
                )
                .selectable(false),
            );
            ui.add_space(10.0);
            ui.add(
                egui::Label::new(
                    RichText::new("No actors in scene")
                        .size(12.0)
                        .color(Color32::from_rgb(150, 158, 175)),
                )
                .selectable(false),
            );
            ui.add_space(12.0);
            if ui
                .button(
                    RichText::new(format!("{} Add Actor", egui_phosphor::regular::PLUS))
                        .size(12.0)
                        .color(ACCENT_BLUE),
                )
                .clicked()
            {
                let label = format!("rect1");
                let pos = [
                    scene_dimensions.width as f32 / 2.0,
                    scene_dimensions.height as f32 / 2.0,
                ];
                actions.create_actor = Some(("Rect".into(), label, pos));
            }
        });
        return;
    }

    if let Some(sel) = selected_actor.as_ref() {
        let Some(track) = timeline.get_track(sel) else {
            components::empty_state(
                ui,
                egui_phosphor::regular::WARNING,
                "Actor not found",
                "The selected actor no longer exists in the timeline",
            );
            return;
        };

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                render_actor_header(ui, track, current_time_s, actions);
                ui.add_space(SPACE_M);

                // ── Active Properties ──
                components::card(ui, |ui| {
                    components::section_header(
                        ui,
                        egui_phosphor::regular::WRENCH,
                        "Properties",
                        None,
                    );
                    let current_time_ms = (current_time_s * 1000.0) as u64;
                    let groups = build_property_groups(track, current_time_ms);
                    if groups.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(SPACE_M);
                            ui.add(
                                egui::Label::new(
                                    RichText::new("No editable properties")
                                        .size(FONT_SIZE_M)
                                        .color(TEXT_MUTED),
                                )
                                .selectable(false),
                            );
                        });
                    } else {
                        for group in &groups {
                            render_property_group(
                                ui,
                                group,
                                &track.label,
                                actions,
                                keyframe_mode,
                            );
                        }
                    }
                });

                ui.add_space(SPACE_M);

                // ── Container Children ──
                if let Some(metadata) = timeline.container_metadata.get(sel) {
                    components::card(ui, |ui| {
                        components::section_header(
                            ui,
                            egui_phosphor::regular::ROWS,
                            "Children",
                            Some(metadata.layout_children.len()),
                        );
                        let time_ms = (current_time_s * 1000.0) as u64;
                        let order = timeline.get_child_order(sel, time_ms);
                        render_container_children(ui, sel, &order, actions, keyframe_mode);
                    });
                    ui.add_space(SPACE_M);
                }

                // ── Mini Timeline ──
                components::card(ui, |ui| {
                    components::section_header(
                        ui,
                        egui_phosphor::regular::CLOCK,
                        "Timeline",
                        None,
                    );
                    let duration_s = timeline.duration_seconds().max(0.1);
                    let all_kf = collect_all_keyframe_times(track);
                    let strip = components::TimelineStrip {
                        duration_s,
                        current_time_s,
                        keyframes: &all_kf,
                        height: ROW_XS,
                    };
                    if let Some(scrub_t) = strip.show(ui, ui.id().with("mini_timeline")) {
                        actions.scrub_to = Some(scrub_t);
                    }
                });

                ui.add_space(SPACE_M);

                // ── Keyframes ──
                let kf_count = count_keyframes(track);
                components::card(ui, |ui| {
                    components::section_header(
                        ui,
                        egui_phosphor::regular::KEY,
                        "Keyframes",
                        Some(kf_count),
                    );
                    render_dope_sheet(
                        ui,
                        timeline,
                        track,
                        (current_time_s * 1000.0) as u64,
                        actions,
                    );
                });
            });
    } else {
        components::empty_state(
            ui,
            egui_phosphor::regular::MAGNIFYING_GLASS,
            "Select an actor to inspect",
            "Click an actor in the preview or Layers panel",
        );
    }
}

// ─── Actor Header ─────────────────────────────────────────────────────────

fn render_actor_header(
    ui: &mut egui::Ui,
    track: &AnimationTrack,
    current_time_s: f64,
    actions: &mut UiActions,
) {
    let current_time_ms = (current_time_s * 1000.0) as u64;
    let available = ui.available_width();
    let row_h = ROW_L;
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(available, row_h), egui::Sense::hover());
    let baseline_y = row_rect.center().y;
    let mut cursor_x = row_rect.min.x;

    // Actor icon
    ui.painter().text(
        egui::pos2(cursor_x + 10.0, baseline_y),
        egui::Align2::CENTER_CENTER,
        actor_icon_str(track.kind),
        egui::FontId::new(FONT_SIZE_XL, egui::FontFamily::Proportional),
        AMBER,
    );
    cursor_x += 22.0;

    // Actor label (click to rename)
    let edit_id = ui.id().with("actor_name_edit");
    let is_editing: bool = ui.data(|d| d.get_temp(edit_id)).unwrap_or(false);
    let mut edit_buffer: String = ui.data(|d| d.get_temp(edit_id.with("buf"))).unwrap_or_else(|| track.label.clone());

    if is_editing {
        let response = ui.add(
            egui::TextEdit::singleline(&mut edit_buffer)
                .font(egui::FontId::new(FONT_SIZE_XL, egui::FontFamily::Proportional))
                .text_color(TEXT_PRIMARY)
                .desired_width(120.0),
        );
        if response.lost_focus() {
            ui.data_mut(|d| d.insert_temp(edit_id, false));
            if edit_buffer != track.label && !edit_buffer.is_empty() {
                actions.rename_actor = Some((track.label.clone(), edit_buffer.clone()));
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ui.data_mut(|d| d.insert_temp(edit_id, false));
            edit_buffer = track.label.clone();
        }
        ui.data_mut(|d| d.insert_temp(edit_id.with("buf"), edit_buffer));
    } else {
        let label_response = ui.add(
            egui::Label::new(
                RichText::new(&track.label)
                    .size(FONT_SIZE_XL)
                    .color(TEXT_PRIMARY),
            )
            .selectable(false)
            .sense(egui::Sense::click()),
        );
        if label_response.clicked() {
            ui.data_mut(|d| {
                d.insert_temp(edit_id, true);
                d.insert_temp(edit_id.with("buf"), track.label.clone());
            });
        }
    }

    // Right-aligned metadata (shape type + first seen time)
    let mut right_x = row_rect.max.x - SPACE_S;
    if track.first_seen_ms > 0 && track.first_seen_ms != u64::MAX {
        let time_text = format!("t = {:.2}s", track.first_seen_ms as f64 / 1000.0);
        ui.painter().text(
            egui::pos2(right_x, baseline_y),
            egui::Align2::RIGHT_CENTER,
            time_text,
            egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );
        // Approximate width for positioning
        right_x -= 60.0;
    }

    if let Some(shape_pt) = &track.shape_type {
        let shape = shape_pt.evaluate(current_time_ms);
        let shape_text = format!("{:?}", shape);
        ui.painter().text(
            egui::pos2(right_x, baseline_y),
            egui::Align2::RIGHT_CENTER,
            shape_text,
            egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );
    }
}



// ─── Container Children Reorder ───────────────────────────────────────────

fn render_container_children(
    ui: &mut egui::Ui,
    container: &str,
    order: &[String],
    actions: &mut UiActions,
    keyframe_mode: bool,
) {
    ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, SPACE_S);
    for (i, label) in order.iter().enumerate() {
        let row_id = ui.id().with(format!("child_{}", i));
        let available = ui.available_width();
        let (row_rect, _) = ui.allocate_exact_size(Vec2::new(available, ROW_M), egui::Sense::hover());

        // Background
        let bg = if ui.rect_contains_pointer(row_rect) {
            BG_HOVER
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(row_rect, RADIUS_M, bg);
        }

        let baseline_y = row_rect.center().y;
        let mut cursor_x = row_rect.min.x + SPACE_M;

        // Index badge
        let badge = format!("{}", i + 1);
        ui.painter().text(
            egui::pos2(cursor_x + 8.0, baseline_y),
            egui::Align2::CENTER_CENTER,
            badge,
            egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );
        cursor_x += 20.0;

        // Label
        ui.painter().text(
            egui::pos2(cursor_x, baseline_y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            TEXT_SECONDARY,
        );

        // Up / Down buttons (right-aligned)
        let btn_size = Vec2::new(ROW_S, ROW_S);
        let btn_y = row_rect.min.y + (row_rect.height() - btn_size.y) * 0.5;
        let mut btn_x = row_rect.max.x - SPACE_S - btn_size.x;

        // Down button
        let down_rect = egui::Rect::from_min_size(egui::pos2(btn_x, btn_y), btn_size);
        let down_resp = ui.interact(down_rect, row_id.with("down"), egui::Sense::click());
        let down_color = if i + 1 >= order.len() {
            TEXT_DISABLED
        } else if down_resp.hovered() {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        };
        ui.painter().text(
            down_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::CARET_DOWN,
            egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            down_color,
        );
        btn_x -= btn_size.x + SPACE_XS;

        // Up button
        let up_rect = egui::Rect::from_min_size(egui::pos2(btn_x, btn_y), btn_size);
        let up_resp = ui.interact(up_rect, row_id.with("up"), egui::Sense::click());
        let up_color = if i == 0 {
            TEXT_DISABLED
        } else if up_resp.hovered() {
            TEXT_PRIMARY
        } else {
            TEXT_SECONDARY
        };
        ui.painter().text(
            up_rect.center(),
            egui::Align2::CENTER_CENTER,
            egui_phosphor::regular::CARET_UP,
            egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
            up_color,
        );

        // Emit reorder on click
        if up_resp.clicked() && i > 0 {
            let mut new_order = order.to_vec();
            new_order.swap(i, i - 1);
            actions.property_edits.push(PropertyEdit {
                actor: container.to_string(),
                property: "child_order".into(),
                value: GuiPropertyValue::StringList(new_order),
                create_keyframe: keyframe_mode,
            });
        }
        if down_resp.clicked() && i + 1 < order.len() {
            let mut new_order = order.to_vec();
            new_order.swap(i, i + 1);
            actions.property_edits.push(PropertyEdit {
                actor: container.to_string(),
                property: "child_order".into(),
                value: GuiPropertyValue::StringList(new_order),
                create_keyframe: keyframe_mode,
            });
        }
    }
}
