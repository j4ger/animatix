use crate::app::commands::{ActionQueue, Command, ShellAction, PropertyEdit, PropertyValue};
use crate::app::components::button;
use crate::app::preview::ActorProps;
use crate::app::design_tokens::*;
use animatix::timeline::{Timeline, read_property_value_or_default, PropertyValue as TlPropertyValue};
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};

/// Show the property popup attached to a selected actor.
///
/// Attached to actor's top edge, follows actor, clamped to viewport.
/// Auto-hides during canvas drag.
pub fn show_property_popup(
    ui: &mut egui::Ui,
    actor: &str,
    props: &ActorProps,
    screen_pos: Pos2,
    commands: &mut ActionQueue,
    is_dragging: bool,
    timeline: Option<&Timeline>,
    current_time_s: f64,
) {
    if is_dragging {
        return; // Auto-hide during canvas drag
    }

    let viewport = ui.max_rect();
    let popup_w = 260.0;
    let popup_h = 220.0; // Increased to fit all rows

    // Position: attached to top edge of actor, centered
    let popup_pos = Pos2::new(
        screen_pos.x - popup_w / 2.0,
        screen_pos.y - popup_h - 12.0,
    );

    // Clamp to viewport
    let clamped_pos = Pos2::new(
        popup_pos.x.clamp(viewport.min.x + 4.0, viewport.max.x - popup_w - 4.0),
        popup_pos.y.clamp(viewport.min.y + 4.0, viewport.max.y - popup_h - 4.0),
    );
    let popup_rect = Rect::from_min_size(clamped_pos, Vec2::new(popup_w, popup_h));

    // Background
    ui.painter().rect_filled(popup_rect, RADIUS_L as u8, BG_SURFACE);
    ui.painter().rect_stroke(
        popup_rect,
        RADIUS_L as u8,
        Stroke::new(STROKE_WIDTH, BORDER),
        egui::StrokeKind::Outside,
    );

    // Build child UI for content
    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(popup_rect.shrink(SPACE_M)));
    content.set_clip_rect(popup_rect);

    // ── Header: actor name + close ──
    content.horizontal(|ui| {
        ui.label(RichText::new(actor).size(FONT_SIZE_M).color(TEXT_PRIMARY).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if button::icon_button(ui, egui_phosphor::regular::X, "Close").clicked() {
                // Close is implicit — deselect actor (handled by Esc or clicking elsewhere)
            }
        });
    });
    content.add_space(SPACE_S);

    // ── Essentials row: 2x2 grid ──
    let essentials_h = 48.0;
    let essentials_rect = Rect::from_min_size(
        content.cursor().min,
        Vec2::new(content.available_width(), essentials_h),
    );

    ui.painter().rect_filled(essentials_rect, RADIUS_M as u8, BG_BASE);

    let mut ess_ui = ui.new_child(egui::UiBuilder::new().max_rect(essentials_rect.shrink(SPACE_S)));
    ess_ui.set_clip_rect(essentials_rect);

    let col_w = (essentials_rect.width() - SPACE_S * 2.0 - SPACE_S) / 2.0;

    // Read opacity from timeline
    let time_ms = (current_time_s * 1000.0) as u64;
    let opacity = timeline
        .and_then(|tl| {
            tl.get_track(actor).map(|track| {
                let val = read_property_value_or_default(track, animatix::timeline::ActorField::Opacity, time_ms, track.kind);
                match val {
                    TlPropertyValue::F32(v) => v,
                    _ => 1.0,
                }
            })
        })
        .unwrap_or(1.0);

    // Row 1: Position | Size
    ess_ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

        let pos_delta = property_essential(ui, "Pos", &format!("{:.0}, {:.0}", props.position[0], props.position[1]), col_w);
        if let Some((dx, dy)) = pos_delta {
            commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                time_s: None,
                actor: actor.to_string(),
                property: "position".into(),
                value: PropertyValue::Vec2([props.position[0] + dx, props.position[1] + dy]),
                create_keyframe: false,
            })));
        }

        let size_delta = property_essential(ui, "Size", &format!("{:.0}×{:.0}", props.size[0], props.size[1]), col_w);
        if let Some((dx, dy)) = size_delta {
            let new_w = (props.size[0] + dx).max(1.0);
            let new_h = (props.size[1] + dy).max(1.0);
            commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                time_s: None,
                actor: actor.to_string(),
                property: "size".into(),
                value: PropertyValue::Vec2([new_w, new_h]),
                create_keyframe: false,
            })));
        }
    });

    // Row 2: Rotation | Opacity
    ess_ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

        let rot_deg = props.rotation.to_degrees();
        let rot_delta = property_essential(ui, "Rot", &format!("{:.0}°", rot_deg), col_w);
        if let Some((dx, _)) = rot_delta {
            let new_rot_deg = (rot_deg + dx * 0.5).rem_euclid(360.0);
            commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                time_s: None,
                actor: actor.to_string(),
                property: "rotation".into(),
                value: PropertyValue::Float(new_rot_deg.to_radians()),
                create_keyframe: false,
            })));
        }

        let opac_delta = property_essential(ui, "Opac", &format!("{:.0}%", opacity * 100.0), col_w);
        if let Some((dx, _)) = opac_delta {
            let new_opacity = (opacity + dx * 0.005).clamp(0.0, 1.0);
            commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                time_s: None,
                actor: actor.to_string(),
                property: "opacity".into(),
                value: PropertyValue::Float(new_opacity),
                create_keyframe: false,
            })));
        }
    });

    content.add_space(essentials_h + SPACE_S);

    // ── Property rows ──
    let content_rect = Rect::from_min_max(
        content.cursor().min,
        Pos2::new(popup_rect.max.x - SPACE_M, popup_rect.max.y - SPACE_M),
    );

    let mut rows_ui = ui.new_child(egui::UiBuilder::new().max_rect(content_rect));
    rows_ui.set_clip_rect(content_rect);

    // Position row
    let has_kf_pos = timeline.map(|tl| tl.has_keyframe_at(actor, "position", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut rows_ui, actor, "position", "Position",
        [props.position[0], props.position[1]],
        commands, has_kf_pos, current_time_s, timeline, time_ms,
    );

    // Size row
    let has_kf_size = timeline.map(|tl| tl.has_keyframe_at(actor, "size", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut rows_ui, actor, "size", "Size",
        [props.size[0], props.size[1]],
        commands, has_kf_size, current_time_s, timeline, time_ms,
    );

    // Rotation row
    let has_kf_rot = timeline.map(|tl| tl.has_keyframe_at(actor, "rotation", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut rows_ui, actor, "rotation", "Rotation",
        [props.rotation.to_degrees(), 0.0],
        commands, has_kf_rot, current_time_s, timeline, time_ms,
    );

    // Opacity row
    let has_kf_opac = timeline.map(|tl| tl.has_keyframe_at(actor, "opacity", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut rows_ui, actor, "opacity", "Opacity",
        [opacity * 100.0, 0.0],
        commands, has_kf_opac, current_time_s, timeline, time_ms,
    );
}

/// Render a single essential property (compact, no diamond).
/// Returns `Some((dx, dy))` if the user is dragging on the value area.
fn property_essential(ui: &mut egui::Ui, label: &str, value: &str, width: f32) -> Option<(f32, f32)> {
    let rect = Rect::from_min_size(ui.cursor().min, Vec2::new(width, 20.0));
    let mut local = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    local.set_clip_rect(rect);

    local.horizontal(|ui| {
        ui.label(RichText::new(label).size(FONT_SIZE_XS).color(TEXT_MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(value).size(FONT_SIZE_S).color(TEXT_PRIMARY));
        });
    });

    let response = ui.interact(rect, ui.id().with((label, "drag_value")), Sense::drag());

    if response.hovered() {
        response.clone().on_hover_text("Drag to change");
    }

    if response.dragged() {
        let delta = response.drag_delta();
        Some((delta.x, delta.y))
    } else {
        None
    }
}

/// Render a property row with a diamond keyframe toggle and inline value editing.
fn popup_property_row(
    ui: &mut egui::Ui,
    actor: &str,
    property: &str,
    label: &str,
    values: [f32; 2],
    commands: &mut ActionQueue,
    has_keyframe: bool,
    current_time_s: f64,
    _timeline: Option<&Timeline>,
    _time_ms: u64,
) {
    let row_h = 24.0;
    let row_rect = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), row_h));
    let response = ui.interact(row_rect, ui.id().with((actor, property, "row")), Sense::click());

    // Hover background
    if response.hovered() {
        ui.painter().rect_filled(row_rect, RADIUS_S as u8, BG_HOVER);
    }

    // Diamond keyframe toggle (clickable)
    let diamond_size = 8.0;
    let diamond_rect = Rect::from_center_size(
        Pos2::new(row_rect.min.x + 10.0, row_rect.center().y),
        Vec2::new(diamond_size + 6.0, diamond_size + 6.0), // Larger hit area
    );
    let diamond_resp = ui.interact(diamond_rect, ui.id().with((actor, property, "diamond")), Sense::click());

    let diamond_color = if has_keyframe || diamond_resp.hovered() { AMBER } else { TEXT_MUTED };
    let center = diamond_rect.center();
    let fill_color = if has_keyframe { diamond_color } else { Color32::TRANSPARENT };
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(center.x, center.y - diamond_size / 2.0),
            Pos2::new(center.x + diamond_size / 2.0, center.y),
            Pos2::new(center.x, center.y + diamond_size / 2.0),
            Pos2::new(center.x - diamond_size / 2.0, center.y),
        ],
        fill_color,
        Stroke::new(STROKE_WIDTH, diamond_color),
    ));

    // Diamond click: create or delete keyframe
    if diamond_resp.clicked() {
        if has_keyframe {
            // Delete keyframe
            commands.push_back(ShellAction::Command(Command::DeleteKeyframe {
                actor: actor.to_string(),
                property: property.to_string(),
                time_s: current_time_s,
            }));
        } else {
            // Create keyframe with current value
            let value = match property {
                "position" => PropertyValue::Vec2(values),
                "size" => PropertyValue::Vec2(values),
                "rotation" => PropertyValue::Float(values[0].to_radians()),
                "opacity" => PropertyValue::Float(values[0] / 100.0),
                _ => PropertyValue::Float(values[0]),
            };
            commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                time_s: None,
                actor: actor.to_string(),
                property: property.to_string(),
                value,
                create_keyframe: true,
            })));
        }
    }

    // Diamond tooltip
    if diamond_resp.hovered() {
        let tooltip = if has_keyframe {
            format!("Click to remove keyframe at {:.2}s", current_time_s)
        } else {
            format!("Click to add keyframe at {:.2}s", current_time_s)
        };
        diamond_resp.clone().on_hover_text(tooltip);
    }

    // Label
    let label_x = row_rect.min.x + 22.0;
    ui.painter().text(
        Pos2::new(label_x, row_rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        TEXT_SECONDARY,
    );

    // Value area (right-aligned, interactive)
    let value_area_width = 100.0;
    let value_rect = Rect::from_min_size(
        Pos2::new(row_rect.max.x - value_area_width - SPACE_S, row_rect.min.y),
        Vec2::new(value_area_width, row_h),
    );

    // Format value string
    let value_str = match property {
        "position" => format!("{:.1}, {:.1}", values[0], values[1]),
        "size" => format!("{:.1} × {:.1}", values[0], values[1]),
        "rotation" => format!("{:.1}°", values[0]),
        "opacity" => format!("{:.0}%", values[0]),
        _ => format!("{:.1}", values[0]),
    };

    // Interactive value: drag to change
    let value_resp = ui.interact(value_rect, ui.id().with((actor, property, "value")), Sense::click_and_drag());

    // Highlight on hover
    if value_resp.hovered() {
        ui.painter().rect_filled(value_rect, RADIUS_S as u8, BG_WIDGET);
    }

    ui.painter().text(
        value_rect.center(),
        egui::Align2::CENTER_CENTER,
        &value_str,
        egui::FontId::monospace(FONT_SIZE_S),
        if value_resp.hovered() { TEXT_PRIMARY } else { TEXT_MUTED },
    );

    // Drag on value to change
    if value_resp.dragged() {
        let delta = value_resp.drag_delta();
        match property {
            "position" => {
                commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: actor.to_string(),
                    property: "position".into(),
                    value: PropertyValue::Vec2([values[0] + delta.x, values[1] + delta.y]),
                    create_keyframe: false,
                })));
            }
            "size" => {
                let new_w = (values[0] + delta.x).max(1.0);
                let new_h = (values[1] + delta.y).max(1.0);
                commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: actor.to_string(),
                    property: "size".into(),
                    value: PropertyValue::Vec2([new_w, new_h]),
                    create_keyframe: false,
                })));
            }
            "rotation" => {
                let new_deg = (values[0] + delta.x * 0.5).rem_euclid(360.0);
                commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: actor.to_string(),
                    property: "rotation".into(),
                    value: PropertyValue::Float(new_deg.to_radians()),
                    create_keyframe: false,
                })));
            }
            "opacity" => {
                let new_opac = (values[0] + delta.x * 0.5).clamp(0.0, 100.0);
                commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: actor.to_string(),
                    property: "opacity".into(),
                    value: PropertyValue::Float(new_opac / 100.0),
                    create_keyframe: false,
                })));
            }
            _ => {}
        }
    }

    // Drag tooltip
    if value_resp.hovered() && !value_resp.dragged() {
        value_resp.clone().on_hover_text("Drag to change value");
    }

    // Row tooltip on hover (when not on value area)
    if response.hovered() && !value_resp.hovered() && !diamond_resp.hovered() {
        let tooltip = if has_keyframe {
            format!("{label}: {value_str}\nKeyframe at {current_time_s:.2}s")
        } else {
            format!("{label}: {value_str}\nNo keyframe at {current_time_s:.2}s")
        };
        response.clone().on_hover_text(tooltip);
    }

    ui.allocate_rect(row_rect, Sense::hover());
}
