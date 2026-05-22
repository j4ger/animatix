//! Natural-Language Command Bar
//!
//! Persistent lightweight input bar at the top of the window.
//! `⌘K` focuses. Live preview of agent intent. `Enter` confirm, `Esc` cancel.

use crate::app::commands::CommandQueue;
use crate::app::design_tokens::*;
use egui::{Color32, FontId, RichText, Vec2};

/// Renders the NL command bar at the top of the window.
pub(crate) fn nl_command_bar_ui(
    ui: &mut egui::Ui,
    _commands: &mut CommandQueue,
) {
    let available_w = ui.available_width();
    let height = ROW_L;

    ui.horizontal(|ui| {
        ui.set_width(available_w);
        ui.set_height(height);

        // ⌘K shortcut hint
        let shortcut = if cfg!(target_os = "macos") { "⌘K" } else { "Ctrl+K" };
        ui.add(
            egui::Label::new(
                RichText::new(shortcut).size(FONT_SIZE_XS).color(TEXT_MUTED),
            )
            .selectable(false),
        );

        ui.add_space(SPACE_S);

        // Input field
        let bar_id = ui.id().with("nl_command_input");
        let mut text = ui.data(|d| {
            d.get_temp::<String>(bar_id).unwrap_or_default()
        });

        let response = ui.add_sized(
            Vec2::new(ui.available_width() - 60.0, height - 4.0),
            egui::TextEdit::singleline(&mut text)
                .hint_text("Ask the agent… (e.g. '让 Circle_1 绕中心旋转一周')")
                .font(FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional))
                .margin(egui::vec2(SPACE_S, 4.0)),
        );

        // ⌘K focus shortcut
        let k_pressed = ui.input(|i| {
            let mod_down = if cfg!(target_os = "macos") {
                i.modifiers.mac_cmd
            } else {
                i.modifiers.ctrl
            };
            mod_down && i.key_pressed(egui::Key::K)
        });
        if k_pressed {
            response.request_focus();
        }

        // Handle Enter / Escape
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if !text.trim().is_empty() {
                // TODO: Send command to agent for processing
                // commands.push_back(Command::AgentRequest(text.clone()));
                tracing::info!("Agent request: {}", text);
            }
            text.clear();
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            text.clear();
            ui.ctx().memory_mut(|mem| mem.stop_text_input());
        }

        ui.data_mut(|d| d.insert_temp(bar_id, text));

        // Send button (right side)
        let send_btn = egui::Button::new(
            RichText::new(egui_phosphor::regular::PAPER_PLANE_RIGHT)
                .size(FONT_SIZE_M)
                .color(ACCENT_BLUE),
        )
        .fill(Color32::TRANSPARENT)
        .corner_radius(egui::CornerRadius::same(RADIUS_M as u8));
        if ui.add(send_btn).on_hover_text("Send to agent").clicked() {
            let text = ui.data(|d| d.get_temp::<String>(bar_id).unwrap_or_default());
            if !text.trim().is_empty() {
                tracing::info!("Agent request: {}", text);
            }
        }
    });

    // Subtle bottom border
    let rect = ui.min_rect();
    ui.painter().line_segment(
        [
            egui::pos2(rect.min.x, rect.max.y - 1.0),
            egui::pos2(rect.max.x, rect.max.y - 1.0),
        ],
        egui::Stroke::new(1.0, BORDER),
    );
}
