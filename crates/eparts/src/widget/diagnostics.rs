use egui::{Color32, Rect, RichText, Sense, Stroke, Vec2};

use crate::tokens::semantic::{border, status, surface, text};
use crate::tokens::spatial::{ROW_L, SPACE_L, SPACE_M, SPACE_S, STROKE_WIDTH};
use crate::tokens::typography::TextRole;
use super::layout::card;

/// Where to place the cursor after clicking a diagnostic.
#[derive(Clone, Copy, Debug)]
pub struct DiagnosticTarget {
    /// 0-indexed source line.
    pub line: usize,
    /// 0-indexed source column.
    pub column: usize,
}

/// Trait abstracting over a diagnostic entry so the widget is decoupled
/// from any concrete diagnostic type.
pub trait DiagnosticEntry {
    /// Returns true if this diagnostic represents an error (high severity).
    fn is_error(&self) -> bool;
    /// Human-readable message describing the issue.
    fn message(&self) -> &str;
    /// 1-based source line, or `None` if unknown.
    fn line(&self) -> Option<usize>;
    /// 1-based source column, or `None` if unknown (defaults to 0 on click).
    fn column(&self) -> Option<usize>;
    /// Optional human-readable phase label (e.g. "parse", "build").
    /// Defaults to `None`.
    fn phase_label(&self) -> Option<&str> {
        None
    }
    /// Optional color for the phase badge.
    /// Defaults to `None`, which renders the badge in `text::MUTED`.
    fn phase_color(&self) -> Option<egui::Color32> {
        None
    }
}

/// Renders a scrollable card of diagnostic messages.
///
/// `visible` is set to `false` when the user clicks the close button.
pub fn diagnostics_list<T: DiagnosticEntry>(
    ui: &mut egui::Ui,
    diagnostics: &[T],
    visible: &mut bool,
) -> Option<DiagnosticTarget> {
    if diagnostics.is_empty() {
        return None;
    }

    let mut clicked_target: Option<DiagnosticTarget> = None;

    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

            let error_count = diagnostics.iter().filter(|d| d.is_error()).count();
            let warning_count = diagnostics.iter().filter(|d| !d.is_error()).count();

            ui.add(
                egui::Label::new(
                    RichText::new(egui_phosphor::regular::WARNING_OCTAGON)
                        .size(TextRole::BodyS.size())
                        .color(text::MUTED),
                )
                .selectable(false),
            );

            ui.add(
                egui::Label::new(
                    RichText::new("Diagnostics")
                        .size(TextRole::BodyS.size())
                        .color(text::SECONDARY),
                )
                .selectable(false),
            );

            if error_count > 0 {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} {}", egui_phosphor::regular::X, error_count))
                            .size(TextRole::Micro.size())
                            .color(status::ERROR),
                    )
                    .selectable(false),
                );
            }
            if warning_count > 0 {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!(
                            "{} {}",
                            egui_phosphor::regular::WARNING,
                            warning_count
                        ))
                        .size(TextRole::Micro.size())
                        .color(status::WARNING),
                    )
                    .selectable(false),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(egui_phosphor::regular::X)
                                .size(TextRole::BodyS.size())
                                .color(text::MUTED),
                        )
                        .frame(false),
                    )
                    .clicked()
                {
                    *visible = false;
                }
            });
        });

        ui.add_space(SPACE_S);

        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
            for (i, d) in diagnostics.iter().enumerate() {
                if let Some(target) = diagnostic_row(ui, d, i == diagnostics.len() - 1) {
                    clicked_target = Some(target);
                }
            }
        });
    });

    clicked_target
}

fn diagnostic_row<T: DiagnosticEntry>(
    ui: &mut egui::Ui,
    diagnostic: &T,
    is_last: bool,
) -> Option<DiagnosticTarget> {
    let available = ui.available_width();
    let row_h = ROW_L;
    let (row_rect, response) = ui.allocate_exact_size(Vec2::new(available, row_h), Sense::click());

    let accent_color = if diagnostic.is_error() {
        status::ERROR
    } else {
        status::WARNING
    };
    let icon = if diagnostic.is_error() {
        egui_phosphor::regular::X
    } else {
        egui_phosphor::regular::WARNING
    };

    let bg = if response.hovered() {
        surface::HOVER
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(row_rect, 0.0, bg);
    }

    let accent_rect = Rect::from_min_size(row_rect.min, Vec2::new(2.0, row_rect.height()));
    ui.painter().rect_filled(accent_rect, 0.0, accent_color);

    let baseline_y = row_rect.center().y;
    let mut cursor_x = row_rect.min.x + SPACE_M + 2.0;

    ui.painter().text(
        egui::pos2(cursor_x + 7.0, baseline_y),
        egui::Align2::CENTER_CENTER,
        icon,
        TextRole::BodyS.font_id(),
        accent_color,
    );
    cursor_x += 18.0;

    let phase_str = diagnostic.phase_label().unwrap_or_default();
    let phase_badge_w = 40.0_f32;
    let msg_max_width = (row_rect.max.x - cursor_x - SPACE_L - phase_badge_w).max(20.0);

    let msg = diagnostic.message().lines().next().unwrap_or_default();
    let font_id = TextRole::Body.font_id();
    let galley =
        ui.painter()
            .layout(msg.to_string(), font_id.clone(), text::PRIMARY, msg_max_width);

    ui.painter().galley(
        egui::pos2(cursor_x, baseline_y - galley.size().y / 2.0),
        galley,
        text::PRIMARY,
    );

    ui.painter().text(
        egui::pos2(row_rect.max.x - SPACE_S, baseline_y),
        egui::Align2::RIGHT_CENTER,
        phase_str,
        TextRole::Micro.font_id(),
        diagnostic.phase_color().unwrap_or(text::MUTED),
    );

    if !is_last {
        ui.painter().line_segment(
            [
                egui::pos2(row_rect.min.x + SPACE_M, row_rect.bottom() - 0.5),
                egui::pos2(row_rect.max.x - SPACE_S, row_rect.bottom() - 0.5),
            ],
            Stroke::new(STROKE_WIDTH, border::DEFAULT),
        );
    }

    if response.clicked() {
        let line = diagnostic.line()?.saturating_sub(1);
        let column = diagnostic.column().map(|c| c.saturating_sub(1)).unwrap_or(0);
        Some(DiagnosticTarget { line, column })
    } else {
        None
    }
}
