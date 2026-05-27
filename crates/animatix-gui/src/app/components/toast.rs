use crate::app::design_tokens::*;
use egui::{Color32, Pos2, Rect, Vec2};
use std::time::Instant;

/// Toast severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastLevel {
    Info,
    Success,
    #[allow(dead_code)] // Available for future use
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
}

impl Toast {
    pub fn new(message: impl Into<String>, level: ToastLevel) -> Self {
        Self {
            message: message.into(),
            level,
            created_at: Instant::now(),
            duration: std::time::Duration::from_secs(3),
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Info)
    }

    pub fn success(message: impl Into<String>) -> Self {
        Self::new(message, ToastLevel::Success)
    }

    #[allow(dead_code)] // Available for future use
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

    pub fn color(&self) -> Color32 {
        match self.level {
            ToastLevel::Info => ACCENT_BLUE,
            ToastLevel::Success => Color32::from_rgb(76, 175, 80),
            ToastLevel::Warning => AMBER,
            ToastLevel::Error => RED,
        }
    }
}

/// Queue of pending toasts.
#[derive(Debug, Default)]
pub struct ToastQueue {
    toasts: Vec<Toast>,
}

impl ToastQueue {
    pub fn push(&mut self, toast: Toast) {
        self.toasts.push(toast);
    }

    /// Remove expired toasts and render the rest.
    pub fn show(&mut self, ui: &mut egui::Ui, now: Instant) {
        self.toasts.retain(|t| !t.is_expired(now));

        if self.toasts.is_empty() {
            return;
        }

        let viewport = ui.max_rect();
        let toast_w = 280.0;
        let toast_h = 40.0;
        let spacing = 8.0;
        let margin = 16.0;

        // Stack from bottom-right
        let start_x = viewport.max.x - margin - toast_w;
        let start_y = viewport.max.y - margin;

        for (i, toast) in self.toasts.iter().enumerate() {
            let alpha = toast.alpha(now);
            if alpha <= 0.01 {
                continue;
            }

            let y = start_y - (i as f32 + 1.0) * (toast_h + spacing);
            let rect = Rect::from_min_size(Pos2::new(start_x, y), Vec2::new(toast_w, toast_h));

            // Background with alpha
            let bg = BG_SURFACE.linear_multiply(alpha);
            ui.painter().rect_filled(rect, RADIUS_M as u8, bg);
            ui.painter().rect_stroke(
                rect,
                RADIUS_M as u8,
                egui::Stroke::new(1.0, BORDER.linear_multiply(alpha)),
                egui::StrokeKind::Outside,
            );

            // Left accent bar
            let accent_rect = Rect::from_min_size(rect.min, Vec2::new(3.0, toast_h));
            let accent_color = toast.color().linear_multiply(alpha);
            ui.painter().rect_filled(accent_rect, RADIUS_S, accent_color);

            // Icon
            let icon_x = rect.min.x + 16.0;
            let icon_color = toast.color().linear_multiply(alpha);
            ui.painter().text(
                Pos2::new(icon_x, rect.center().y),
                egui::Align2::CENTER_CENTER,
                toast.icon(),
                egui::FontId::new(FONT_SIZE_M, egui::FontFamily::Proportional),
                icon_color,
            );

            // Message
            let text_x = icon_x + 16.0;
            let text_color = TEXT_PRIMARY.linear_multiply(alpha);
            ui.painter().text(
                Pos2::new(text_x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                &toast.message,
                egui::FontId::new(FONT_SIZE_S, egui::FontFamily::Proportional),
                text_color,
            );
        }

        // Request repaint while toasts are visible for fade animation
        if !self.toasts.is_empty() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
    }
}
