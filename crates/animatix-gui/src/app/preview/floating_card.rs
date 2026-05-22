use crate::app::commands::{Command, CommandQueue, PropertyEdit, PropertyValue};
use crate::app::preview::ActorProps;
use crate::app::theme::*;
use egui::{Vec2, Pos2, Stroke, RichText};

/// A translucent floating card that appears next to a selected actor
/// on the preview canvas, offering direct manipulation of key properties.
pub fn show_floating_card(
    ui: &mut egui::Ui,
    actor: &str,
    props: &ActorProps,
    screen_pos: Pos2,
    commands: &mut CommandQueue,
) {
    let card_size = Vec2::new(180.0, 140.0);
    let card_pos = Pos2::new(screen_pos.x + 12.0, screen_pos.y - 12.0);
    let card_rect = egui::Rect::from_min_size(card_pos, card_size);

    // Clamp to viewport
    let viewport = ui.max_rect();
    let clamped_min = Pos2::new(
        card_rect.min.x.max(viewport.min.x),
        card_rect.min.y.max(viewport.min.y),
    );
    let clamped_rect = egui::Rect::from_min_size(clamped_min, card_size);

    // Background
    ui.painter().rect_filled(
        clamped_rect,
        6.0,
        floating_card_bg(),
    );
    ui.painter().rect_stroke(
        clamped_rect,
        6.0,
        Stroke::new(1.0, ACCENT_BLUE),
        egui::StrokeKind::Outside,
    );

    // Content
    let mut content_ui = ui.new_child(egui::UiBuilder::new().max_rect(clamped_rect));
    content_ui.set_clip_rect(clamped_rect);

    content_ui.add_space(6.0);
    content_ui.horizontal(|ui| {
        ui.label(RichText::new(actor).color(ACCENT_BLUE).strong());
    });
    content_ui.add_space(4.0);

    // Position row
    content_ui.horizontal(|ui| {
        ui.label(RichText::new("Pos").color(TEXT_SECONDARY).size(11.0));
        ui.add_space(4.0);
        let mut x = props.position[0];
        let mut y = props.position[1];
        let x_changed = ui.add_sized(
            [48.0, 18.0],
            egui::DragValue::new(&mut x).speed(1.0).fixed_decimals(1)
        ).changed();
        let y_changed = ui.add_sized(
            [48.0, 18.0],
            egui::DragValue::new(&mut y).speed(1.0).fixed_decimals(1)
        ).changed();
        if x_changed || y_changed {
            commands.push_back(Command::PropertyEdit(PropertyEdit {
                actor: actor.to_string(),
                property: "position".to_string(),
                value: PropertyValue::Vec2([x, y]),
                create_keyframe: false,
            }));
        }
    });

    // Size row
    content_ui.horizontal(|ui| {
        ui.label(RichText::new("Size").color(TEXT_SECONDARY).size(11.0));
        ui.add_space(4.0);
        let mut w = props.size[0];
        let mut h = props.size[1];
        let w_changed = ui.add_sized(
            [48.0, 18.0],
            egui::DragValue::new(&mut w).speed(1.0).fixed_decimals(1)
        ).changed();
        let h_changed = ui.add_sized(
            [48.0, 18.0],
            egui::DragValue::new(&mut h).speed(1.0).fixed_decimals(1)
        ).changed();
        if w_changed || h_changed {
            commands.push_back(Command::PropertyEdit(PropertyEdit {
                actor: actor.to_string(),
                property: "size".to_string(),
                value: PropertyValue::Vec2([w, h]),
                create_keyframe: false,
            }));
        }
    });

    // Rotation row
    content_ui.horizontal(|ui| {
        ui.label(RichText::new("Rot").color(TEXT_SECONDARY).size(11.0));
        ui.add_space(4.0);
        let mut rot = props.rotation.to_degrees();
        if ui.add_sized(
            [48.0, 18.0],
            egui::DragValue::new(&mut rot).speed(1.0).fixed_decimals(1).suffix("°")
        ).changed() {
            commands.push_back(Command::PropertyEdit(PropertyEdit {
                actor: actor.to_string(),
                property: "rotation".to_string(),
                value: PropertyValue::Float(rot.to_radians()),
                create_keyframe: false,
            }));
        }
    });
}
