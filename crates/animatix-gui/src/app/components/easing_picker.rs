use animatix::easing::{apply_easing, Easing, EASING_REGISTRY};
use egui::{Sense, Stroke, Vec2};

use crate::app::design_tokens::*;

/// A reusable easing picker that renders a ComboBox populated from
/// [`animatix::easing::EASING_REGISTRY`] with a 40×20px mini curve preview.
///
/// Returns the response from the ComboBox so callers can check `.changed()`.
#[allow(dead_code)]
pub fn easing_picker(ui: &mut egui::Ui, id: egui::Id, easing: &mut Easing) -> egui::Response {
    let mut response = None;

    ui.horizontal(|ui| {
        // Dropdown
        let selected_text = easing_display_name(*easing);
        let combo = egui::ComboBox::from_id_salt(id)
            .width(100.0)
            .selected_text(selected_text)
            .show_ui(ui, |ui| {
                for &(id_str, display_name) in EASING_REGISTRY {
                    let variant = parse_easing_id(id_str).unwrap_or(Easing::Linear);
                    if ui.selectable_value(easing, variant, display_name).changed() {
                        // easing already updated by selectable_value
                    }
                }
            });
        response = Some(combo.response);

        ui.add_space(SPACE_S);

        // Mini curve preview: 40×20px
        let preview_size = Vec2::new(40.0, 20.0);
        let (preview_rect, _preview_response) =
            ui.allocate_exact_size(preview_size, Sense::hover());

        // Background
        ui.painter().rect_filled(preview_rect, RADIUS_S, BG_WIDGET);
        ui.painter().rect_stroke(
            preview_rect,
            RADIUS_S,
            Stroke::new(1.0, BORDER),
            egui::StrokeKind::Inside,
        );

        // Draw curve: sample 20 points
        let padding = 2.0;
        let graph_rect = preview_rect.shrink(padding);
        if graph_rect.width() > 0.0 && graph_rect.height() > 0.0 {
            let samples = 20;
            let mut points = Vec::with_capacity(samples);
            for i in 0..=samples {
                let t = i as f32 / samples as f32;
                let eased = apply_easing(t, *easing);
                let x = graph_rect.left() + t * graph_rect.width();
                let y = graph_rect.bottom() - eased * graph_rect.height();
                points.push(egui::pos2(x, y));
            }
            ui.painter().add(egui::Shape::line(
                points,
                Stroke::new(1.5, ACCENT_BLUE),
            ));
        }
    });

    response.unwrap_or_else(|| ui.allocate_response(Vec2::ZERO, Sense::hover()))
}

#[allow(dead_code)]
fn easing_display_name(easing: Easing) -> &'static str {
    match easing {
        Easing::Linear => "Linear",
        Easing::EaseIn => "Ease In",
        Easing::EaseOut => "Ease Out",
        Easing::EaseInOut => "Ease In Out",
        Easing::Bounce => "Bounce",
        Easing::Elastic => "Elastic",
        Easing::Back => "Back",
        Easing::Expo => "Expo",
    }
}

#[allow(dead_code)]
fn parse_easing_id(id: &str) -> Option<Easing> {
    match id {
        "linear" => Some(Easing::Linear),
        "easein" => Some(Easing::EaseIn),
        "easeout" => Some(Easing::EaseOut),
        "easeinout" => Some(Easing::EaseInOut),
        "bounce" => Some(Easing::Bounce),
        "elastic" => Some(Easing::Elastic),
        "back" => Some(Easing::Back),
        "expo" => Some(Easing::Expo),
        _ => None,
    }
}
