//! Standalone tooltip / hover-card widget with configurable open delay and close grace.
//!
//! Shows rich/custom content anchored to a trigger `Response`.  The open delay prevents
//! tooltips from flashing on quick hovers; the close grace period keeps the tooltip visible
//! while the pointer moves between the trigger and the content (or briefly passes over empty
//! space).
//!
//! State is kept in `egui::Memory` keyed by an `Id` derived from the trigger, following the
//! immediate-mode state contract.
//!
//! # Usage
//!
//! ```ignore
//! let response = ui.button("Hover me");
//! Tooltip::new(ui.id().with("my_tooltip"))
//!     .open_delay(400.0)
//!     .close_delay(150.0)
//!     .show(ui, &response, |ui| {
//!         ui.label("Rich tooltip content here");
//!     });
//! ```
//!
//! Simple text tooltip:
//!
//! ```ignore
//! let response = ui.button("Hover me");
//! text_tooltip(ui, ui.id().with("my_tooltip"), &response, "Hello, world!");
//! ```

use crate::tokens::theme::theme;
use crate::tokens::spatial::{RADIUS_M, SPACE_2, SPACE_M, STROKE_WIDTH};
use egui::{Area, CornerRadius, Id, Margin, Order, Response, Stroke, Ui, Vec2};

use std::time::Duration;

// ─── Constants ──────────────────────────────────────────────────────────────

const DEFAULT_OPEN_DELAY_MS: f64 = 400.0;
const DEFAULT_CLOSE_DELAY_MS: f64 = 150.0;

// ─── State ─────────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct State {
    hover_start: Option<f64>,
    grace_until: Option<f64>,
    visible_last_frame: bool,
}

// ─── Builder ───────────────────────────────────────────────────────────────

/// Builder for a configurable tooltip / hover-card.
///
/// Create with [`Tooltip::new`], chain `.open_delay()` / `.close_delay()` as needed,
/// then call `.show()`.
#[derive(Debug, Clone, Copy)]
pub struct Tooltip {
    id: Id,
    open_delay: f64,
    close_delay: f64,
}

impl Tooltip {
    /// Create a new tooltip builder.
    ///
    /// `id` must be unique per tooltip instance (typically `ui.id().with("tooltip")`).
    pub fn new(id: Id) -> Self {
        Self {
            id,
            open_delay: DEFAULT_OPEN_DELAY_MS,
            close_delay: DEFAULT_CLOSE_DELAY_MS,
        }
    }

    /// Set the open delay in milliseconds.
    ///
    /// The pointer must hover the trigger for at least this long before the tooltip appears.
    /// Default is 400ms.
    pub fn open_delay(mut self, delay_ms: f64) -> Self {
        self.open_delay = delay_ms;
        self
    }

    /// Set the close grace period in milliseconds.
    ///
    /// After the pointer leaves both the trigger and the tooltip, the tooltip remains
    /// visible for this long before closing. Default is 150ms.
    pub fn close_delay(mut self, delay_ms: f64) -> Self {
        self.close_delay = delay_ms;
        self
    }

    /// Show the tooltip anchored to `trigger`.
    ///
    /// The tooltip is placed below the trigger by default and clamped to the viewport.
    pub fn show(self, ui: &Ui, trigger: &Response, add_contents: impl FnOnce(&mut Ui)) {
        let ctx = ui.ctx();
        let now = ui.input(|i| i.time);
        let open_delay_secs = self.open_delay / 1000.0;
        let close_delay_secs = self.close_delay / 1000.0;
        let trigger_hovered = trigger.hovered();

        if trigger.rect.is_negative() {
            return;
        }

        let mut state: State = ctx
            .data(|d| d.get_temp::<State>(self.id))
            .unwrap_or_default();

        // ── Hover tracking ────────────────────────────────────────────────
        if trigger_hovered {
            state.hover_start = Some(state.hover_start.unwrap_or(now));
            state.grace_until = None;
        } else {
            state.hover_start = None;
        }

        // ── Grace period ──────────────────────────────────────────────────
        // Start grace if we were visible, nothing is hovered, and grace isn't already set.
        if !trigger_hovered && state.visible_last_frame && state.grace_until.is_none() {
            state.grace_until = Some(now + close_delay_secs);
        }

        // ── Open decision ─────────────────────────────────────────────────
        let will_open = if state.visible_last_frame {
            // Already open: keep open when trigger is hovered (no re-delay).
            trigger_hovered
        } else {
            // Closed: require full open delay.
            trigger_hovered
                && state
                    .hover_start
                    .map(|t| now - t >= open_delay_secs)
                    .unwrap_or(false)
        };

        let in_grace = state.grace_until.map(|t| now <= t).unwrap_or(false);
        let prelim_open = will_open || in_grace;

        // ── Render ────────────────────────────────────────────────────────
        let mut tooltip_hovered = false;
        if prelim_open {
            let t = theme(ui);
            let screen_rect = ctx.viewport_rect();
            let gap = SPACE_2;
            let mut desired_pos = trigger.rect.left_bottom() + Vec2::new(0.0, gap);

            // Rough horizontal clamp so we don't spawn off-screen.
            let est_min_width = 40.0;
            let est_max_width = (screen_rect.width() * 0.35).min(320.0).max(est_min_width);
            desired_pos.x = desired_pos
                .x
                .max(screen_rect.min.x + SPACE_2)
                .min(screen_rect.max.x - est_max_width - SPACE_2);
            desired_pos.y = desired_pos.y.min(screen_rect.max.y - SPACE_2);

            let area = Area::new(self.id.with("area"))
                .order(Order::Tooltip)
                .fixed_pos(desired_pos);

            let inner = area.show(ctx, |ui| {
                ui.set_max_width(est_max_width);
                egui::Frame::new()
                    .fill(t.overlay.tooltip_bg)
                    .stroke(Stroke::new(STROKE_WIDTH, t.border.default))
                    .corner_radius(CornerRadius::same(RADIUS_M as u8))
                    .inner_margin(Margin::same(SPACE_M as i8))
                    .show(ui, add_contents);
            });

            tooltip_hovered = inner.response.hovered();
        }

        let is_open = tooltip_hovered || prelim_open;

        if tooltip_hovered {
            state.grace_until = None;
        }

        let was_visible = state.visible_last_frame;
        state.visible_last_frame = is_open;

        ctx.data_mut(|d| {
            d.insert_temp::<State>(self.id, state);
        });

        // Keep repainting while a timer is pending so delays and grace resolve.
        if is_open || was_visible {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

// ─── Convenience ────────────────────────────────────────────────────────────

/// Render a simple themed text tooltip anchored to `trigger`.
pub fn text_tooltip(ui: &Ui, id: Id, trigger: &Response, text: &str) {
    Tooltip::new(id).show(ui, trigger, |ui| {
        ui.label(text);
    });
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_defaults() {
        let t = Tooltip::new(Id::new("test"));
        assert_eq!(t.open_delay, DEFAULT_OPEN_DELAY_MS);
        assert_eq!(t.close_delay, DEFAULT_CLOSE_DELAY_MS);
    }

    #[test]
    fn builder_overrides() {
        let t = Tooltip::new(Id::new("test"))
            .open_delay(100.0)
            .close_delay(50.0);
        assert_eq!(t.open_delay, 100.0);
        assert_eq!(t.close_delay, 50.0);
    }
}
