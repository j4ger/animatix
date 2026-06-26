use egui::{Color32, Response, Sense, Vec2};

use crate::tokens::spatial::{RADIUS_M, RADIUS_S, ROW_M, SPACE_3, STROKE_WIDTH};
use crate::tokens::theme;
use crate::tokens::typography::TextRole;
use crate::widget::spinner::Spinner;

// ── Button types ──
// eparts principle 3: default arrow cursor for buttons, pointer only for links.
// egui's Sense::click() would otherwise show a PointingHand on hover — overridden below.

/// Variant of button behavior and appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonVariant {
    /// Filled accent background; for primary actions.
    Primary,
    /// Transparent background, accent underline when active; for toolbar toggles.
    Ghost,
    /// Square icon-only button; for small icon commands.
    Icon,
}

/// Preset sizes for buttons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonSize {
    /// Default size.
    Medium,
}

/// A unified button widget with builder API.
///
/// ## Examples
/// ```ignore
/// ui.add(Button::icon(PLAY).with_tooltip("Play"));
/// ui.add(Button::ghost("Label").with_icon(GEAR).active(true));
/// ```
pub struct Button {
    variant: ButtonVariant,
    size: ButtonSize,
    icon: Option<&'static str>,
    label: Option<String>,
    tooltip: &'static str,
    disabled: bool,
    active: bool,
    icon_color: Option<Color32>,
    hover_icon_color: Option<Color32>,
    on_hover: Option<Box<dyn FnOnce()>>,
    loading: bool,
}

impl Button {
    fn new_base() -> Self {
        Self {
            variant: ButtonVariant::Primary,
            size: ButtonSize::Medium,
            icon: None,
            label: None,
            tooltip: "",
            disabled: false,
            active: false,
            icon_color: None,
            hover_icon_color: None,
            on_hover: None,
            loading: false,
        }
    }

    /// Create a Primary variant button (filled accent) with the given label.
    pub fn primary(label: impl Into<String>) -> Self {
        Self {
            variant: ButtonVariant::Primary,
            label: Some(label.into()),
            ..Self::new_base()
        }
    }

    /// Create a Ghost variant button with the given label.
    pub fn ghost(label: impl Into<String>) -> Self {
        Self {
            variant: ButtonVariant::Ghost,
            label: Some(label.into()),
            ..Self::new_base()
        }
    }

    /// Create an Icon variant button (square icon-only).
    pub fn icon(icon: &'static str) -> Self {
        Self {
            variant: ButtonVariant::Icon,
            icon: Some(icon),
            ..Self::new_base()
        }
    }

    /// Set the icon.
    pub fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Set the tooltip.
    pub fn with_tooltip(mut self, tip: &'static str) -> Self {
        self.tooltip = tip;
        self
    }

    /// Set the active state (for Ghost toggle buttons).
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Set a custom icon color.
    pub fn icon_color(mut self, c: Color32) -> Self {
        self.icon_color = Some(c);
        self
    }

    /// Set a custom hover icon color.
    pub fn hover_icon_color(mut self, c: Color32) -> Self {
        self.hover_icon_color = Some(c);
        self
    }

    /// Set the loading state (shows a spinner and disables interaction).
    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Set a callback invoked when the button is hovered.
    pub fn on_hover(mut self, cb: Box<dyn FnOnce()>) -> Self {
        self.on_hover = Some(cb);
        self
    }
}

/// Normalize a button label: an empty or whitespace-only label is treated as
/// `None` so icon-only buttons don't reserve blank label space (which would
/// push the icon off-center).
fn effective_label(label: Option<&str>) -> Option<&str> {
    label.filter(|l| !l.trim().is_empty())
}

impl egui::Widget for Button {
    fn ui(mut self, ui: &mut egui::Ui) -> Response {
        let (row_height, radius) = match self.size {
            ButtonSize::Medium => (ROW_M, RADIUS_M),
        };

        let t = theme::theme(ui);

        // An empty/whitespace label is treated as "no label" so icon-only ghost/
        // primary buttons (e.g. `Button::ghost("").with_icon(..)`) center the icon
        // instead of reserving phantom space for a blank label.
        let label: Option<&str> = effective_label(self.label.as_deref());

        let icon_font = TextRole::Body.font_id();
        let label_font = TextRole::BodyS.font_id();

        let icon_galley = self
            .icon
            .map(|i| ui.painter().layout_no_wrap(i.to_string(), icon_font.clone(), t.text.primary));

        let response = match self.variant {
            ButtonVariant::Icon => {
                let size = Vec2::new(row_height, row_height);
                let sense = if self.loading { Sense::hover() } else { Sense::click() };
                let (rect, response) = ui.allocate_exact_size(size, sense);

                let slot_group = &t.button.icon;
                let slot = if self.loading || self.disabled {
                    &slot_group.disabled
                } else if self.active || response.is_pointer_button_down_on() {
                    &slot_group.active
                } else if response.hovered() {
                    &slot_group.hover
                } else {
                    &slot_group.normal
                };

                if slot.bg != Color32::TRANSPARENT {
                    ui.painter().rect_filled(rect, RADIUS_M, slot.bg);
                }

                if !self.loading {
                    let icon_color = if self.disabled {
                        slot.fg
                    } else if response.hovered() {
                        self.hover_icon_color.unwrap_or(slot.fg)
                    } else {
                        self.icon_color.unwrap_or(slot.fg)
                    };

                    if let Some(i) = self.icon {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            i,
                            icon_font,
                            icon_color,
                        );
                    }
                }

                if !self.loading && !self.disabled && response.has_focus() {
                    ui.painter().rect_stroke(
                        rect.shrink(1.0),
                        RADIUS_M,
                        egui::Stroke::new(STROKE_WIDTH, t.focus_ring()),
                        egui::StrokeKind::Inside,
                    );
                }

                if self.loading {
                    let spinner_size = row_height * 0.6;
                    let spinner_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(spinner_size));
                    ui.put(spinner_rect, Spinner::new().set_size(spinner_size));
                }

                // Principle 3: override egui's default PointingHand with Default arrow.
                if !self.tooltip.is_empty() {
                    response.on_hover_cursor(egui::CursorIcon::Default)
                        .on_hover_text(self.tooltip)
                } else {
                    response.on_hover_cursor(egui::CursorIcon::Default)
                }
            },
            ButtonVariant::Ghost => {
                let icon_width = icon_galley.as_ref().map_or(0.0, |g| g.size().x);
                let mut width = icon_width + SPACE_3 * 2.0;
                let mut label_galley = None;
                if let Some(l) = label {
                    let galley = ui.painter().layout_no_wrap(
                        format!("  {}", l),
                        label_font.clone(),
                        t.text.primary,
                    );
                    width += galley.size().x;
                    label_galley = Some(galley);
                }
                let size = Vec2::new(width.max(row_height), row_height);
                let sense = if self.loading { Sense::hover() } else { Sense::click() };
                let (rect, response) = ui.allocate_exact_size(size, sense);

                let slot_group = &t.button.ghost;
                let slot = if self.loading || self.disabled {
                    &slot_group.disabled
                } else if self.active {
                    &slot_group.selected
                } else if response.is_pointer_button_down_on() {
                    &slot_group.active
                } else if response.hovered() {
                    &slot_group.hover
                } else {
                    &slot_group.normal
                };

                if slot.bg != Color32::TRANSPARENT {
                    ui.painter().rect_filled(rect, radius, slot.bg);
                }

                if self.active && !self.loading && !self.disabled {
                    let accent_rect = egui::Rect::from_min_size(
                        egui::pos2(rect.min.x + 4.0, rect.max.y - 2.0),
                        Vec2::new(rect.width() - 8.0, 2.0),
                    );
                    ui.painter().rect_filled(accent_rect, RADIUS_S, slot.border);
                }

                if !self.loading {
                    let icon_color = if self.disabled || self.active {
                        slot.fg
                    } else if response.hovered() {
                        self.hover_icon_color.unwrap_or(slot.fg)
                    } else {
                        self.icon_color.unwrap_or(slot.fg)
                    };

                    let mut cursor_x = rect.min.x + SPACE_3;
                    let baseline_y = rect.center().y;

                    if let Some(icon) = self.icon {
                        ui.painter().text(
                            egui::pos2(cursor_x + icon_width / 2.0, baseline_y),
                            egui::Align2::CENTER_CENTER,
                            icon,
                            icon_font,
                            icon_color,
                        );
                        cursor_x += icon_width;
                    }

                    if let Some(galley) = label_galley {
                        ui.painter().galley(
                            egui::pos2(cursor_x, baseline_y - galley.size().y / 2.0),
                            galley,
                            slot.fg,
                        );
                    }
                }

                if !self.loading && !self.disabled && response.has_focus() {
                    ui.painter().rect_stroke(
                        rect.shrink(1.0),
                        radius,
                        egui::Stroke::new(STROKE_WIDTH, t.focus_ring()),
                        egui::StrokeKind::Inside,
                    );
                }

                if self.loading {
                    let spinner_size = row_height * 0.6;
                    let spinner_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(spinner_size));
                    ui.put(spinner_rect, Spinner::new().set_size(spinner_size));
                }

                // Principle 3: override egui's default PointingHand with Default arrow.
                if !self.tooltip.is_empty() {
                    response.on_hover_cursor(egui::CursorIcon::Default)
                        .on_hover_text(self.tooltip)
                } else {
                    response.on_hover_cursor(egui::CursorIcon::Default)
                }
            },
            ButtonVariant::Primary => {
                let icon_width = icon_galley.as_ref().map_or(0.0, |g| g.size().x);
                let mut width = SPACE_3 * 2.0;
                if icon_width > 0.0 {
                    width += icon_width + SPACE_3;
                }
                let label_str = label;
                if let Some(l) = label_str {
                    let galley =
                        ui.painter().layout_no_wrap(l.to_string(), label_font.clone(), t.text.primary);
                    width += galley.size().x;
                }
                let size = Vec2::new(width.max(row_height), row_height);
                let sense = if self.loading { Sense::hover() } else { Sense::click() };
                let (rect, response) = ui.allocate_exact_size(size, sense);

                let slot_group = &t.button.primary;
                let slot = if self.loading || self.disabled {
                    &slot_group.disabled
                } else if self.active || response.is_pointer_button_down_on() {
                    &slot_group.active
                } else if response.hovered() {
                    &slot_group.hover
                } else {
                    &slot_group.normal
                };

                ui.painter().rect_filled(rect, radius, slot.bg);

                if !self.loading {
                    let mut cursor_x = rect.min.x + SPACE_3;
                    let baseline_y = rect.center().y;

                    if icon_width > 0.0 {
                        if let Some(icon) = self.icon {
                            let icon_fg = if self.disabled {
                                slot.fg
                            } else if response.hovered() {
                                self.hover_icon_color.unwrap_or(slot.fg)
                            } else {
                                self.icon_color.unwrap_or(slot.fg)
                            };
                            ui.painter().text(
                                egui::pos2(cursor_x + icon_width / 2.0, baseline_y),
                                egui::Align2::CENTER_CENTER,
                                icon,
                                icon_font,
                                icon_fg,
                            );
                            cursor_x += icon_width + SPACE_3;
                        }
                    }

                    if let Some(l) = label_str {
                        ui.painter().text(
                            egui::pos2(cursor_x, baseline_y),
                            egui::Align2::LEFT_CENTER,
                            l.to_string(),
                            label_font,
                            slot.fg,
                        );
                    }
                }

                if !self.loading && !self.disabled && response.has_focus() {
                    ui.painter().rect_stroke(
                        rect.shrink(1.0),
                        radius,
                        egui::Stroke::new(STROKE_WIDTH, t.focus_ring()),
                        egui::StrokeKind::Inside,
                    );
                }

                if self.loading {
                    let spinner_size = row_height * 0.6;
                    let spinner_rect = egui::Rect::from_center_size(rect.center(), Vec2::splat(spinner_size));
                    ui.put(spinner_rect, Spinner::new().set_size(spinner_size));
                }

                // Principle 3: override egui's default PointingHand with Default arrow.
                if !self.tooltip.is_empty() {
                    response.on_hover_cursor(egui::CursorIcon::Default)
                        .on_hover_text(self.tooltip)
                } else {
                    response.on_hover_cursor(egui::CursorIcon::Default)
                }
            },
        };
        if response.hovered() {
            if let Some(cb) = self.on_hover.take() {
                cb();
            }
        }
        response
    }
}

/// Returns the play/pause icon character based on playback state.
pub fn play_pause_icon(is_playing: bool) -> &'static str {
    if is_playing {
        egui_phosphor::regular::PAUSE
    } else {
        egui_phosphor::regular::PLAY
    }
}

/// A small vertical separator for toolbar button groups.
pub fn toolbar_separator(ui: &mut egui::Ui) {
    let t = theme::theme(ui);
    let height = ROW_M - 4.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, height), Sense::hover());
    ui.painter().line_segment(
        [
            egui::pos2(rect.center().x, rect.min.y),
            egui::pos2(rect.center().x, rect.max.y),
        ],
        egui::Stroke::new(STROKE_WIDTH, t.border.default),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_builder_defaults() {
        let b = Button::icon("★");
        assert_eq!(b.variant, ButtonVariant::Icon);
        assert_eq!(b.icon, Some("★"));
        assert!(!b.loading);
        assert!(!b.active);
        assert!(b.on_hover.is_none());
    }

    #[test]
    fn ghost_builder() {
        let b = Button::ghost("Hello").with_icon("★").active(true);
        assert_eq!(b.variant, ButtonVariant::Ghost);
        assert_eq!(b.label, Some("Hello".to_string()));
        assert_eq!(b.icon, Some("★"));
        assert!(b.active);
    }

    #[test]
    fn loading_flag() {
        let b = Button::icon("★").loading(true);
        assert!(b.loading);
    }

    #[test]
    fn active_flag() {
        let b = Button::ghost("X").active(true);
        assert_eq!(b.label, Some("X".to_string()));
        assert!(b.active);
    }

    #[test]
    fn on_hover_callback() {
        let b = Button::icon("★").on_hover(Box::new(|| {}));
        assert!(b.on_hover.is_some());
    }

    #[test]
    fn effective_label_treats_blank_as_none() {
        // Empty / whitespace labels must collapse to None so icon-only ghost
        // buttons (`Button::ghost("").with_icon(..)`) center their icon.
        assert_eq!(effective_label(None), None);
        assert_eq!(effective_label(Some("")), None);
        assert_eq!(effective_label(Some("   ")), None);
        assert_eq!(effective_label(Some("Hi")), Some("Hi"));
    }
}
