use crate::tokens::theme::{theme, Theme};
use crate::tokens::spatial::component::{
    TOAST_HEIGHT, TOAST_MARGIN, TOAST_SPACING, TOAST_WIDTH,
};
use crate::tokens::spatial::{RADIUS_M, RADIUS_S, SPACE_2, SPACE_6, STROKE_WIDTH};
use crate::tokens::typography::TextRole;
use egui::{Color32, Pos2, Rect, Vec2};
use std::time::Instant;

/// Toast severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// A single toast notification.
#[derive(Debug, Clone)]
pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    pub created_at: Instant,
    pub duration: std::time::Duration,
    pub count: u32,
}

impl Toast {
    pub fn new(message: impl Into<String>, level: ToastLevel) -> Self {
        Self {
            message: message.into(),
            level,
            created_at: Instant::now(),
            duration: std::time::Duration::from_secs(3),
            count: 1,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Info)
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Success)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Warning)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Error)
    }

    /// Returns the fade alpha (0.0 to 1.0) based on elapsed time.
    pub fn alpha(&self, now: Instant) -> f32 {
        let elapsed = now.duration_since(self.created_at).as_secs_f32();
        let total = self.duration.as_secs_f32();
        if elapsed < 0.15 {
            // Fade in
            elapsed / 0.15
        } else if elapsed > total - 0.3 {
            // Fade out in last 0.3s
            ((total - elapsed) / 0.3).max(0.0)
        } else {
            1.0
        }
    }

    pub fn is_expired(&self, now: Instant) -> bool {
        now.duration_since(self.created_at) > self.duration
    }

    pub fn icon(&self) -> &'static str {
        match self.level {
            ToastLevel::Info => egui_phosphor::regular::INFO,
            ToastLevel::Success => egui_phosphor::regular::CHECK_CIRCLE,
            ToastLevel::Warning => egui_phosphor::regular::WARNING,
            ToastLevel::Error => egui_phosphor::regular::X_CIRCLE,
        }
    }

    pub fn color(&self, t: &Theme) -> Color32 {
        match self.level {
            ToastLevel::Info => t.accent.primary,
            ToastLevel::Success => t.status.success,
            ToastLevel::Warning => t.status.warning,
            ToastLevel::Error => t.status.error,
        }
    }
}

/// Screen corner where toasts are anchored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToastPlacement {
    #[default]
    BottomRight,
    BottomLeft,
    TopRight,
    TopLeft,
    BottomCenter,
    TopCenter,
}

/// Queue of pending toasts.
#[derive(Debug)]
pub struct ToastQueue {
    toasts: Vec<Toast>,
    placement: ToastPlacement,
}

impl Default for ToastQueue {
    fn default() -> Self {
        Self {
            toasts: Vec::new(),
            placement: ToastPlacement::BottomRight,
        }
    }
}

impl ToastQueue {
    pub fn push(&mut self, toast: Toast) {
        // Dedup: collapse non-expired toasts with the same level and message.
        if let Some(existing) = self
            .toasts
            .iter_mut()
            .find(|t| t.level == toast.level && t.message == toast.message && !t.is_expired(Instant::now()))
        {
            existing.count += 1;
            existing.created_at = Instant::now();
            return;
        }
        self.toasts.push(toast);
    }

    /// Set the toast anchor corner.
    pub fn with_placement(mut self, placement: ToastPlacement) -> Self {
        self.placement = placement;
        self
    }

    /// Remove expired toasts and render the rest.
    pub fn show(&mut self, ui: &mut egui::Ui, now: Instant) {
        self.toasts.retain(|t| !t.is_expired(now));

        if self.toasts.is_empty() {
            return;
        }

        let theme = theme(ui);
        let viewport = ui.max_rect();
        let toast_w = TOAST_WIDTH;
        let toast_h = TOAST_HEIGHT;
        let spacing = TOAST_SPACING;
        let margin = TOAST_MARGIN;

        // Determine anchor corner and stacking direction from placement.
        let (start_x, start_y, stack_up) = match self.placement {
            ToastPlacement::BottomRight => {
                (viewport.max.x - margin - toast_w, viewport.max.y - margin, true)
            }
            ToastPlacement::BottomLeft => (margin, viewport.max.y - margin, true),
            ToastPlacement::TopRight => (viewport.max.x - margin - toast_w, margin, false),
            ToastPlacement::TopLeft => (margin, margin, false),
            ToastPlacement::BottomCenter => (
                viewport.center().x - toast_w / 2.0,
                viewport.max.y - margin,
                true,
            ),
            ToastPlacement::TopCenter => (viewport.center().x - toast_w / 2.0, margin, false),
        };

        let mut i = 0;
        while i < self.toasts.len() {
            let toast = &self.toasts[i];
            let alpha = toast.alpha(now);
            if alpha <= 0.01 {
                i += 1;
                continue;
            }

            let y = if stack_up {
                start_y - (i as f32 + 1.0) * (toast_h + spacing)
            } else {
                start_y + i as f32 * (toast_h + spacing)
            };
            let rect = Rect::from_min_size(Pos2::new(start_x, y), Vec2::new(toast_w, toast_h));

            // Make the toast clickable to dismiss
            let response = ui.interact(rect, ui.id().with("toast").with(i), egui::Sense::click());
            if response.clicked() {
                self.toasts.remove(i);
                continue;
            }

            // Background with alpha
            let bg = theme.surface.surface.linear_multiply(alpha);
            ui.painter().rect_filled(rect, RADIUS_M as u8, bg);
            ui.painter().rect_stroke(
                rect,
                RADIUS_M as u8,
                egui::Stroke::new(STROKE_WIDTH, theme.border.default.linear_multiply(alpha)),
                egui::StrokeKind::Outside,
            );

            // Left accent bar
            let accent_rect = Rect::from_min_size(rect.min, Vec2::new(SPACE_2, toast_h));
            let accent_color = toast.color(&theme).linear_multiply(alpha);
            ui.painter().rect_filled(accent_rect, RADIUS_S, accent_color);

            // Icon
            let icon_x = rect.min.x + SPACE_6;
            let icon_color = toast.color(&theme).linear_multiply(alpha);
            ui.painter().text(
                Pos2::new(icon_x, rect.center().y),
                egui::Align2::CENTER_CENTER,
                toast.icon(),
                TextRole::Body.font_id(),
                icon_color,
            );

            // Message (wrapped to toast width so it doesn't overflow)
            let text_x = icon_x + SPACE_6;
            let text_color = theme.text.primary.linear_multiply(alpha);
            let text_max_w = (toast_w - (text_x - rect.min.x) - SPACE_6).max(40.0);
            let display_message = if toast.count > 1 {
                format!("{} (x{})", toast.message, toast.count)
            } else {
                toast.message.clone()
            };
            let galley = ui.painter().layout(
                display_message,
                TextRole::BodyS.font_id(),
                text_color,
                text_max_w,
            );
            let text_pos = Pos2::new(text_x, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(text_pos, galley, text_color);
            i += 1;
        }

        // Request repaint while toasts are visible for fade animation
        if !self.toasts.is_empty() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_push_same_message_and_level() {
        let mut queue = ToastQueue::default();
        queue.push(Toast::info("hello"));
        queue.push(Toast::info("hello"));
        assert_eq!(queue.toasts.len(), 1);
        assert_eq!(queue.toasts[0].count, 2);
    }

    #[test]
    fn push_different_messages() {
        let mut queue = ToastQueue::default();
        queue.push(Toast::info("hello"));
        queue.push(Toast::info("world"));
        assert_eq!(queue.toasts.len(), 2);
    }

    #[test]
    fn with_placement_sets_field() {
        let mut queue = ToastQueue::default();
        queue = queue.with_placement(ToastPlacement::TopLeft);
        assert_eq!(queue.placement, ToastPlacement::TopLeft);
    }
}
