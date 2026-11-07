//! Anchored Popover: a floating panel anchored to a trigger element.
//!
//! Open state lives in `egui::Memory` keyed by the popover `Id`. The popover
//! registers with the [`crate::widget::overlay`] coordination layer at
//! [`OverlayLayer::Popover`] priority, so Escape and click-outside are consumed
//! by exactly one overlay (the topmost), and it restores focus to the
//! previously-focused widget when it closes — the [`crate::widget::dialog`]
//! focus-restore pattern.
//!
//! ```ignore
//! Popover::new("my_popover").below().show(ui, trigger_response, |ui| {
//!     ui.label("content");
//! });
//! // Externally controlled: Popover::new("filter_popover").open(&ctx, true);
//! ```

use egui::{Context, CornerRadius, Id, Order, Pos2, Rect, Response, Ui};

use crate::spatial;
use crate::tokens::spatial::{RADIUS_M, STROKE_WIDTH};
use crate::widget::IntoWidgetId;
use crate::widget::overlay::{
    OverlayLayer, clicked_outside, escape_pressed, is_topmost, push_overlay, remove_overlay,
};

/// Per-popover open-state key suffix.
const OPEN_KEY: &str = "__popover_open";
/// Per-popover saved-focus key suffix.
const PREV_FOCUS_KEY: &str = "__popover_prev_focus";
/// Per-popover "opened-this-instance" key, so focus is not re-saved each frame.
const OPENED_KEY: &str = "__popover_opened";
/// Per-popover "closing-this-instance" key.
const CLOSING_KEY: &str = "__popover_closing";
/// Set only by [`Popover::open`]; marks the caller as owner of the open state.
/// Kept separate from `OPEN_KEY`, which the auto-toggle path also writes.
const EXTERNAL_KEY: &str = "__popover_external";

/// Response returned by [`Popover::show`].
pub struct PopoverResponse {
    /// Whether the popover is open after this frame.
    pub is_open: bool,
    /// Content rect in screen space; [`Rect::NOTHING`] when closed.
    pub popover_rect: Rect,
}

/// Which side of the trigger the popover opens on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PopoverDirection {
    /// Below the trigger (default).
    #[default]
    Below,
    /// Above the trigger.
    Above,
    /// To the right of the trigger.
    Right,
    /// To the left of the trigger.
    Left,
}

/// An anchored, immediate-mode floating popover panel.
///
/// State is managed in `egui::Memory` when the trigger is clicked, or by
/// [`open`](Self::open) for external control. Participates in
/// [`OverlayLayer::Popover`] coordination and restores focus on close.
#[derive(Clone, Debug)]
pub struct Popover {
    id: Id,
    direction: PopoverDirection,
    gap: f32,
    max_width: Option<f32>,
    arrow: bool,
}

impl Popover {
    /// Create a popover; `id` must be unique among open overlays in the viewport.
    pub fn new(id: impl IntoWidgetId) -> Self {
        Self {
            id: id.into_widget_id(),
            direction: PopoverDirection::default(),
            gap: 8.0,
            max_width: None,
            arrow: false,
        }
    }

    /// Anchor the popover below the trigger.
    pub fn below(mut self) -> Self {
        self.direction = PopoverDirection::Below;
        self
    }

    /// Anchor the popover above the trigger.
    pub fn above(mut self) -> Self {
        self.direction = PopoverDirection::Above;
        self
    }

    /// Anchor the popover to the right of the trigger.
    pub fn right(mut self) -> Self {
        self.direction = PopoverDirection::Right;
        self
    }

    /// Anchor the popover to the left of the trigger.
    pub fn left(mut self) -> Self {
        self.direction = PopoverDirection::Left;
        self
    }

    /// Gap in points between trigger and popover edges. Default: `8.0`.
    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    /// Maximum width; `None` sizes to content. Default: `None`.
    pub fn max_width(mut self, max_width: f32) -> Self {
        self.max_width = Some(max_width);
        self
    }

    /// Draw a small triangular arrow pointing at the trigger.
    /// Default: `false`.
    pub fn with_arrow(mut self, arrow: bool) -> Self {
        self.arrow = arrow;
        self
    }

    /// Open/close programmatically. After this the caller owns the state and
    /// the trigger click no longer toggles.
    pub fn open(&self, ctx: &Context, open: bool) {
        ctx.data_mut(|data| {
            data.insert_temp(open_key(self.id), open);
            data.insert_temp(external_key(self.id), true);
        });
    }

    /// Return `true` if the popover is currently open, `false` otherwise.
    pub fn is_open(ctx: &Context, id: impl IntoWidgetId) -> bool {
        let id = id.into_widget_id();
        ctx.data(|data| data.get_temp::<bool>(open_key(id)).unwrap_or(false))
    }

    /// Force-close the popover.
    pub fn close(&self, ctx: &Context) {
        Self::close_by_id(ctx, self.id);
    }

    /// Close the popover by its `Id`.
    pub fn close_by_id(ctx: &Context, id: Id) {
        ctx.data_mut(|data| {
            data.insert_temp(open_key(id), false);
            data.insert_temp(closing_key(id), true);
        });
    }

    /// Render the popover, toggling on `trigger_response.clicked()` unless the
    /// state is externally controlled. `content` receives the themed inner `Ui`.
    ///
    /// Returns [`PopoverResponse`].
    pub fn show(
        &self,
        ui: &mut Ui,
        trigger_response: &Response,
        content: impl FnOnce(&mut Ui),
    ) -> PopoverResponse {
        let ctx = ui.ctx();
        let screen_rect = ctx.viewport_rect();
        let is_open_key = open_key(self.id);
        let prev_focus_key = prev_focus_key(self.id);
        let opened_key = opened_key(self.id);
        let closing_key = closing_key(self.id);

        let mut is_open: bool = ctx.data(|d| d.get_temp::<bool>(is_open_key).unwrap_or(false));
        let mut is_closing: bool = ctx.data(|d| d.get_temp::<bool>(closing_key).unwrap_or(false));

        // Only `.open()` marks the caller as owner; auto-toggle must not set this,
        // or the trigger could never reopen the popover after the first open.
        let externally_controlled: bool =
            ctx.data(|d| d.get_temp::<bool>(external_key(self.id)).unwrap_or(false));

        if externally_controlled {
            if !is_open && is_closing {
                finish_close(ctx, self.id, prev_focus_key, opened_key, closing_key);
            }
        } else if trigger_response.clicked() {
            if is_open && !is_closing {
                is_closing = true;
                ctx.data_mut(|d| d.insert_temp(closing_key, true));
            } else {
                is_open = true;
                is_closing = false;
                save_focus(ctx, prev_focus_key);
                push_overlay(ctx, self.id, OverlayLayer::Popover);
                ctx.data_mut(|d| {
                    d.insert_temp(is_open_key, true);
                    d.insert_temp(opened_key, true);
                });
            }
        }

        if is_open && !is_closing && is_topmost(ctx, self.id) && escape_pressed(ctx, self.id) {
            is_open = false;
            is_closing = true;
            ctx.data_mut(|d| {
                d.insert_temp(is_open_key, false);
                d.insert_temp(closing_key, true);
            });
        }

        let _s = spatial(ui);
        let popover_rect = if is_open {
            // Anchor clamped to the viewport so `fixed_pos` is already in bounds.
            let anchor_pos = compute_anchor_pos_clamped(
                trigger_response,
                self.direction,
                self.gap,
                screen_rect,
                self.max_width.unwrap_or(300.0) + 24.0, // width + frame inner margin
            );

            let area_id = self.id.with("__area");
            let area = egui::Area::new(area_id).fixed_pos(anchor_pos).order(Order::Foreground);

            let inner = area.show(ctx, |ui| {
                if let Some(mw) = self.max_width {
                    ui.set_max_width(mw);
                }
                let t = crate::tokens::theme::theme(ui);
                popover_frame(&t).show(ui, content).inner
            });

            let rect = inner.response.rect;

            if is_open && is_topmost(ctx, self.id) && clicked_outside(ui, rect, &inner.response) {
                is_open = false;
                is_closing = true;
                ctx.data_mut(|d| {
                    d.insert_temp(is_open_key, false);
                    d.insert_temp(closing_key, true);
                });
            }

            rect
        } else {
            Rect::NOTHING
        };

        if is_closing {
            finish_close(ctx, self.id, prev_focus_key, opened_key, closing_key);
        }

        PopoverResponse {
            is_open,
            popover_rect,
        }
    }
}

/// Save the currently focused widget Id to Memory.
fn save_focus(ctx: &Context, key: Id) {
    // Read focus before writing data: both live in Memory's same RwLock, so
    // nesting `ctx.memory` inside `ctx.data_mut` would deadlock.
    let focused = ctx.memory(|m| m.focused());
    ctx.data_mut(|d| {
        d.insert_temp(key, focused);
    });
}

/// Restore focus, remove overlay, and clean up state keys.
fn finish_close(
    ctx: &Context,
    popover_id: Id,
    prev_focus_key: Id,
    opened_key: Id,
    closing_key: Id,
) {
    remove_overlay(ctx, popover_id);

    // Read and clear the saved focus before requesting it, for the same
    // Memory-lock reason as `save_focus`.
    let saved = ctx.data_mut(|d| {
        let saved = d.get_temp::<Option<Id>>(prev_focus_key).as_ref().and_then(|o| *o);
        d.remove::<bool>(opened_key);
        d.remove::<bool>(closing_key);
        d.remove::<Option<Id>>(prev_focus_key);
        saved
    });
    // Nothing had focus: `saved` is None and this is a no-op.
    if let Some(saved) = saved {
        ctx.memory_mut(|m| m.request_focus(saved));
    }
}

fn open_key(id: Id) -> Id {
    id.with(OPEN_KEY)
}
fn prev_focus_key(id: Id) -> Id {
    id.with(PREV_FOCUS_KEY)
}
fn opened_key(id: Id) -> Id {
    id.with(OPENED_KEY)
}
fn closing_key(id: Id) -> Id {
    id.with(CLOSING_KEY)
}
fn external_key(id: Id) -> Id {
    id.with(EXTERNAL_KEY)
}

/// Compute the anchor position, clamped so the Area's `fixed_pos` is in bounds.
fn compute_anchor_pos_clamped(
    trigger_response: &Response,
    direction: PopoverDirection,
    gap: f32,
    screen_rect: Rect,
    estimated_width: f32,
) -> Pos2 {
    let rect = trigger_response.rect;
    let mut pos = match direction {
        PopoverDirection::Below => Pos2::new(rect.left(), rect.bottom() + gap),
        PopoverDirection::Above => Pos2::new(rect.left(), rect.top() - gap),
        PopoverDirection::Right => Pos2::new(rect.right() + gap, rect.top()),
        PopoverDirection::Left => Pos2::new(rect.left() - gap, rect.top()),
    };

    let est_right = pos.x + estimated_width;
    if est_right > screen_rect.max.x {
        pos.x -= est_right - screen_rect.max.x;
    }
    if pos.x < screen_rect.min.x {
        pos.x = screen_rect.min.x;
    }
    if pos.y < screen_rect.min.y {
        pos.y = screen_rect.min.y;
    }
    pos
}

/// Themed popover body frame: floating-card surface fill, border stroke,
/// [`RADIUS_M`] corners, and raised elevation shadow.
fn popover_frame(t: &crate::tokens::theme::Theme) -> egui::Frame {
    egui::Frame::new()
        .fill(t.palette.surface.floating_card_bg)
        .stroke(egui::Stroke::new(STROKE_WIDTH, t.palette.border.default))
        .corner_radius(CornerRadius::same(RADIUS_M as u8))
        .inner_margin(egui::Margin::same(12))
        .shadow(t.elevation.raised)
}

#[cfg(test)]
mod tests {
    use egui::Sense;

    use super::*;
    use crate::widget::overlay::{OverlayLayer, is_topmost, push_overlay, remove_overlay};

    #[test]
    fn builder_fields() {
        let p = Popover::new("test");
        assert_eq!(p.direction, PopoverDirection::Below, "defaults: direction");
        assert!((p.gap - 8.0).abs() < f32::EPSILON, "defaults: gap");
        assert!(p.max_width.is_none(), "defaults: max_width");
        assert!(!p.arrow, "defaults: arrow");

        assert_eq!(Popover::new("test").below().direction, PopoverDirection::Below, "below");
        assert_eq!(Popover::new("test").above().direction, PopoverDirection::Above, "above");
        assert_eq!(Popover::new("test").right().direction, PopoverDirection::Right, "right");
        assert_eq!(Popover::new("test").left().direction, PopoverDirection::Left, "left");
        assert!((Popover::new("test").gap(16.0).gap - 16.0).abs() < f32::EPSILON, "gap");
        assert_eq!(Popover::new("test").max_width(200.0).max_width, Some(200.0), "max_width");
        assert!(Popover::new("test").with_arrow(true).arrow, "with_arrow");
    }

    #[test]
    fn is_open_defaults_to_false() {
        let ctx = Context::default();
        assert!(!Popover::is_open(&ctx, "nonexistent"));
    }

    #[test]
    fn is_open_reflects_open_call() {
        let ctx = Context::default();
        let popover = Popover::new("p1");
        assert!(!Popover::is_open(&ctx, "p1"));
        popover.open(&ctx, true);
        assert!(Popover::is_open(&ctx, "p1"));
    }

    #[test]
    fn is_open_reflects_close_call() {
        let ctx = Context::default();
        let popover = Popover::new("p2");
        popover.open(&ctx, true);
        popover.close(&ctx);
        assert!(!Popover::is_open(&ctx, "p2"));
    }

    #[test]
    fn is_open_reflects_toggle() {
        let ctx = Context::default();
        let popover = Popover::new("p3");

        popover.open(&ctx, true);
        assert!(Popover::is_open(&ctx, "p3"));

        popover.open(&ctx, false);
        assert!(!Popover::is_open(&ctx, "p3"));

        popover.open(&ctx, true);
        assert!(Popover::is_open(&ctx, "p3"));
    }

    /// Regression: the auto-toggle path wrote `OPEN_KEY`, which used to double as
    /// the external-control signal, so a trigger could open the popover only once.
    #[test]
    fn auto_toggle_open_does_not_mark_external_control() {
        let ctx = Context::default();
        let popover = Popover::new("auto_reopen");

        ctx.data_mut(|d| d.insert_temp(open_key(popover.id), true));
        assert!(
            !ctx.data(|d| d.get_temp::<bool>(external_key(popover.id)).unwrap_or(false)),
            "auto-toggle must not mark the popover as externally controlled"
        );

        popover.open(&ctx, true);
        assert!(
            ctx.data(|d| d.get_temp::<bool>(external_key(popover.id)).unwrap_or(false)),
            "open() marks the caller as the owner"
        );
    }

    #[test]
    fn open_registers_with_overlay() {
        let ctx = Context::default();
        let popover = Popover::new("ol1");

        // Mirrors what `show()` does on closed→open.
        push_overlay(&ctx, popover.id, OverlayLayer::Popover);
        assert!(is_topmost(&ctx, popover.id), "popover should be topmost after push");

        remove_overlay(&ctx, popover.id);
        assert!(!is_topmost(&ctx, popover.id), "popover should not be topmost after removal");
    }

    #[test]
    fn close_removes_from_overlay() {
        let ctx = Context::default();
        let popover = Popover::new("ol2");
        push_overlay(&ctx, popover.id, OverlayLayer::Popover);
        assert!(is_topmost(&ctx, popover.id), "popover should be topmost after push");

        remove_overlay(&ctx, popover.id);
        assert!(!is_topmost(&ctx, popover.id), "popover should not be topmost after removal");
    }

    #[test]
    fn multiple_popovers_coordinate() {
        let ctx = Context::default();
        let p1 = Popover::new("mp1");
        let p2 = Popover::new("mp2");

        push_overlay(&ctx, p1.id, OverlayLayer::Popover);
        push_overlay(&ctx, p2.id, OverlayLayer::Popover);

        assert!(is_topmost(&ctx, p2.id));
        assert!(!is_topmost(&ctx, p1.id));

        remove_overlay(&ctx, p2.id);
        assert!(is_topmost(&ctx, p1.id), "p1 should become topmost after removing p2");
    }

    /// `show()` snapshots the currently focused widget Id into Memory.
    #[test]
    fn show_saves_focus_on_open() {
        let ctx = Context::default();

        let viewport_rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
        let mut ui = Ui::new(
            ctx.clone(),
            Id::unique("popover_focus_test"),
            egui::UiBuilder::new().max_rect(viewport_rect),
        );

        // Register a focusable widget so `m.focused()` becomes `Some(..)`.
        let dummy_id = Id::unique("dummy_focus_target");
        ui.interact(
            Rect::from_min_max(Pos2::new(100.0, 100.0), Pos2::new(200.0, 130.0)),
            dummy_id,
            Sense::click(),
        );

        let trigger_response = ui.interact(
            Rect::from_min_max(Pos2::new(10.0, 10.0), Pos2::new(60.0, 40.0)),
            Id::unique("trigger"),
            Sense::click(),
        );

        let popover = Popover::new("focus_popover").below();
        popover.open(&ctx, true);

        let _resp = popover.show(&mut ui, &trigger_response, |_| {});

        let focused_at_show = ctx.memory(|m| m.focused());
        ctx.data(|d| {
            let saved: Option<Id> = d.get_temp(prev_focus_key(popover.id)).unwrap_or(None);
            assert_eq!(
                saved, focused_at_show,
                "save_focus should snapshot m.focused() at show() time"
            );
        });
    }

    /// After closing, the overlay is removed and the open flag is cleared.
    #[test]
    fn show_cleans_up_on_close() {
        let ctx = Context::default();
        let viewport_rect = Rect::from_min_max(Pos2::ZERO, Pos2::new(400.0, 300.0));
        let mut ui = Ui::new(
            ctx.clone(),
            Id::unique("popover_cleanup_test"),
            egui::UiBuilder::new().max_rect(viewport_rect),
        );

        let trigger_response = ui.interact(
            Rect::from_min_max(Pos2::new(10.0, 10.0), Pos2::new(60.0, 40.0)),
            Id::unique("trigger3"),
            Sense::click(),
        );

        let popover = Popover::new("cleanup_popover").below();

        popover.open(&ctx, true);
        let resp1 = popover.show(&mut ui, &trigger_response, |_| {});
        assert!(resp1.is_open, "popover should be open after frame 1");

        popover.open(&ctx, false);
        let resp2 = popover.show(&mut ui, &trigger_response, |_| {});
        assert!(!resp2.is_open, "popover should be closed after frame 2");

        ctx.data(|d| {
            let still_open: Option<bool> = d.get_temp(open_key(popover.id));
            assert!(
                still_open.is_none() || !still_open.unwrap_or(false),
                "open state flag should be absent or false after close"
            );
        });
    }
}
