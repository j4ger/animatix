use std::collections::HashSet;
use animatix::timeline::{AnimationTrack, Timeline};
use egui::{Color32, RichText, ScrollArea, Vec2};

use crate::app::components;
use crate::app::icons::actor_icon_str;
use crate::app::theme::*;
use crate::app::commands::{Command, CommandQueue, PropertyEdit, PropertyValue as GuiPropertyValue};

pub(crate) mod property_groups;
mod keyframe_table;

use self::property_groups::*;
use self::keyframe_table::{render_dope_sheet, collect_all_keyframe_times, count_keyframes};

fn default_actor_type() -> &'static str {
    animatix::primitives::actor_kind_registry()
        .iter()
        .find(|meta| {
            meta.category == animatix::timeline::ActorCategory::Shape && !meta.advanced
        })
        .map(|meta| meta.type_name)
        .unwrap_or("Rect")
}

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
    selected_actors: &mut HashSet<String>,
    current_time_s: f64,
    commands: &mut CommandQueue,
    keyframe_mode: bool,
    scene_dimensions: animatix::timeline::SceneDimensions,
    pivot_offsets: &mut std::collections::HashMap<String, [f32; 2]>,
) {
    let should_reset = selected_actors
        .iter()
        .next()
        .is_some_and(|sel| timeline.is_some_and(|t| !t.has_actor(sel)));
    if should_reset {
        selected_actors.clear();
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
            ui.add_space(SPACE_L);
            if ui
                .button(
                    RichText::new(format!("{} Add Actor", egui_phosphor::regular::PLUS))
                        .size(FONT_SIZE_L)
                        .color(ACCENT_BLUE),
                )
                .clicked()
            {
                let label = format!("rect1");
                let pos = [
                    scene_dimensions.width as f32 / 2.0,
                    scene_dimensions.height as f32 / 2.0,
                ];
                commands.push_back(Command::CreateActor {
                    ty: default_actor_type().into(),
                    label,
                    position: pos,
                });
            }
        });
        return;
    }

    if let Some(sel) = selected_actors.iter().next() {
        let Some(track) = timeline.get_track(sel) else {
            components::empty_state(
                ui,
                egui_phosphor::regular::WARNING,
                "Actor not found",
                "The selected actor no longer exists in the timeline",
            );
            return;
        };

        let multi_count = selected_actors.len();

        ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                if multi_count > 1 {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{} actors selected", multi_count))
                                .size(FONT_SIZE_M)
                                .color(TEXT_SECONDARY),
                        );
                    });
                    ui.add_space(SPACE_S);
                }
                render_actor_header(ui, track, current_time_s, commands);
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
                                commands,
                                keyframe_mode,
                            );
                        }
                    }
                });

                ui.add_space(SPACE_M);

                // ── Pivot ──
                if multi_count == 1 {
                    components::card(ui, |ui| {
                        components::section_header(
                            ui,
                            egui_phosphor::regular::CROSSHAIR,
                            "Pivot",
                            None,
                        );
                        let pivot = pivot_offsets.entry(sel.clone()).or_insert([0.0, 0.0]);
                        components::labeled_row(ui, "X", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                            ui.add(egui::DragValue::new(&mut pivot[0]).speed(1.0).suffix(" px"));
                        });
                        components::labeled_row(ui, "Y", INSPECTOR_INPUT_WIDTH_FLOAT, |ui| {
                            ui.add(egui::DragValue::new(&mut pivot[1]).speed(1.0).suffix(" px"));
                        });
                        if ui.button(RichText::new("Reset").size(FONT_SIZE_S).color(TEXT_MUTED)).clicked() {
                            *pivot = [0.0, 0.0];
                        }
                    });
                    ui.add_space(SPACE_M);
                }

                // ── Container Children ──
                if timeline.container_metadata.get(sel).is_some() {
                    components::card(ui, |ui| {
                        components::section_header(
                            ui,
                            egui_phosphor::regular::ROWS,
                            "Children",
                            Some(timeline.layout_children_for(sel).len()),
                        );
                        let time_ms = (current_time_s * 1000.0) as u64;
                        let order = timeline.get_child_order(sel, time_ms);
                        render_container_children(ui, sel, &order, commands, keyframe_mode);
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
                        commands.push_back(Command::ScrubTo(scrub_t));
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
                        sel,
                        commands,
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
    commands: &mut CommandQueue,
) {
    let current_time_ms = (current_time_s * 1000.0) as u64;
    let available = ui.available_width();
    let row_h = ROW_L;
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(available, row_h), egui::Sense::hover());

    // ── Left side: icon + name ──
    let left_rect = egui::Rect::from_min_max(
        row_rect.min,
        egui::pos2(row_rect.center().x, row_rect.max.y),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(left_rect), |ui| {
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(false),
            |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(actor_icon_str(track.kind))
                            .size(FONT_SIZE_XL)
                            .color(AMBER),
                    )
                    .selectable(false),
                );
                ui.add_space(SPACE_S);

                // Actor label (click to rename)
                let edit_id = ui.id().with("actor_name_edit");
                let is_editing: bool = ui.data(|d| d.get_temp(edit_id)).unwrap_or(false);
                let mut edit_buffer: String =
                    ui.data(|d| d.get_temp(edit_id.with("buf")))
                        .unwrap_or_else(|| track.label.clone());

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
                            commands.push_back(Command::RenameActor {
                                old_label: track.label.clone(),
                                new_label: edit_buffer.clone(),
                            });
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
            },
        );
    });

    // ── Right side: shape type + first seen time ──
    let right_rect = egui::Rect::from_min_max(
        egui::pos2(row_rect.center().x, row_rect.min.y),
        row_rect.max,
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(right_rect), |ui| {
        ui.with_layout(
            egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(false),
            |ui| {
                if track.first_seen_ms > 0 && track.first_seen_ms != u64::MAX {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "t = {:.2}s",
                                track.first_seen_ms as f64 / 1000.0
                            ))
                            .size(FONT_SIZE_XS)
                            .color(TEXT_MUTED),
                        )
                        .selectable(false),
                    );
                    ui.add_space(SPACE_M);
                }

                if let Some(shape_pt) = &track.shape_type {
                    let shape = shape_pt.evaluate(current_time_ms);
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("{:?}", shape))
                                .size(FONT_SIZE_S)
                                .color(TEXT_MUTED),
                        )
                        .selectable(false),
                    );
                }
            },
        );
    });
}



// ─── Container Children Reorder ───────────────────────────────────────────

fn render_container_children(
    ui: &mut egui::Ui,
    container: &str,
    order: &[String],
    commands: &mut CommandQueue,
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
            commands.push_back(Command::PropertyEdit(PropertyEdit {
                actor: container.to_string(),
                property: "child_order".into(),
                value: GuiPropertyValue::StringList(new_order),
                create_keyframe: keyframe_mode,
            }));
        }
        if down_resp.clicked() && i + 1 < order.len() {
            let mut new_order = order.to_vec();
            new_order.swap(i, i + 1);
            commands.push_back(Command::PropertyEdit(PropertyEdit {
                actor: container.to_string(),
                property: "child_order".into(),
                value: GuiPropertyValue::StringList(new_order),
                create_keyframe: keyframe_mode,
            }));
        }
    }
}
