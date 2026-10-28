use egui::{Color32, Response, Sense, Vec2, WidgetInfo, WidgetType};

use crate::tokens::spatial::{RADIUS_S, ROW_M, SPACE_3, STROKE_WIDTH};
use crate::tokens::theme;
use crate::tokens::typography::TextRole;
use crate::widget::spinner::Spinner;
use crate::widget::tooltip::text_tooltip;
use crate::widget::traits::{Sizable, Size};
use crate::{density, spatial};

// ── Button types ──
// eparts principle 3: default arrow cursor for buttons, pointer only for links.
// egui's Sense::click() would otherwise show a PointingHand on hover — overridden below.

/// Content alignment within a button when allocated more width than its natural size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ButtonAlign {
    /// Center content (icon + label) horizontally (default).
    #[default]
    Center,
    /// Align content to the left padding.
    Left,
}

/// Variant of button behavior and appearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ButtonVariant {
    /// Filled accent background; for primary actions.
    Primary,
    /// Filled neutral background; for secondary actions alongside a primary one.
    Secondary,
    /// Transparent background, accent underline when active; for toolbar toggles.
    Ghost,
    /// Square icon-only button; for small icon commands.
    Icon,
    /// Destructive-action button (delete, remove, …).
    Danger,
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
    size: Size,
    icon: Option<&'static str>,
    label: Option<String>,
    tooltip: &'static str,
    disabled: bool,
    active: bool,
    align: ButtonAlign,
    icon_color: Option<Color32>,
    hover_icon_color: Option<Color32>,
    on_hover: Option<Box<dyn FnOnce()>>,
    loading: bool,
}

impl Button {
    fn new_base() -> Self {
        Self {
            variant: ButtonVariant::Primary,
            size: Size::Md,
            icon: None,
            label: None,
            tooltip: "",
            disabled: false,
            active: false,
            align: ButtonAlign::Center,
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

    /// Create a Secondary variant button (filled neutral) with the given label.
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
            ..Self::new_base()
        }
    }

    /// Create a Danger variant button (destructive action) with the given label.
    pub fn danger(label: impl Into<String>) -> Self {
        Self {
            variant: ButtonVariant::Danger,
            label: Some(label.into()),
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

    /// Disable the button: it renders in the disabled slot and ignores clicks.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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

    /// Set the button size.
    pub fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// Align button contents to the left padding (useful for full-width menu/palette items).
    pub fn left_aligned(mut self) -> Self {
        self.align = ButtonAlign::Left;
        self
    }

    /// Set horizontal alignment of the button contents.
    pub fn with_align(mut self, align: ButtonAlign) -> Self {
        self.align = align;
        self
    }
}

impl Sizable for Button {
    fn with_size(self, size: Size) -> Self {
        self.with_size(size)
    }
}

/// Normalize a button label: an empty or whitespace-only label is treated as
/// `None` so icon-only buttons don't reserve blank label space (which would
/// push the icon off-center).
fn effective_label(label: Option<&str>) -> Option<&str> {
    label.filter(|l| !l.trim().is_empty())
}

/// Apply the default arrow cursor and, when a tooltip is set, render it with
/// the grace-period eparts `Tooltip` instead of egui's immediate hover text.
fn finish_response(ui: &mut egui::Ui, response: Response, tooltip: &str) -> Response {
    let response = response.on_hover_cursor(egui::CursorIcon::Default);
    if !tooltip.is_empty() {
        text_tooltip(ui, response.id.with("tooltip"), &response, tooltip);
    }
    response
}

impl ButtonVariant {
    /// The slot group that paints this variant.
    fn slots(self, slots: &theme::ButtonSlots) -> &theme::ButtonStateSlots {
        match self {
            Self::Primary => &slots.primary,
            Self::Secondary => &slots.secondary,
            Self::Ghost => &slots.ghost,
            Self::Icon => &slots.icon,
            Self::Danger => &slots.danger,
        }
    }
}

/// Resolve the state slot a variant paints with.
///
/// A ghost button's `active` is a persistent toggle (`selected` plus the
/// underline); every other variant treats `active` and pointer-down alike.
/// Loading freezes the disabled slot, so a spinner never paints over a live
/// hover state.
fn state_slot(
    group: &theme::ButtonStateSlots,
    variant: ButtonVariant,
    loading: bool,
    disabled: bool,
    active: bool,
    pressed: bool,
    hovered: bool,
) -> &theme::Slot {
    if loading || disabled {
        &group.disabled
    } else if active {
        if matches!(variant, ButtonVariant::Ghost) {
            &group.selected
        } else {
            &group.active
        }
    } else if pressed {
        &group.active
    } else if hovered {
        &group.hover
    } else {
        &group.normal
    }
}

/// Measured icon + label content of a button, and the metrics layout needs.
struct Content {
    icon_width: f32,
    gap: f32,
    label_galley: Option<std::sync::Arc<egui::Galley>>,
    /// Icon + gap + label, without padding.
    width: f32,
    /// Button width for this content, including horizontal padding.
    natural_width: f32,
    pad_x: f32,
}

/// Lay the label out and measure the content once. The icon-only variant is
/// square, so its `natural_width` is unused.
fn content_metrics(
    ui: &egui::Ui,
    t: theme::Theme,
    label: Option<&str>,
    icon_width: f32,
    pad_x: f32,
    row_height: f32,
) -> Content {
    let gap = if icon_width > 0.0 && label.is_some() {
        SPACE_3
    } else {
        0.0
    };
    let label_galley = label.map(|l| {
        ui.painter().layout_no_wrap(
            l.to_string(),
            TextRole::BodyS.font_id(),
            t.palette.text.primary,
        )
    });
    let label_width = label_galley.as_ref().map_or(0.0, |g| g.size().x);
    let width = if icon_width > 0.0 {
        icon_width + gap + label_width
    } else {
        label_width
    };
    let natural_width = if label.is_none() && icon_width > 0.0 {
        row_height.max(icon_width + pad_x * 2.0)
    } else {
        width + pad_x * 2.0
    };
    Content {
        icon_width,
        gap,
        label_galley,
        width,
        natural_width,
        pad_x,
    }
}

impl Button {
    /// Outline the button while it has keyboard focus and, while loading, put a
    /// spinner over its content. The button keeps its size in every state, so
    /// the geometry does not jump.
    fn paint_focus_and_loading(
        &self,
        ui: &mut egui::Ui,
        painter: &egui::Painter,
        rect: egui::Rect,
        radius: f32,
        row_height: f32,
        response: &Response,
        focus_color: Color32,
    ) {
        if !self.loading && !self.disabled && response.has_focus() {
            painter.rect_stroke(
                rect.shrink(1.0),
                radius,
                egui::Stroke::new(STROKE_WIDTH, focus_color),
                egui::StrokeKind::Inside,
            );
        }
        if self.loading {
            let spinner_size = row_height * 0.6;
            let spinner_rect =
                egui::Rect::from_center_size(rect.center(), Vec2::splat(spinner_size));
            ui.put(spinner_rect, Spinner::new().set_size(spinner_size));
        }
    }
}

impl egui::Widget for Button {
    fn ui(mut self, ui: &mut egui::Ui) -> Response {
        let d = density(ui);
        let row_height = self.size.row_height_for(d);
        let radius = self.size.radius();
        let _s = spatial(ui);

        let t = theme::theme(ui);

        // An empty/whitespace label is treated as "no label" so icon-only ghost/
        // primary buttons (e.g. `Button::ghost("").with_icon(..)`) center the icon
        // instead of reserving phantom space for a blank label.
        let label: Option<&str> = effective_label(self.label.as_deref());
        let accessible_label = match (label, self.tooltip) {
            (Some(l), _) => l.to_string(),
            (None, tip) if !tip.is_empty() => tip.to_string(),
            _ => "Button".to_string(),
        };

        let icon_font = TextRole::Body.font_id();
        let icon_width = self.icon.map_or(0.0, |i| {
            ui.painter()
                .layout_no_wrap(i.to_string(), icon_font.clone(), t.palette.text.primary)
                .size()
                .x
        });
        let content = content_metrics(ui, t, label, icon_width, self.size.pad_x_for(d), row_height);

        // The icon variant is square; the labelled variants allocate their
        // measured content plus padding.
        let icon_only = matches!(self.variant, ButtonVariant::Icon);
        let size = if icon_only {
            Vec2::splat(row_height)
        } else {
            Vec2::new(content.natural_width, row_height)
        };
        let sense = if self.loading {
            Sense::hover()
        } else {
            Sense::click()
        };
        let (rect, response) = ui.allocate_exact_size(size, sense);

        let pressed = response.is_pointer_button_down_on();
        let slot = state_slot(
            self.variant.slots(&t.components.button),
            self.variant,
            self.loading,
            self.disabled,
            self.active,
            pressed,
            response.hovered(),
        );
        let painter = ui.painter().with_clip_rect(rect);

        // Variant chrome. Ghost and icon rest on a transparent slot, so they only
        // fill when the current state (hover/selected) has a background; the
        // filled variants always paint and add the one-pixel top rim that reads
        // as raised.
        match self.variant {
            ButtonVariant::Icon | ButtonVariant::Ghost => {
                if slot.bg != Color32::TRANSPARENT {
                    painter.rect_filled(rect, radius, slot.bg);
                }
            },
            ButtonVariant::Primary | ButtonVariant::Danger | ButtonVariant::Secondary => {
                painter.rect_filled(rect, radius, slot.bg);
                // Subtle top rim highlight for tactile elevated chassis
                if !pressed && !self.disabled && radius >= 1.0 {
                    let top_edge = egui::Rect::from_min_max(
                        egui::pos2(rect.min.x + radius * 0.5, rect.min.y),
                        egui::pos2(rect.max.x - radius * 0.5, rect.min.y + 1.0),
                    );
                    painter.rect_filled(top_edge, 0.5, t.palette.border.rim_highlight);
                }
            },
        }
        if matches!(self.variant, ButtonVariant::Ghost)
            && self.active
            && !self.loading
            && !self.disabled
        {
            let accent_rect = egui::Rect::from_min_size(
                egui::pos2(rect.min.x + 4.0, rect.max.y - 2.0),
                Vec2::new(rect.width() - 8.0, 2.0),
            );
            painter.rect_filled(accent_rect, RADIUS_S, slot.border);
        }

        // Icon + label. A pressed button sinks its content by one pixel; the
        // icon-only variant centers it, the labelled ones lay out from the
        // alignment edge.
        if !self.loading {
            let y_shift = if pressed { 1.0 } else { 0.0 };
            let baseline_y = rect.center().y + y_shift;
            let start_x = if icon_only {
                rect.min.x + (rect.width() - content.width) * 0.5
            } else {
                match self.align {
                    ButtonAlign::Center => {
                        rect.min.x + ((rect.width() - content.width) * 0.5).max(content.pad_x)
                    },
                    ButtonAlign::Left => rect.min.x + content.pad_x,
                }
            };
            // A ghost toggle keeps its active foreground rather than the hover
            // accent, so it does not flash while toggled on; the filled variants
            // let the hover accent override.
            let toggled = matches!(self.variant, ButtonVariant::Ghost) && self.active;
            let icon_color = if self.disabled || toggled {
                slot.fg
            } else if response.hovered() {
                self.hover_icon_color.unwrap_or(slot.fg)
            } else {
                self.icon_color.unwrap_or(slot.fg)
            };
            let mut cursor_x = start_x;
            if let Some(icon) = self.icon {
                let icon_y = if label.is_some() {
                    baseline_y + 1.0
                } else {
                    baseline_y
                };
                painter.text(
                    egui::pos2(cursor_x + content.icon_width / 2.0, icon_y),
                    egui::Align2::CENTER_CENTER,
                    icon,
                    icon_font,
                    icon_color,
                );
                cursor_x += content.icon_width + content.gap;
            }
            if let Some(galley) = &content.label_galley {
                painter.galley(
                    egui::pos2(cursor_x, baseline_y - galley.size().y / 2.0),
                    galley.clone(),
                    slot.fg,
                );
            }
        }

        self.paint_focus_and_loading(
            ui,
            &painter,
            rect,
            radius,
            row_height,
            &response,
            t.focus_ring(),
        );

        // Principle 3: override egui's default PointingHand with Default arrow.
        let response = finish_response(ui, response, self.tooltip);
        if response.hovered() {
            if let Some(cb) = self.on_hover.take() {
                cb();
            }
        }
        response.widget_info(|| {
            WidgetInfo::labeled(WidgetType::Button, ui.is_enabled(), accessible_label.clone())
        });
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
        egui::Stroke::new(STROKE_WIDTH, t.palette.border.default),
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

    #[test]
    fn danger_constructor() {
        let b = Button::danger("Delete");
        assert_eq!(b.variant, ButtonVariant::Danger);
        assert_eq!(b.label, Some("Delete".to_string()));
    }

    #[test]
    fn size_setter() {
        let b = Button::primary("Save").sm();
        assert_eq!(b.size, Size::Sm);
    }

    #[test]
    fn size_default_is_md() {
        let b = Button::primary("Save");
        assert_eq!(b.size, Size::Md);
    }

    #[test]
    fn align_defaults_to_center_and_supports_left() {
        let b = Button::primary("Save");
        assert_eq!(b.align, ButtonAlign::Center);
        let b_left = Button::primary("Save").left_aligned();
        assert_eq!(b_left.align, ButtonAlign::Left);
        let b_custom = Button::ghost("Option").with_align(ButtonAlign::Left);
        assert_eq!(b_custom.align, ButtonAlign::Left);
    }
}
