use crate::app::commands::{Command, CommandQueue, PropertyEdit, PropertyValue};
use crate::app::components;
use crate::app::preview::ActorProps;
use crate::app::design_tokens::*;
use animatix::timeline::Timeline;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};

/// Tab categories in the property popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PopupTab {
    Transform,
    Style,
    Shape,
    Text,
}

/// Show the property popup attached to a selected actor.
///
/// Attached to actor's top edge, follows actor, clamped to viewport.
/// Auto-hides during canvas drag.
pub fn show_property_popup(
    ui: &mut egui::Ui,
    actor: &str,
    props: &ActorProps,
    screen_pos: Pos2,
    commands: &mut CommandQueue,
    is_dragging: bool,
    timeline: Option<&Timeline>,
    current_time_s: f64,
) {
    if is_dragging {
        return; // Auto-hide during canvas drag
    }

    let viewport = ui.max_rect();
    let popup_w = 260.0;
    let popup_h = 180.0;

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
        Stroke::new(1.0, BORDER),
        egui::StrokeKind::Outside,
    );

    // Build child UI for content
    let mut content = ui.new_child(egui::UiBuilder::new().max_rect(popup_rect));
    content.set_clip_rect(popup_rect);
    content.add_space(SPACE_S);

    // ── Header: actor name + close + pin ──
    content.horizontal(|ui| {
        ui.label(RichText::new(actor).size(FONT_SIZE_M).color(TEXT_PRIMARY).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(SPACE_XS, 0.0);
            if components::icon_button(ui, egui_phosphor::regular::X, "Close").clicked() {
                // Close is implicit — deselect actor
                // (handled by Esc or clicking elsewhere)
            }
        });
    });
    content.add_space(SPACE_S);

    // ── Essentials row: 4 properties ──
    let essentials_h = 56.0;
    let essentials_rect = Rect::from_min_size(
        Pos2::new(popup_rect.min.x + SPACE_S, content.cursor().min.y),
        Vec2::new(popup_w - SPACE_S * 2.0, essentials_h),
    );

    // Background for essentials
    ui.painter().rect_filled(essentials_rect, RADIUS_M as u8, BG_BASE);

    let mut ess_ui = ui.new_child(egui::UiBuilder::new().max_rect(essentials_rect));
    ess_ui.set_clip_rect(essentials_rect);
    ess_ui.add_space(SPACE_XS);

    // 2×2 grid
    let col_w = (essentials_rect.width() - SPACE_S) / 2.0;

    // Row 1: Position | Size
    ess_ui.horizontal(|ui| {
        ui.set_width(essentials_rect.width());
        ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

        property_essential(ui, "Pos", &format!("{:.0}, {:.0}", props.position[0], props.position[1]), col_w);
        property_essential(ui, "Size", &format!("{:.0}×{:.0}", props.size[0], props.size[1]), col_w);
    });

    // Row 2: Rotation | Opacity
    ess_ui.horizontal(|ui| {
        ui.set_width(essentials_rect.width());
        ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

        let rot_deg = props.rotation.to_degrees();
        let rot_delta = property_essential(ui, "Rot", &format!("{:.0}°", rot_deg), col_w);
        if let Some(delta) = rot_delta {
            let new_rot_deg = (rot_deg + delta * 0.5).rem_euclid(360.0);
            commands.push_back(Command::PropertyEdit(PropertyEdit {
                actor: actor.to_string(),
                property: "rotation".into(),
                value: PropertyValue::Float(new_rot_deg.to_radians()),
                create_keyframe: false,
            }));
        }
        // Opacity not in ActorProps — show placeholder
        property_essential(ui, "Opac", "100%", col_w);
    });

    content.add_space(essentials_h + SPACE_S);

    // ── Tabs ──
    let tab_h = 24.0;
    let tabs = [
        (PopupTab::Transform, "Transform"),
        (PopupTab::Transform, "Style"),
        (PopupTab::Transform, "Shape"),
        (PopupTab::Transform, "Text"),
    ];

    content.horizontal(|ui| {
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 0.0);
        for (i, (_, label)) in tabs.iter().enumerate() {
            let is_first = i == 0;
            let tab_color = if is_first { TEXT_PRIMARY } else { TEXT_MUTED };
            let tab_bg = if is_first { BG_WIDGET } else { Color32::TRANSPARENT };
            let tab_rect = Rect::from_min_size(
                ui.cursor().min,
                Vec2::new(58.0, tab_h),
            );
            ui.painter().rect_filled(tab_rect, RADIUS_S as u8, tab_bg);
            ui.painter().text(
                tab_rect.center(),
                egui::Align2::CENTER_CENTER,
                *label,
                egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                tab_color,
            );
            ui.allocate_rect(tab_rect, egui::Sense::hover());
        }
    });

    content.add_space(SPACE_S);

    // ── Transform tab content (default) ──
    let content_rect = Rect::from_min_max(
        Pos2::new(popup_rect.min.x + SPACE_S, content.cursor().min.y),
        Pos2::new(popup_rect.max.x - SPACE_S, popup_rect.max.y - SPACE_S),
    );

    let mut tab_ui = ui.new_child(egui::UiBuilder::new().max_rect(content_rect));
    tab_ui.set_clip_rect(content_rect);

    // Position row with diamond
    let time_ms = (current_time_s * 1000.0) as u64;
    let has_kf_pos = timeline.map(|tl| tl.has_keyframe_at(actor, "position", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut tab_ui,
        actor,
        "position",
        "Position",
        &format!("{:.1}, {:.1}", props.position[0], props.position[1]),
        commands,
        has_kf_pos,
        current_time_s,
    );

    // Size row with diamond
    let has_kf_size = timeline.map(|tl| tl.has_keyframe_at(actor, "size", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut tab_ui,
        actor,
        "size",
        "Size",
        &format!("{:.1} × {:.1}", props.size[0], props.size[1]),
        commands,
        has_kf_size,
        current_time_s,
    );

    // Rotation row with diamond
    let has_kf_rot = timeline.map(|tl| tl.has_keyframe_at(actor, "rotation", time_ms)).unwrap_or(false);
    popup_property_row(
        &mut tab_ui,
        actor,
        "rotation",
        "Rotation",
        &format!("{:.1}°", props.rotation.to_degrees()),
        commands,
        has_kf_rot,
        current_time_s,
    );
}

/// Render a single essential property (compact, no diamond).
/// Returns `Some(drag_delta.x)` if the user is dragging on the value area.
fn property_essential(ui: &mut egui::Ui, label: &str, value: &str, width: f32) -> Option<f32> {
    let rect = Rect::from_min_size(ui.cursor().min, Vec2::new(width, 22.0));
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
        response.clone().on_hover_text("Drag left/right to change");
    }

    if response.dragged() {
        Some(response.drag_delta().x)
    } else {
        None
    }
}

/// Render a property row with a diamond keyframe toggle.
fn popup_property_row(
    ui: &mut egui::Ui,
    actor: &str,
    property: &str,
    label: &str,
    value: &str,
    commands: &mut CommandQueue,
    has_keyframe: bool,
    current_time_s: f64,
) {
    let row_h = 22.0;
    let row_rect = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), row_h));

    // Diamond keyframe toggle (filled ◆ if keyframe exists, hollow ○ if not)
    let diamond_size = 8.0;
    let diamond_rect = Rect::from_center_size(
        Pos2::new(row_rect.min.x + 10.0, row_rect.center().y),
        Vec2::new(diamond_size, diamond_size),
    );

    // Draw diamond based on keyframe state
    let diamond_color = AMBER;
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
        Stroke::new(1.0, diamond_color),
    ));

    // Label
    ui.painter().text(
        Pos2::new(row_rect.min.x + 22.0, row_rect.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        TEXT_SECONDARY,
    );

    // Value (right-aligned)
    ui.painter().text(
        Pos2::new(row_rect.max.x - SPACE_S, row_rect.center().y),
        egui::Align2::RIGHT_CENTER,
        value.to_string(),
        egui::FontId::monospace(FONT_SIZE_S),
        TEXT_PRIMARY,
    );

    // Hover: show tooltip
    let interact = ui.interact(row_rect, ui.id().with((actor, property)), egui::Sense::hover());
    if interact.hovered() {
        let tooltip = if has_keyframe {
            format!("{label}: {value}\nKeyframe at {current_time_s:.2}s\nClick to remove keyframe")
        } else {
            format!("{label}: {value}\nNo keyframe at {current_time_s:.2}s")
        };
        interact.on_hover_text(tooltip);
    }

    ui.allocate_rect(row_rect, egui::Sense::hover());
}
