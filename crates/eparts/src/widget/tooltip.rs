//! Standalone tooltip / hover-card with configurable open delay and close grace:
//! the delay prevents flashing on quick hovers, and the grace keeps the tooltip
//! visible while the pointer moves between the trigger and the content.
//!
//! State is kept in `egui::Memory`, keyed by an `Id` derived from the trigger.
//!
//! ```ignore
//! Tooltip::new(ui.id().with("my_tooltip"))
//!     .open_delay(400.0)
//!     .show(ui, &response, |ui| { ui.label("Rich content"); });
//! ```

use std::time::Duration;

use egui::{Area, CornerRadius, Id, Margin, Order, Response, Stroke, Ui, Vec2};

use crate::tokens::spatial::{RADIUS_M, STROKE_WIDTH};
use crate::tokens::theme::theme;

const DEFAULT_OPEN_DELAY_MS: f64 = 400.0;
const DEFAULT_CLOSE_DELAY_MS: f64 = 150.0;

#[derive(Clone, Default)]
struct State {
    hover_start: Option<f64>,
    grace_until: Option<f64>,
    visible_last_frame: bool,
}

/// Builder for a configurable tooltip. Create with [`Tooltip::new`], chain the
/// delays, then call `.show()`.
#[derive(Debug, Clone, Copy)]
pub struct Tooltip {
    id: Id,
    open_delay: f64,
    close_delay: f64,
}

impl Tooltip {
    /// Create a tooltip builder. `id` namespaces transient state and is combined
    /// with the trigger's response id, so one `id` may serve several triggers
    /// with independent hover/grace state. Typically `ui.id().with("tooltip")`.
    pub fn new(id: Id) -> Self {
        Self {
            id,
            open_delay: DEFAULT_OPEN_DELAY_MS,
            close_delay: DEFAULT_CLOSE_DELAY_MS,
        }
    }

    /// Delay in milliseconds before showing; the pointer must hover at least
    /// this long. Default: 400ms.
    pub fn open_delay(mut self, delay_ms: f64) -> Self {
        self.open_delay = delay_ms;
        self
    }

    /// Grace period in milliseconds after the pointer leaves both trigger and
    /// tooltip. Default: 150ms.
    pub fn close_delay(mut self, delay_ms: f64) -> Self {
        self.close_delay = delay_ms;
        self
    }

    /// Show the tooltip anchored below the trigger, clamped to the viewport.
    pub fn show(self, ui: &Ui, trigger: &Response, add_contents: impl FnOnce(&mut Ui)) {
        let ctx = ui.ctx();
        let now = ui.input(|i| i.time);
        let open_delay_secs = self.open_delay / 1000.0;
        let close_delay_secs = self.close_delay / 1000.0;
        let trigger_hovered = trigger.hovered();

        if trigger.rect.is_negative() {
            return;
        }

        // Key on the trigger id namespaced by the tooltip id: a constant id reused
        // across triggers (e.g. a toolbar in a loop) must not share hover state.
        let state_key = state_key(trigger.id, self.id);
        let mut state: State = ctx.data(|d| d.get_temp::<State>(state_key)).unwrap_or_default();

        if trigger_hovered {
            state.hover_start = Some(state.hover_start.unwrap_or(now));
            state.grace_until = None;
        } else {
            state.hover_start = None;
        }

        if !trigger_hovered && state.visible_last_frame && state.grace_until.is_none() {
            state.grace_until = Some(now + close_delay_secs);
        }

        let will_open = if state.visible_last_frame {
            trigger_hovered
        } else {
            trigger_hovered
                && state.hover_start.map(|t| now - t >= open_delay_secs).unwrap_or(false)
        };

        let in_grace = state.grace_until.map(|t| now <= t).unwrap_or(false);
        let prelim_open = will_open || in_grace;

        let mut tooltip_hovered = false;
        if prelim_open {
            let t = theme(ui);
            let s = crate::spatial(ui);
            let screen_rect = ctx.viewport_rect();
            let gap = s.space_2;
            let mut desired_pos = trigger.rect.left_bottom() + Vec2::new(0.0, gap);

            // Rough horizontal clamp so we don't spawn off-screen.
            let est_min_width = 40.0;
            let est_max_width = (screen_rect.width() * 0.35).min(320.0).max(est_min_width);
            desired_pos.x = desired_pos
                .x
                .max(screen_rect.min.x + s.space_2)
                .min(screen_rect.max.x - est_max_width - s.space_2);
            desired_pos.y = desired_pos.y.min(screen_rect.max.y - s.space_2);

            let area = Area::new(self.id.with("area")).order(Order::Tooltip).fixed_pos(desired_pos);

            let inner = area.show(ctx, |ui| {
                ui.set_max_width(est_max_width);
                egui::Frame::new()
                    .fill(t.components.tooltip.bg)
                    .stroke(Stroke::new(STROKE_WIDTH, t.components.tooltip.border))
                    .corner_radius(CornerRadius::same(RADIUS_M as u8))
                    .inner_margin(Margin::same(s.space_3 as i8))
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
            d.insert_temp::<State>(state_key, state);
        });

        // Keep repainting while a timer is pending so delays and grace resolve.
        if is_open || was_visible {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}

/// Tooltip state key: trigger id namespaced by the tooltip id, giving each
/// trigger independent hover/grace state.
fn state_key(trigger_id: Id, tooltip_id: Id) -> Id {
    trigger_id.with(tooltip_id)
}

/// Render a simple themed text tooltip anchored to `trigger`.
pub fn text_tooltip(ui: &Ui, id: Id, trigger: &Response, text: &str) {
    Tooltip::new(id).show(ui, trigger, |ui| {
        ui.label(text);
    });
}

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
        let t = Tooltip::new(Id::new("test")).open_delay(100.0).close_delay(50.0);
        assert_eq!(t.open_delay, 100.0);
        assert_eq!(t.close_delay, 50.0);
    }

    /// Regression: state used to be keyed on the caller-supplied id alone, so two
    /// triggers sharing a constant tooltip id shared one hover state.
    #[test]
    fn state_key_is_per_trigger() {
        let tooltip = Id::new("shared_tooltip");
        let trigger_a = Id::new("trigger_a");
        let trigger_b = Id::new("trigger_b");

        assert_ne!(
            state_key(trigger_a, tooltip),
            state_key(trigger_b, tooltip),
            "distinct triggers sharing one tooltip id must not share state"
        );
        assert_eq!(
            state_key(trigger_a, tooltip),
            state_key(trigger_a, tooltip),
            "the same trigger must resolve a stable key across frames"
        );
    }
}
