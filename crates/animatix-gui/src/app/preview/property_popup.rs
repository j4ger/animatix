use crate::app::commands::{ActionQueue, Command, ShellAction, ViewAction, PropertyEdit, PropertyValue};
use crate::app::components::button;
use crate::app::preview::{ActorProps, PreviewTransform};
use crate::app::design_tokens::*;
use animatix::timeline::{Timeline, SceneDimensions, read_property_value_or_default, PropertyValue as TlPropertyValue};
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
    scene_dimensions: SceneDimensions,
    zoom: f32,
    preview_rect: egui::Rect,
    pan: egui::Vec2,
) {
    if is_dragging {
        return; // Auto-hide during canvas drag
    }

    let viewport = ui.max_rect();
    let popup_w = 260.0;
    let popup_h = 170.0; // Slightly taller for better header spacing

    // Drag offset state (persisted between frames)
    let drag_offset_id = ui.id().with(("popup_drag_offset", actor));
    let drag_offset: Vec2 = ui.data(|d| d.get_temp(drag_offset_id)).unwrap_or(Vec2::ZERO);

    // Position: attached to top edge of actor, centered, with drag offset
    let popup_pos = Pos2::new(
        screen_pos.x - popup_w / 2.0 + drag_offset.x,
        screen_pos.y - popup_h - 12.0 + drag_offset.y,
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

    // ── Header: actor name + close (draggable) ──
    let header_h = ROW_L; // 28px - comfortable height for header
    let header_rect = Rect::from_min_size(
        content.cursor().min,
        Vec2::new(content.available_width(), header_h),
    );
    let header_resp = ui.interact(header_rect, ui.id().with("popup_header"), Sense::click_and_drag());
    if header_resp.dragged() {
        let new_offset = drag_offset + header_resp.drag_delta();
        ui.data_mut(|d| d.insert_temp(drag_offset_id, new_offset));
    }
    // Double-click header to reset position
    if header_resp.double_clicked() {
        ui.data_mut(|d| d.remove::<Vec2>(drag_offset_id));
    }
    
    // Draw header background on hover (drag affordance)
    if header_resp.hovered() {
        ui.painter().rect_filled(header_rect, RADIUS_S as u8, BG_HOVER);
    }
    
    // Subtle drag handle indicator (6 dots) on the left side of header
    let handle_center = Pos2::new(header_rect.min.x + SPACE_S + 4.0, header_rect.center().y);
    let dot_color = if header_resp.hovered() { TEXT_MUTED } else { TEXT_DISABLED };
    for row in 0..3 {
        for col in 0..2 {
            let dot_pos = Pos2::new(
                handle_center.x + col as f32 * 4.0,
                handle_center.y + (row as f32 - 1.0) * 4.0,
            );
            ui.painter().circle_filled(dot_pos, 1.0, dot_color);
        }
    }
    
    // Header content with proper padding
    let header_content_rect = header_rect.shrink2(Vec2::new(SPACE_S, 0.0));
    let mut header_ui = ui.new_child(egui::UiBuilder::new().max_rect(header_content_rect));
    header_ui.set_clip_rect(header_content_rect);
    header_ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);
        ui.label(RichText::new(actor).size(FONT_SIZE_M).color(TEXT_PRIMARY).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if button::icon_button(ui, egui_phosphor::regular::X, "Close").clicked() {
                commands.push_back(ShellAction::View(ViewAction::DeselectActors));
            }
        });
    });
    content.add_space(header_h + SPACE_S);

    // ── Property rows ──
    let time_ms = (current_time_s * 1000.0) as u64;

    // Read opacity from timeline
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

    let content_rect = Rect::from_min_max(
        content.cursor().min,
        Pos2::new(popup_rect.max.x - SPACE_M, popup_rect.max.y - SPACE_M),
    );

    let mut rows_ui = ui.new_child(egui::UiBuilder::new().max_rect(content_rect));
    rows_ui.set_clip_rect(content_rect);

    // Compute scale factor for converting screen delta to scene delta
    let scale = PreviewTransform::new(scene_dimensions, preview_rect, zoom, pan).scale();
    let scale_x = scale.0 as f32;
    let scale_y = scale.1 as f32;

    // Position row
    let has_kf_pos = timeline.map(|tl| tl.has_keyframe_at(actor, "position", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut rows_ui, actor, "position", "Position",
        [props.position[0], props.position[1]],
        commands, has_kf_pos, current_time_s, scale_x, scale_y,
    );

    // Size row
    let has_kf_size = timeline.map(|tl| tl.has_keyframe_at(actor, "size", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut rows_ui, actor, "size", "Size",
        [props.size[0], props.size[1]],
        commands, has_kf_size, current_time_s, scale_x, scale_y,
    );

    // Rotation row
    let has_kf_rot = timeline.map(|tl| tl.has_keyframe_at(actor, "rotation", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut rows_ui, actor, "rotation", "Rotation",
        [props.rotation.to_degrees(), 0.0],
        commands, has_kf_rot, current_time_s, scale_x, scale_y,
    );

    // Opacity row
    let has_kf_opac = timeline.map(|tl| tl.has_keyframe_at(actor, "opacity", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut rows_ui, actor, "opacity", "Opacity",
        [opacity * 100.0, 0.0],
        commands, has_kf_opac, current_time_s, scale_x, scale_y,
    );
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
    scale_x: f32,
    scale_y: f32,
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
        // Convert screen delta to scene delta
        let scene_dx = delta.x * scale_x;
        let scene_dy = delta.y * scale_y;
        match property {
            "position" => {
                commands.push_back(ShellAction::Command(Command::PropertyEdit(PropertyEdit {
                    time_s: None,
                    actor: actor.to_string(),
                    property: "position".into(),
                    value: PropertyValue::Vec2([values[0] + scene_dx, values[1] + scene_dy]),
                    create_keyframe: false,
                })));
            }
            "size" => {
                let new_w = (values[0] + scene_dx).max(1.0);
                let new_h = (values[1] + scene_dy).max(1.0);
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
