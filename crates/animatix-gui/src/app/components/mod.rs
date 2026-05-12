//! High-level reusable UI components.
//!
//! All components in this module are built on top of egui primitives and share
//! the design tokens from [`crate::app::theme`].

//! # Component catalogue
//!
//! | Component | Purpose |
//! |-----------|---------|
//! | [`Row`] | Full-width interactive row (sidebar, property lists, trees) |
//! | [`card`] | Styled container with surface background and rounded corners |
//! | [`section_header`] | Collapsible section header with accent line and icon |
//! | [`empty_state`] | Centered placeholder for empty panels |
//! | [`field`] | Themed input frame for DragValue, Slider, TextEdit, etc. |
//! | [`icon_button`] | Small square icon button with hover feedback |
//! | [`keyframe_dot`] | Diamond-shaped keyframe marker |
//! | [`playhead`] | Vertical amber playhead line |
//! | [`TimelineStrip`] | Mini timeline scrubber with keyframe markers |
//! | [`diagnostics_list`] | Scrollable card of diagnostic messages |

pub mod widgets;

use egui::{Color32, CornerRadius, Id, Margin, Rect, Response, RichText, Sense, Stroke, Vec2};

use crate::app::theme::*;
use animatix::diagnostics::{Diagnostic, DiagnosticPhase, DiagnosticSeverity};

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

// ─── Icon Button ──────────────────────────────────────────────────────────

/// A small square icon button with hover highlight.
///
/// Default size is 28×28 px with a subtle rounded background on hover.
/// Returns the [`Response`] so callers can check `.clicked()`.
///
/// ```ignore
/// if icon_button(ui, egui_phosphor::regular::PLAY, "Play").clicked() {
///     // …
/// }
/// ```
pub fn icon_button(
    ui: &mut egui::Ui,
    icon: &str,
    tooltip: &str,
) -> Response {
    let size = Vec2::new(ROW_L, ROW_L);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, RADIUS_M, BG_HOVER);
    }

    let icon_color = if response.hovered() {
        TEXT_PRIMARY
    } else {
        TEXT_SECONDARY
    };

    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::TextStyle::Body.resolve(ui.style()),
        icon_color,
    );

    if !tooltip.is_empty() {
        return response.on_hover_text(tooltip);
    }
    response
}

/// Variant of [`icon_button`] that uses a custom icon color instead of the
/// default muted → primary hover transition.
pub fn icon_button_colored(
    ui: &mut egui::Ui,
    icon: &str,
    tooltip: &str,
    color: Color32,
    hover_color: Color32,
) -> Response {
    let size = Vec2::new(ROW_L, ROW_L);
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    if response.hovered() || response.is_pointer_button_down_on() {
        ui.painter().rect_filled(rect, RADIUS_M, BG_HOVER);
    }

    let icon_color = if response.hovered() { hover_color } else { color };

    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::TextStyle::Body.resolve(ui.style()),
        icon_color,
    );

    if !tooltip.is_empty() {
        return response.on_hover_text(tooltip);
    }
    response
}

/// A compact badge button showing an icon + count (e.g. "✕ 3").
///
/// Width expands automatically to fit the label.  Returns the [`Response`].
///
/// ```ignore
/// if badge_button(ui, egui_phosphor::regular::X, 3, RED, TEXT_PRIMARY, "Errors").clicked() {
///     // …
/// }
/// ```
pub fn badge_button(
    ui: &mut egui::Ui,
    icon: &str,
    count: usize,
    color: Color32,
    hover_color: Color32,
    tooltip: &str,
) -> Response {
    let label = format!("{} {}", icon, count);
    let galley = ui.painter().layout(
        label.clone(),
        egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
        color,
        f32::INFINITY,
    );

    let padding = Vec2::new(SPACE_M * 2.0, SPACE_S);
    let size = Vec2::new(
        galley.size().x + padding.x,
        ROW_L.max(galley.size().y + padding.y),
    );

    let (rect, response) = ui.allocate_exact_size(size, Sense::click());

    // Background
    let bg = if response.is_pointer_button_down_on() {
        BG_ACTIVE
    } else if response.hovered() {
        BG_HOVER
    } else {
        BG_WIDGET
    };
    ui.painter().rect_filled(rect, RADIUS_M, bg);
    ui.painter().rect_stroke(rect, RADIUS_M, Stroke::new(1.0, BORDER), egui::StrokeKind::Inside);

    // Text
    let text_color = if response.hovered() { hover_color } else { color };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
        text_color,
    );

    if !tooltip.is_empty() {
        return response.on_hover_text(tooltip);
    }
    response
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

// ─── Diagnostics List ─────────────────────────────────────────────────────

/// Where to place the cursor after clicking a diagnostic.
#[derive(Clone, Copy, Debug)]
pub struct DiagnosticTarget {
    /// 0-indexed source line.
    pub line: usize,
    /// 0-indexed source column.
    pub column: usize,
}

/// Renders a scrollable card of diagnostic messages.
///
/// Returns the target location if a diagnostic was clicked.
pub fn diagnostics_list(
    ui: &mut egui::Ui,
    diagnostics: &[Diagnostic],
) -> Option<DiagnosticTarget> {
    if diagnostics.is_empty() {
        return None;
    }

    let mut clicked_target: Option<DiagnosticTarget> = None;

    card(ui, |ui| {
        // Header with counts
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(SPACE_S, 0.0);

            let error_count = diagnostics.iter().filter(|d| d.is_error()).count();
            let warning_count = diagnostics.iter().filter(|d| !d.is_error()).count();

            ui.add(
                egui::Label::new(
                    RichText::new(egui_phosphor::regular::WARNING_OCTAGON)
                        .size(FONT_SIZE_S)
                        .color(TEXT_MUTED),
                )
                .selectable(false),
            );

            ui.add(
                egui::Label::new(
                    RichText::new("Diagnostics")
                        .size(FONT_SIZE_S)
                        .color(TEXT_SECONDARY),
                )
                .selectable(false),
            );

            if error_count > 0 {
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("{} {}", egui_phosphor::regular::X, error_count))
                            .size(FONT_SIZE_XS)
                            .color(RED),
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
                        .size(FONT_SIZE_XS)
                        .color(AMBER),
                    )
                    .selectable(false),
                );
            }
        });

        ui.add_space(SPACE_S);

        // Diagnostic rows
        egui::ScrollArea::vertical()
            .max_height(180.0)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                for (i, d) in diagnostics.iter().enumerate() {
                    if let Some(target) =
                        diagnostic_row(ui, d, i == diagnostics.len() - 1)
                    {
                        clicked_target = Some(target);
                    }
                }
            });
    });

    clicked_target
}

/// Render a single diagnostic row. Returns the target location if clicked.
fn diagnostic_row(
    ui: &mut egui::Ui,
    diagnostic: &Diagnostic,
    is_last: bool,
) -> Option<DiagnosticTarget> {
    let available = ui.available_width();
    let row_h = ROW_L;
    let (row_rect, response) =
        ui.allocate_exact_size(Vec2::new(available, row_h), Sense::click());

    // Severity-based accent
    let accent_color = if diagnostic.is_error() { RED } else { AMBER };
    let icon = if diagnostic.is_error() {
        egui_phosphor::regular::X
    } else {
        egui_phosphor::regular::WARNING
    };

    // Background
    let bg = if response.hovered() {
        BG_HOVER
    } else {
        Color32::TRANSPARENT
    };
    if bg != Color32::TRANSPARENT {
        ui.painter().rect_filled(row_rect, 0.0, bg);
    }

    // Left accent bar
    let accent_rect = Rect::from_min_size(
        row_rect.min,
        Vec2::new(2.0, row_rect.height()),
    );
    ui.painter().rect_filled(accent_rect, 0.0, accent_color);

    let baseline_y = row_rect.center().y;
    let mut cursor_x = row_rect.min.x + SPACE_M + 2.0;

    // Severity icon
    ui.painter().text(
        egui::pos2(cursor_x + 7.0, baseline_y),
        egui::Align2::CENTER_CENTER,
        icon,
        egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
        accent_color,
    );
    cursor_x += 18.0;

    // Phase badge (small, right-aligned later)
    let phase_str = phase_label(diagnostic.phase);

    // Compute how much width the phase badge needs (~40 px)
    let phase_badge_w = 40.0_f32;
    let msg_max_width = (row_rect.max.x - cursor_x - SPACE_L - phase_badge_w).max(20.0);

    // Message — truncate to actual available width using egui's galley
    let msg = diagnostic.message.lines().next().unwrap_or(&diagnostic.message);
    let font_id = egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional);
    let galley = ui.painter().layout(
        msg.to_string(),
        font_id.clone(),
        TEXT_PRIMARY,
        msg_max_width,
    );

    ui.painter().galley(
        egui::pos2(cursor_x, baseline_y - galley.size().y / 2.0),
        galley,
        TEXT_PRIMARY,
    );

    // Phase badge (right side)
    ui.painter().text(
        egui::pos2(row_rect.max.x - SPACE_S, baseline_y),
        egui::Align2::RIGHT_CENTER,
        phase_str,
        egui::FontId::new(FONT_SIZE_XS, egui::FontFamily::Proportional),
        phase_color(diagnostic.phase),
    );

    // Bottom divider (subtle)
    if !is_last {
        ui.painter().line_segment(
            [
                egui::pos2(row_rect.min.x + SPACE_M, row_rect.bottom() - 0.5),
                egui::pos2(row_rect.max.x - SPACE_S, row_rect.bottom() - 0.5),
            ],
            Stroke::new(1.0, BORDER),
        );
    }

    if response.clicked() {
        let line = diagnostic.location.line.map(|l| l.saturating_sub(1))?;
        let column = diagnostic.location.column.map(|c| c.saturating_sub(1)).unwrap_or(0);
        Some(DiagnosticTarget { line, column })
    } else {
        None
    }
}

fn phase_label(phase: DiagnosticPhase) -> &'static str {
    match phase {
        DiagnosticPhase::Parse => "parse",
        DiagnosticPhase::Build => "build",
        DiagnosticPhase::Render => "render",
    }
}

fn phase_color(phase: DiagnosticPhase) -> Color32 {
    match phase {
        DiagnosticPhase::Parse => Color32::from_rgb(137, 180, 250),
        DiagnosticPhase::Build => Color32::from_rgb(180, 190, 254),
        DiagnosticPhase::Render => Color32::from_rgb(203, 166, 126),
    }
}
