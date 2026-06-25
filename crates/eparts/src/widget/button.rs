use egui::{Color32, Response, Sense, Vec2};

use crate::tokens::semantic::{accent, border, surface, text};
use crate::tokens::spatial::{
    RADIUS_M, RADIUS_S, ROW_L, ROW_M, ROW_S, SPACE_M, STROKE_WIDTH,
};
use crate::tokens::typography::TextRole;

// ── Button types ──

/// Variant of button behavior and appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonVariant {
    /// Filled accent background; for primary actions.
    Primary,
    /// Subtle fill; for secondary actions.
    #[allow(dead_code)] // Reserved for future secondary-action buttons
    Secondary,
    /// Transparent background, accent underline when active; for toolbar toggles.
    Ghost,
    /// Square icon-only button; for small icon commands.
    Icon,
}

/// Preset sizes for buttons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonSize {
    #[allow(dead_code)] // Reserved for future compact layouts
    Small,
    Medium,
    #[allow(dead_code)] // Reserved for future prominent buttons
    Large,
}

/// A unified button widget with builder API.
///
/// ## Examples
/// ```ignore
/// ui.add(Button::icon(PLAY).with_tooltip("Play"));
/// ui.add(Button::ghost("Label").with_icon(GEAR).active(true));
/// ```
#[allow(dead_code)] // Builder methods are part of the public API; unused variants suppressed at variant-level
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
    show_label: bool,
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
            show_label: true,
        }
    }

    /// Create a Primary variant button with the given label.
    #[allow(dead_code)] // Reserved for future button call sites
    pub fn primary(label: impl Into<String>) -> Self {
        Self {
            variant: ButtonVariant::Primary,
            label: Some(label.into()),
            ..Self::new_base()
        }
    }

    /// Create a Secondary variant button with the given label.
    #[allow(dead_code)] // Reserved for future secondary-action buttons
    pub fn secondary(label: impl Into<String>) -> Self {
        Self {
            variant: ButtonVariant::Secondary,
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
            show_label: false,
            ..Self::new_base()
        }
    }

    /// Set size to Small.
    #[allow(dead_code)] // Reserved for future button call sites
    pub fn small(mut self) -> Self {
        self.size = ButtonSize::Small;
        self
    }

    /// Set size to Large.
    #[allow(dead_code)] // Reserved for future button call sites
    pub fn large(mut self) -> Self {
        self.size = ButtonSize::Large;
        self
    }

    /// Set the icon.
    pub fn with_icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Set the label (for icon-only buttons that sometimes show text).
    #[allow(dead_code)] // Reserved for future button call sites
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Set the tooltip.
    pub fn with_tooltip(mut self, tip: &'static str) -> Self {
        self.tooltip = tip;
        self
    }

    /// Set the disabled state.
    #[allow(dead_code)] // Reserved for future button call sites
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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

    /// Whether to show the label (default true for label-carrying variants, false for Icon).
    #[allow(dead_code)] // Reserved for future button call sites
    pub fn show_label(mut self, show: bool) -> Self {
        self.show_label = show;
        self
    }
}

impl egui::Widget for Button {
    fn ui(self, ui: &mut egui::Ui) -> Response {
        let (row_height, radius) = match self.size {
            ButtonSize::Small => (ROW_S, RADIUS_S),
            ButtonSize::Medium => (ROW_M, RADIUS_M),
            ButtonSize::Large => (ROW_L, RADIUS_M),
        };

        let icon_font = TextRole::Body.font_id();
        let label_font = TextRole::BodyS.font_id();

        let icon_galley = self
            .icon
            .map(|i| ui.painter().layout_no_wrap(i.to_string(), icon_font.clone(), text::PRIMARY));

        let show_label = self.label.is_some() && self.show_label;

        match self.variant {
            ButtonVariant::Icon => {
                let size = Vec2::new(row_height, row_height);
                let (rect, response) = ui.allocate_exact_size(size, Sense::click());

                if !self.disabled && (response.hovered() || response.is_pointer_button_down_on()) {
                        ui.painter().rect_filled(rect, RADIUS_M, surface::HOVER);
                }

                let icon_color = if self.disabled {
                    text::DISABLED
                } else if response.hovered() {
                    self.hover_icon_color.unwrap_or(text::PRIMARY)
                } else {
                    self.icon_color.unwrap_or(text::SECONDARY)
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

                if !self.disabled && response.has_focus() {
                    ui.painter().rect_stroke(
                        rect.shrink(1.0),
                        RADIUS_M,
                        egui::Stroke::new(STROKE_WIDTH, accent::PRIMARY),
                        egui::StrokeKind::Inside,
                    );
                }

                if !self.tooltip.is_empty() {
                    response.on_hover_text(self.tooltip)
                } else {
                    response
                }
            },
            ButtonVariant::Ghost => {
                let icon_width = icon_galley.as_ref().map_or(0.0, |g| g.size().x);
                let mut width = icon_width + SPACE_M * 2.0;
                let mut label_galley = None;
                if let Some(ref l) = self.label.filter(|_| show_label) {
                    let galley = ui.painter().layout_no_wrap(
                        format!("  {}", l),
                        label_font.clone(),
                        text::PRIMARY,
                    );
                    width += galley.size().x;
                    label_galley = Some(galley);
                }
                let size = Vec2::new(width.max(row_height), row_height);
                let (rect, response) = ui.allocate_exact_size(size, Sense::click());

                if !self.disabled {
                    let bg = if self.active {
                        surface::ACTIVE
                    } else if response.hovered() || response.is_pointer_button_down_on() {
                        surface::HOVER
                    } else {
                        Color32::TRANSPARENT
                    };
                    if bg != Color32::TRANSPARENT {
                        ui.painter().rect_filled(rect, radius, bg);
                    }

                    if self.active {
                        let accent_rect = egui::Rect::from_min_size(
                            egui::pos2(rect.min.x + 4.0, rect.max.y - 2.0),
                            Vec2::new(rect.width() - 8.0, 2.0),
                        );
                        ui.painter().rect_filled(accent_rect, RADIUS_S, accent::PRIMARY);
                    }
                }

                let icon_color = if self.disabled {
                    text::DISABLED
                } else if self.active {
                    accent::PRIMARY
                } else if response.hovered() {
                    text::PRIMARY
                } else {
                    text::SECONDARY
                };

                let mut cursor_x = rect.min.x + SPACE_M;
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
                    let label_color = if self.disabled {
                        text::DISABLED
                    } else if self.active {
                        accent::PRIMARY
                    } else if response.hovered() {
                        text::PRIMARY
                    } else {
                        text::SECONDARY
                    };
                    ui.painter().galley(
                        egui::pos2(cursor_x, baseline_y - galley.size().y / 2.0),
                        galley,
                        label_color,
                    );
                }

                if !self.disabled && response.has_focus() {
                    ui.painter().rect_stroke(
                        rect.shrink(1.0),
                        radius,
                        egui::Stroke::new(STROKE_WIDTH, accent::PRIMARY),
                        egui::StrokeKind::Inside,
                    );
                }

                if !self.tooltip.is_empty() {
                    response.on_hover_text(self.tooltip)
                } else {
                    response
                }
            },
            ButtonVariant::Primary | ButtonVariant::Secondary => {
                let icon_width = icon_galley.as_ref().map_or(0.0, |g| g.size().x);
                let mut width = SPACE_M * 2.0;
                if icon_width > 0.0 {
                    width += icon_width + SPACE_M;
                }
                let label_str = self.label.as_ref().filter(|_| show_label).cloned();
                if let Some(ref l) = label_str {
                    let galley =
                        ui.painter().layout_no_wrap(l.clone(), label_font.clone(), text::PRIMARY);
                    width += galley.size().x;
                }
                let size = Vec2::new(width.max(row_height * 2.0), row_height);
                let (rect, response) = ui.allocate_exact_size(size, Sense::click());

                if !self.disabled {
                    let (bg, hover_bg, active_bg) = match self.variant {
                        ButtonVariant::Primary => {
                            (accent::PRIMARY, accent::PRIMARY_HOVER, accent::PRIMARY_ACTIVE)
                        },
                        _ => (surface::WIDGET, surface::HOVER, surface::ACTIVE),
                    };
                    let fill = if response.is_pointer_button_down_on() {
                        active_bg
                    } else if response.hovered() {
                        hover_bg
                    } else {
                        bg
                    };
                    ui.painter().rect_filled(rect, radius, fill);
                } else {
                    ui.painter().rect_filled(rect, radius, surface::WIDGET);
                }

                let text_color = if self.disabled {
                    text::DISABLED
                } else {
                    match self.variant {
                        ButtonVariant::Primary => text::ON_ACCENT,
                        _ => text::PRIMARY,
                    }
                };

                let mut cursor_x = rect.min.x + SPACE_M;
                let baseline_y = rect.center().y;

                if icon_width > 0.0 {
                    if let Some(icon) = self.icon {
                        ui.painter().text(
                            egui::pos2(cursor_x + icon_width / 2.0, baseline_y),
                            egui::Align2::CENTER_CENTER,
                            icon,
                            icon_font,
                            text_color,
                        );
                        cursor_x += icon_width + SPACE_M;
                    }
                }

                if let Some(ref l) = label_str {
                    ui.painter().text(
                        egui::pos2(cursor_x, baseline_y),
                        egui::Align2::LEFT_CENTER,
                        l.clone(),
                        label_font,
                        text_color,
                    );
                }

                if !self.disabled && response.has_focus() {
                    ui.painter().rect_stroke(
                        rect.shrink(1.0),
                        radius,
                        egui::Stroke::new(STROKE_WIDTH, accent::PRIMARY),
                        egui::StrokeKind::Inside,
                    );
                }

                if !self.tooltip.is_empty() {
                    response.on_hover_text(self.tooltip)
                } else {
                    response
                }
            },
        }
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
    let height = ROW_M - 4.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(1.0, height), Sense::hover());
    ui.painter().line_segment(
        [
            egui::pos2(rect.center().x, rect.min.y),
            egui::pos2(rect.center().x, rect.max.y),
        ],
        egui::Stroke::new(STROKE_WIDTH, border::DEFAULT),
    );
}
