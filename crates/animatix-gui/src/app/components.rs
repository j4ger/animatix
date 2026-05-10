//! High-level reusable UI components.
//!
//! Built on top of egui primitives, themed consistently.

use egui::{Color32, CornerRadius, Id, Margin, Rect, Response, Sense, Stroke, Vec2};

use crate::app::theme::*;

// ─── Row ──────────────────────────────────────────────────────────────────

/// Response from a `Row`.
pub struct RowResponse {
    pub row_clicked: bool,
    pub chevron_clicked: bool,
    pub row_rect: Rect,
}

/// A full-width interactive row used in sidebars, property lists, and keyframe groups.
pub struct Row<'a> {
    pub height: f32,
    pub indent: f32,
    pub has_children: bool,
    pub is_expanded: bool,
    pub is_selected: bool,
    pub icon: Option<&'static str>,
    pub label: &'a str,
    pub label_color: Option<Color32>,
    pub right: Option<Box<dyn FnOnce(&mut egui::Ui) + 'a>>,
}

impl<'a> Row<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            height: ROW_M,
            indent: 0.0,
            has_children: false,
            is_expanded: false,
            is_selected: false,
            icon: None,
            label,
            label_color: None,
            right: None,
        }
    }

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    pub fn indent(mut self, px: f32) -> Self {
        self.indent = px;
        self
    }

    pub fn expanded(mut self, yes: bool) -> Self {
        self.is_expanded = yes;
        self
    }

    pub fn selected(mut self, yes: bool) -> Self {
        self.is_selected = yes;
        self
    }

    pub fn icon(mut self, icon: Option<&'static str>) -> Self {
        self.icon = icon;
        self
    }

    pub fn has_children(mut self, yes: bool) -> Self {
        self.has_children = yes;
        self
    }

    pub fn label_color(mut self, c: Color32) -> Self {
        self.label_color = Some(c);
        self
    }

    pub fn right<F: FnOnce(&mut egui::Ui) + 'a>(mut self, f: F) -> Self {
        self.right = Some(Box::new(f));
        self
    }

    pub fn show(self, ui: &mut egui::Ui, row_id: Id) -> RowResponse {
        let available = ui.available_width();
        let (row_rect, row_response) =
            ui.allocate_exact_size(Vec2::new(available, self.height), Sense::click());

        // Background
        let bg = if self.is_selected {
            BG_WIDGET
        } else if row_response.hovered() {
            BG_HOVER
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(row_rect, 0.0, bg);
        }

        // Selected accent bar
        if self.is_selected {
            let accent = Rect::from_min_size(row_rect.min, Vec2::new(2.0, row_rect.height()));
            ui.painter().rect_filled(accent, 0.0, ACCENT_BLUE);
        }

        let baseline_y = row_rect.center().y;
        let mut cursor_x = row_rect.min.x + SPACE_S + self.indent;

        // Chevron
        let chevron_rect = Rect::from_min_size(
            egui::pos2(cursor_x, row_rect.min.y),
            Vec2::new(14.0, self.height),
        );
        let chevron_response =
            ui.interact(chevron_rect, row_id.with("chevron"), Sense::click());

        if self.has_children {
            let icon = if self.is_expanded {
                egui_phosphor::regular::CARET_DOWN
            } else {
                egui_phosphor::regular::CARET_RIGHT
            };
            let color = if chevron_response.hovered() {
                TEXT_SECONDARY
            } else {
                TEXT_MUTED
            };
            ui.painter().text(
                egui::pos2(chevron_rect.center().x, baseline_y),
                egui::Align2::CENTER_CENTER,
                icon,
                egui::TextStyle::Small.resolve(ui.style()),
                color,
            );
        }
        cursor_x += 14.0;

        // Icon
        if let Some(icon_str) = self.icon {
            cursor_x += SPACE_S;
            let icon_rect = Rect::from_min_size(
                egui::pos2(cursor_x, row_rect.min.y),
                Vec2::new(14.0, self.height),
            );
            let default_color = if self.is_selected { TEXT_PRIMARY } else { TEXT_MUTED };
            ui.painter().text(
                egui::pos2(icon_rect.center().x, baseline_y),
                egui::Align2::CENTER_CENTER,
                icon_str,
                egui::TextStyle::Small.resolve(ui.style()),
                self.label_color.unwrap_or(default_color),
            );
            cursor_x += 14.0 + SPACE_S;
        } else {
            cursor_x += SPACE_S * 2.0;
        }

        // Label
        let label_color = self.label_color.unwrap_or_else(|| {
            if self.is_selected {
                TEXT_PRIMARY
            } else {
                TEXT_SECONDARY
            }
        });
        ui.painter().text(
            egui::pos2(cursor_x, baseline_y),
            egui::Align2::LEFT_CENTER,
            self.label,
            egui::TextStyle::Small.resolve(ui.style()),
            label_color,
        );

        // Right content
        if let Some(right) = self.right {
            let right_x = row_rect.max.x - SPACE_S;
            ui.allocate_ui_at_rect(
                Rect::from_min_size(
                    egui::pos2(cursor_x + SPACE_L, row_rect.min.y),
                    Vec2::new((right_x - cursor_x - SPACE_L).max(20.0), self.height),
                ),
                |ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
                },
            );
        }

        RowResponse {
            row_clicked: row_response.clicked() && !chevron_response.clicked(),
            chevron_clicked: chevron_response.clicked(),
            row_rect,
        }
    }
}

// ─── Card ─────────────────────────────────────────────────────────────────

/// A styled container with our surface background and rounded corners.
pub fn card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(BG_SURFACE)
        .corner_radius(CornerRadius::same(RADIUS_M as u8))
        .inner_margin(Margin::same(SPACE_M as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add_contents(ui);
        });
}

// ─── Section Header ───────────────────────────────────────────────────────

/// A collapsible section header with an accent line and icon.
pub fn section_header(ui: &mut egui::Ui, icon: &str, title: &str, count: Option<usize>) {
    let header_rect = ui.available_rect_before_wrap();
    let line_rect = Rect::from_min_size(header_rect.min, Vec2::new(24.0, 2.0));
    ui.painter().rect_filled(line_rect, RADIUS_S, ACCENT_BLUE);
    ui.add_space(5.0);

    let available = ui.available_width();
    let row_h = ROW_S;
    let (row_rect, _) = ui.allocate_exact_size(Vec2::new(available, row_h), egui::Sense::hover());
    let baseline_y = row_rect.center().y;
    let mut cursor_x = row_rect.min.x;

    // Icon
    ui.painter().text(
        egui::pos2(cursor_x + 7.0, baseline_y),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        TEXT_MUTED,
    );
    cursor_x += 18.0;

    // Title
    ui.painter().text(
        egui::pos2(cursor_x, baseline_y),
        egui::Align2::LEFT_CENTER,
        title.to_uppercase(),
        egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
        TEXT_MUTED,
    );

    // Count (right-aligned)
    if let Some(n) = count {
        ui.painter().text(
            egui::pos2(row_rect.max.x - SPACE_S, baseline_y),
            egui::Align2::RIGHT_CENTER,
            n.to_string(),
            egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
            TEXT_MUTED,
        );
    }

    ui.add_space(SPACE_S);
}

// ─── Empty State ──────────────────────────────────────────────────────────

/// Centered empty-state placeholder with icon, title, and subtitle.
pub fn empty_state(ui: &mut egui::Ui, icon: &str, title: &str, subtitle: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(SPACE_XL * 3.0);
        ui.add(
            egui::Label::new(egui::RichText::new(icon).size(28.0).color(TEXT_MUTED))
                .selectable(false),
        );
        ui.add_space(SPACE_M);
        ui.add(
            egui::Label::new(
                egui::RichText::new(title).size(FONT_SIZE_L).color(TEXT_SECONDARY),
            )
            .selectable(false),
        );
        ui.add_space(SPACE_S);
        ui.add(
            egui::Label::new(
                egui::RichText::new(subtitle).size(FONT_SIZE_M).color(TEXT_MUTED),
            )
            .selectable(false),
        );
    });
}

// ─── Field ────────────────────────────────────────────────────────────────

/// Wraps native egui widgets in our themed input frame.
///
/// Usage:
/// ```ignore
/// Field(ui, id, |ui| {
///     ui.add(egui::DragValue::new(&mut val));
/// });
/// ```
pub fn field(ui: &mut egui::Ui, id: Id, add_contents: impl FnOnce(&mut egui::Ui)) -> Response {
    let frame = egui::Frame::new()
        .fill(BG_WIDGET)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(RADIUS_M as u8))
        .inner_margin(Margin::symmetric(SPACE_S as i8, 1));

    let response = frame.show(ui, |ui| {
        ui.set_width(ui.available_width());
        add_contents(ui)
    });
    response.response
}

// ─── KeyframeDot ──────────────────────────────────────────────────────────

/// Draws a diamond-shaped keyframe marker.
pub fn keyframe_dot(
    painter: &egui::Painter,
    center: egui::Pos2,
    size: f32,
    is_active: bool,
) {
    let color = if is_active { TEXT_PRIMARY } else { AMBER };
    let half = size * 0.5;
    let points = vec![
        center + Vec2::new(0.0, -half),
        center + Vec2::new(half, 0.0),
        center + Vec2::new(0.0, half),
        center + Vec2::new(-half, 0.0),
    ];
    painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
}

// ─── Playhead ─────────────────────────────────────────────────────────────

/// Draws the vertical amber playhead line.
pub fn playhead(painter: &egui::Painter, x: f32, y_range: std::ops::Range<f32>) {
    painter.line_segment(
        [egui::pos2(x, y_range.start), egui::pos2(x, y_range.end)],
        Stroke::new(1.5, AMBER),
    );
}

// ─── TimelineStrip ────────────────────────────────────────────────────────

/// A mini timeline strip that returns a scrub time on click/drag.
pub struct TimelineStrip<'a> {
    pub duration_s: f64,
    pub current_time_s: f64,
    pub keyframes: &'a [f64],
    pub height: f32,
}

impl<'a> TimelineStrip<'a> {
    pub fn show(self, ui: &mut egui::Ui, id: Id) -> Option<f64> {
        let desired = Vec2::new(ui.available_width(), self.height);
        let (rect, response) = ui.allocate_exact_size(desired, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        let track = rect.shrink2(Vec2::new(SPACE_S, 3.0));
        painter.rect_filled(track, RADIUS_M, BG_WIDGET);
        painter.rect_stroke(track, RADIUS_M, Stroke::new(1.0, BORDER), egui::StrokeKind::Outside);

        // Tick marks
        let sec_step = if self.duration_s > 20.0 { 5.0 } else { 1.0 };
        let mut sec = sec_step;
        while sec < self.duration_s {
            let frac = (sec / self.duration_s) as f32;
            let x = egui::lerp(track.left()..=track.right(), frac);
            painter.line_segment(
                [
                    egui::pos2(x, track.top() + 2.0),
                    egui::pos2(x, track.bottom() - 2.0),
                ],
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 15)),
            );
            sec += sec_step;
        }

        // Keyframe dots
        for &kf in self.keyframes {
            let frac = ((kf / self.duration_s) as f32).clamp(0.0, 1.0);
            let x = egui::lerp(track.left()..=track.right(), frac);
            keyframe_dot(&painter, egui::pos2(x, track.center().y), 4.0, false);
        }

        // Playhead
        let playhead_frac = ((self.current_time_s / self.duration_s) as f32).clamp(0.0, 1.0);
        let playhead_x = egui::lerp(track.left()..=track.right(), playhead_frac);
        playhead(&painter, playhead_x, track.top() - 1.0..track.bottom() + 1.0);

        // Interaction
        if (response.clicked() || response.dragged()) && response.interact_pointer_pos().is_some()
        {
            let pos = response.interact_pointer_pos().unwrap();
            let frac = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
            return Some(frac * self.duration_s);
        }

        None
    }
}
